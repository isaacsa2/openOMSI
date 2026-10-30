//! FSR 2's shaders without a GPU: every module parses and validates, translates for every
//! backend the renderer runs on, and its bindings are exactly its pass's bind group layout.

use super::*;

fn validated(name: &str, src: &str) -> (naga::Module, naga::valid::ModuleInfo) {
    let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    (module, info)
}

/// The module of each pass of `layout_entries`, in its order.
fn pass_sources() -> Vec<String> {
    let s = sources();
    let by = |n: &str| s.iter().find(|(name, _)| *name == n).expect(n).1.clone();
    vec![by("luminance pyramid"), by("luminance tail"), by("reconstruct previous depth"), by("depth clip"), by("lock"), by("accumulate"), by("accumulate"), by("rcas")]
}

/// Valid with no optional capability (no f16, no subgroups, no storage texture atomics), and
/// translatable to SPIR-V (Vulkan), HLSL (DirectX 12), MSL (Metal) and GLSL ES 3.1 (OpenGL,
/// ANGLE) with the pipeline constant set.
#[test]
fn the_shaders_validate_and_translate_for_every_backend() {
    let layouts = layout_entries();
    for (i, src) in pass_sources().into_iter().enumerate() {
        let (name, entry, _) = &layouts[i];
        let (module, info) = validated(name, &src);
        let constants = naga::back::PipelineConstants::from_iter([("HDR_COLOR_INPUT".to_string(), 0.0)]);
        let (module, info) = naga::back::pipeline_constants::process_overrides(&module, &info, None, &constants).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        naga::back::spv::write_vec(&module, &info, &Default::default(), None).unwrap_or_else(|e| panic!("{name} SPIR-V: {e:?}"));
        let mut hlsl = String::new();
        naga::back::hlsl::Writer::new(&mut hlsl, &naga::back::hlsl::Options { shader_model: naga::back::hlsl::ShaderModel::V5_1, ..Default::default() }, &Default::default())
            .write(&module, &info, None)
            .unwrap_or_else(|e| panic!("{name} HLSL: {e:?}"));
        let mut glsl = String::new();
        let options = naga::back::glsl::Options { version: naga::back::glsl::Version::Embedded { version: 310, is_webgl: false }, ..Default::default() };
        let pipeline = naga::back::glsl::PipelineOptions { shader_stage: naga::ShaderStage::Compute, entry_point: entry.to_string(), multiview: None };
        naga::back::glsl::Writer::new(&mut glsl, &module, &info, &options, &pipeline, naga::proc::BoundsCheckPolicies::default())
            .and_then(|mut w| w.write())
            .unwrap_or_else(|e| panic!("{name} GLSL ES 3.1: {e:?}"));
    }
}

/// The bindings an entry point uses are its pass's layout, entry for entry and kind for kind
/// (a uniform, a read-only or writable storage buffer, a sampled texture, a storage texture of
/// the same format, a sampler), and the layout holds nothing it does not use.
#[test]
fn every_pass_binds_what_its_shader_reads() {
    use naga::{AddressSpace as A, ImageClass, TypeInner};
    use wgpu::BindingType as B;
    let layouts = layout_entries();
    for (i, src) in pass_sources().into_iter().enumerate() {
        let (name, entry, entries) = &layouts[i];
        let (module, info) = validated(name, &src);
        let ep = module.entry_points.iter().position(|e| e.name == *entry).unwrap_or_else(|| panic!("{name}: no {entry}"));
        let ep_info = info.get_entry_point(ep);
        let mut used = Vec::new();
        for (handle, var) in module.global_variables.iter() {
            if ep_info[handle].is_empty() {
                continue;
            }
            let Some(binding) = &var.binding else { continue };
            assert_eq!(binding.group, 0, "{name}: {:?}", var.name);
            let layout = entries.iter().find(|e| e.binding == binding.binding).unwrap_or_else(|| panic!("{name}: {:?} (binding {}) is not in the layout", var.name, binding.binding));
            let ok = match (&var.space, &module.types[var.ty].inner, &layout.ty) {
                (A::Uniform, _, B::Buffer { ty: wgpu::BufferBindingType::Uniform, .. }) => true,
                (A::Storage { access }, _, B::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, .. }) => *read_only == !access.contains(naga::StorageAccess::STORE),
                (A::Handle, TypeInner::Image { class: ImageClass::Sampled { .. }, .. }, B::Texture { .. }) => true,
                (A::Handle, TypeInner::Image { class: ImageClass::Storage { format, .. }, .. }, B::StorageTexture { format: f, .. }) => {
                    matches!((format, f), (naga::StorageFormat::Rgba16Float, wgpu::TextureFormat::Rgba16Float) | (naga::StorageFormat::R32Float, wgpu::TextureFormat::R32Float) | (naga::StorageFormat::Rgba8Unorm, wgpu::TextureFormat::Rgba8Unorm))
                }
                (A::Handle, TypeInner::Sampler { comparison: false }, B::Sampler(wgpu::SamplerBindingType::Filtering)) => true,
                _ => false,
            };
            assert!(ok, "{name}: {:?} (binding {}) is not what the layout says ({:?})", var.name, binding.binding, layout.ty);
            used.push(binding.binding);
        }
        for e in entries {
            assert!(used.contains(&e.binding), "{name}: binding {} is in the layout but the shader does not use it", e.binding);
        }
        assert!(entries.iter().filter(|e| matches!(e.ty, B::StorageTexture { .. })).count() <= 4, "{name}: more storage textures than WebGPU's minimum");
    }
}

/// The dispatches cover the picture: SPD's 64 x 64 tiles, 8 x 8 groups for the render and
/// display passes, RCAS's 16 x 16.
#[test]
fn the_dispatches_cover_every_pixel() {
    for (render, display) in [((1280u32, 720u32), (1920u32, 1080u32)), ((853, 480), (1280, 720)), ((1, 1), (3, 3)), ((2560, 1440), (3840, 2160))] {
        let (gx, gy, n, mips) = params::spd_setup(render);
        assert!(gx * 64 >= render.0 && gy * 64 >= render.1 && n == gx * gy);
        assert!(mips <= 12 && (render.0.max(render.1) < 2 || 1u32 << mips <= render.0.max(render.1)));
        assert!(render.0.div_ceil(8) * 8 >= render.0 && display.1.div_ceil(16) * 16 >= display.1);
    }
}
