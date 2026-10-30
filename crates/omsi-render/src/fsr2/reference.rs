//! FSR 2 against AMD's own shaders, number for number (`OMSI_FSR2_REFERENCE`).
//!
//! The reference's GLSL passes (GPUOpen-Effects/FidelityFX-FSR2 v2.2.1; the folder
//! `src/ffx-fsr2-api/shaders` named by `OMSI_FSR2_REFERENCE`) are compiled with glslangValidator
//! in the 32-bit, reversed depth, low resolution motion vector permutation and run unchanged as
//! SPIR-V, on resources in the reference's own formats (R16F, RG16F, R8, R32_UINT with image
//! atomics ...), with the host side of ffx_fsr2.cpp; this port runs on the same device with
//! the same inputs; every frame's output and intermediates are compared.
//!
//! Three image format qualifiers of the GLSL copy are set to their resource's format, as the
//! reference's resource table and its HLSL (DirectX 12) path have them: the dilated depth r16f
//! to r32f (an R32_FLOAT resource), the prepared input colour rgba16 to rgba16f (an
//! R16G16B16A16_FLOAT one), the upscaled output (no qualifier) to rgba16f (the output's).
//!
//! The copy also defines D3D12's out of bounds behaviour, which the reference relies on (a load
//! outside a resource gives 0, a store there is dropped), and its bindings are renumbered
//! without gaps (`compact_bindings`): neither changes a value the passes compute.
//!
//! Knobs: `OMSI_FSR2_REFERENCE_SIZE=rw,rh,dw,dh` other sizes; `OMSI_FSR2_REFERENCE_DETAIL`
//! every intermediate compared; `OMSI_FSR2_REFERENCE_ONLY=port|reference` one side only (the
//! comparison then fails: the control that it compares something); `OMSI_FSR2_REFERENCE_UPTO=n`
//! the reference's first n passes; `OMSI_FSR2_REFERENCE_PATCH=file|from|to;;...` a text change
//! to the copy (to look at a value inside a pass).
//!
//! Needs a Vulkan adapter with pass-through SPIR-V, image atomics and the adapter's own format
//! table (Mesa's lavapipe has them): `OMSI_FSR2_REFERENCE=<shaders> cargo test -p omsi-render
//! --lib fsr2::reference -- --ignored --nocapture`.

use super::*;
use std::collections::HashMap;
use std::path::Path;

const PASSES: [&str; 7] = ["compute_luminance_pyramid", "reconstruct_previous_depth", "depth_clip", "lock", "accumulate", "accumulate_sharpen", "rcas"];

/// The reference's passes compiled to SPIR-V (None when glslangValidator is missing).
fn compile_reference(src: &Path, out: &Path) -> Option<HashMap<&'static str, Vec<u32>>> {
    std::fs::create_dir_all(out).ok()?;
    // the copy, with the three qualifiers set (see the module's notes)
    for e in std::fs::read_dir(src).ok()?.flatten() {
        let mut text = std::fs::read_to_string(e.path()).ok()?;
        if e.file_name() == "ffx_fsr2_callbacks_glsl.h" {
            for (from, to) in [
                ("FSR2_BIND_UAV_DILATED_DEPTH, r16f)", "FSR2_BIND_UAV_DILATED_DEPTH, r32f)"),
                ("FSR2_BIND_UAV_PREPARED_INPUT_COLOR, rgba16)", "FSR2_BIND_UAV_PREPARED_INPUT_COLOR, rgba16f)"),
                ("FSR2_BIND_UAV_UPSCALED_OUTPUT /* app controlled format */)", "FSR2_BIND_UAV_UPSCALED_OUTPUT, rgba16f)"),
            ] {
                assert!(text.contains(from), "the reference's {from} not found");
                text = text.replace(from, to);
            }
            // D3D12's (and Vulkan robustness's) out of bounds access, which the reference relies
            // on - its threads past the render size read and write outside their resources:
            // a load there gives 0, a store is dropped. Without it lavapipe read past the
            // images and crashed. (The load reads a clamped texel and multiplies it by 0 outside:
            // a `?:` of two loads became a select of both, which still read past the image.)
            let robust = "#if defined(FFX_GPU)\n#ifndef FFX_ROBUST_ACCESS\n#define FFX_ROBUST_ACCESS\n\
                #define texelFetch(t, p, l) (texelFetch(t, clamp(ivec2(p), ivec2(0), textureSize(t, l) - 1), l) * (all(lessThan(uvec2(p), uvec2(textureSize(t, l)))) ? 1 : 0))\n\
                #define imageLoad(i, p) (imageLoad(i, clamp(ivec2(p), ivec2(0), imageSize(i) - 1)) * (all(lessThan(uvec2(p), uvec2(imageSize(i)))) ? 1 : 0))\n\
                #define imageStore(i, p, v) if (all(lessThan(uvec2(p), uvec2(imageSize(i))))) imageStore(i, p, v)\n\
                #endif\n#endif\n";
            let at = text.find("#if defined(FFX_GPU)\n#ifndef FFX_FSR2_PREFER_WAVE64").expect("the callbacks' GPU part");
            text.insert_str(at, robust);
        }
        if let Ok(patch) = std::env::var("OMSI_FSR2_REFERENCE_PATCH") {
            for p in patch.split(";;") {
                let mut it = p.splitn(3, "|");
                if let (Some(f), Some(from), Some(to)) = (it.next(), it.next(), it.next()) {
                    if e.file_name() == f {
                        assert!(text.contains(from), "patch: {from} not in {f}");
                        text = text.replace(from, to);
                    }
                }
            }
        }
        std::fs::write(out.join(e.file_name()), text).ok()?;
    }
    let mut spv = HashMap::new();
    for pass in PASSES {
        let (file, sharpen) = match pass {
            "accumulate_sharpen" => ("accumulate", 1),
            p => (p, 0),
        };
        let o = out.join(format!("{pass}.spv"));
        let status = std::process::Command::new("glslangValidator")
            .args(["-V", "--target-env", "vulkan1.1", "-S", "comp", "-DFFX_GPU=1", "-DFFX_GLSL=1", "-DFFX_HALF=0"])
            .args(["-DFFX_FSR2_OPTION_HDR_COLOR_INPUT=0", "-DFFX_FSR2_OPTION_LOW_RESOLUTION_MOTION_VECTORS=1", "-DFFX_FSR2_OPTION_JITTERED_MOTION_VECTORS=0"])
            .args(["-DFFX_FSR2_OPTION_INVERTED_DEPTH=1", "-DFFX_FSR2_OPTION_REPROJECT_USE_LANCZOS_TYPE=0", "-DFFX_FSR2_OPTION_UPSAMPLE_USE_LANCZOS_TYPE=2"])
            .arg(format!("-DFFX_FSR2_OPTION_APPLY_SHARPENING={sharpen}"))
            .arg("-o")
            .arg(&o)
            .arg(out.join(format!("ffx_fsr2_{file}_pass.glsl")))
            .output()
            .ok()?;
        assert!(status.status.success(), "{pass}: {}", String::from_utf8_lossy(&status.stdout));
        let bytes = std::fs::read(&o).ok()?;
        spv.insert(pass, bytes.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect());
    }
    Some(spv)
}

/// A resource variable of a SPIR-V module: its name, set, binding and kind.
#[derive(Debug, Clone)]
struct SpvBinding {
    name: String,
    set: u32,
    binding: u32,
    kind: SpvKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SpvKind {
    Uniform,
    Sampler,
    /// sampled: 1 a sampled image, 2 a storage one; `uint` its component type; `format` the
    /// SPIR-V image format (0 Unknown)
    Image { sampled: u32, uint: bool, format: u32 },
}

/// The module's bindings renumbered 0, 1, 2... in each set, in their order. wgpu's Vulkan
/// backend numbers a layout's descriptors so (its own shaders are renumbered to match by naga);
/// a pass-through module keeps its numbers, and past a gap in them it would read the next
/// descriptor - the reference's are not contiguous (its reconstruct pass has no binding 10).
fn compact_bindings(words: &mut [u32]) {
    let (mut sets, mut at) = (HashMap::new(), Vec::new());
    let mut i = 5;
    while i < words.len() {
        let (len, op) = ((words[i] >> 16) as usize, words[i] & 0xffff);
        if op == 71 && words[i + 2] == 34 {
            sets.insert(words[i + 1], words[i + 3]);
        }
        if op == 71 && words[i + 2] == 33 {
            at.push((words[i + 1], i + 3));
        }
        i += len;
    }
    let mut used: HashMap<u32, Vec<u32>> = HashMap::new();
    for (id, pos) in &at {
        used.entry(sets.get(id).copied().unwrap_or(0)).or_default().push(words[*pos]);
    }
    for v in used.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    for (id, pos) in at {
        let set = sets.get(&id).copied().unwrap_or(0);
        words[pos] = used[&set].iter().position(|b| *b == words[pos]).expect("collected") as u32;
    }
}

/// The resource variables of a module (OpName, OpDecorate, OpVariable and the types).
fn reflect(words: &[u32]) -> Vec<SpvBinding> {
    let (mut names, mut sets, mut bindings) = (HashMap::new(), HashMap::new(), HashMap::new());
    let (mut pointers, mut images, mut samplers, mut structs, mut ints, mut vars) = (HashMap::new(), HashMap::new(), Vec::new(), Vec::new(), HashMap::new(), Vec::new());
    let mut i = 5;
    while i < words.len() {
        let (len, op) = ((words[i] >> 16) as usize, words[i] & 0xffff);
        let w = &words[i..i + len];
        match op {
            5 => {
                let bytes: Vec<u8> = w[2..].iter().flat_map(|x| x.to_le_bytes()).take_while(|b| *b != 0).collect();
                names.insert(w[1], String::from_utf8_lossy(&bytes).into_owned());
            }
            71 if w[2] == 34 => {
                sets.insert(w[1], w[3]);
            }
            71 if w[2] == 33 => {
                bindings.insert(w[1], w[3]);
            }
            21 => {
                ints.insert(w[1], ());
            }
            25 => {
                images.insert(w[1], (w[2], w[7], w[8]));
            }
            26 => samplers.push(w[1]),
            30 => structs.push(w[1]),
            32 => {
                pointers.insert(w[1], w[3]);
            }
            59 => vars.push((w[2], w[1])),
            _ => {}
        }
        i += len;
    }
    vars.into_iter()
        .filter_map(|(id, ptr)| {
            let (set, binding) = (*sets.get(&id)?, *bindings.get(&id)?);
            let ty = *pointers.get(&ptr)?;
            let kind = if let Some(&(component, sampled, format)) = images.get(&ty) {
                SpvKind::Image { sampled, uint: ints.contains_key(&component), format }
            } else if samplers.contains(&ty) {
                SpvKind::Sampler
            } else if structs.contains(&ty) {
                SpvKind::Uniform
            } else {
                return None;
            };
            Some(SpvBinding { name: names.get(&id).cloned().unwrap_or_default(), set, binding, kind })
        })
        .collect()
}

/// A texture of the reference's, with its view (mip `level`) and the view of all its mips.
struct Tex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

fn tex(device: &wgpu::Device, label: &str, size: (u32, u32), format: wgpu::TextureFormat, mips: u32) -> Tex {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Tex { texture, view }
}

fn mip_view(t: &Tex, level: u32) -> wgpu::TextureView {
    t.texture.create_view(&wgpu::TextureViewDescriptor { base_mip_level: level, mip_level_count: Some(1), ..Default::default() })
}

/// The reference's internal resources (internalSurfaceDesc of ffx_fsr2.cpp).
struct RefResources {
    prepared_input_color: Tex,
    reconstructed_previous_depth: Tex,
    dilated_motion_vectors: [Tex; 2],
    dilated_depth: Tex,
    lock_status: [Tex; 2],
    lock_input_luma: Tex,
    new_locks: Tex,
    internal_upscaled: [Tex; 2],
    scene_luminance: Tex,
    scene_luminance_mips: u32,
    luma_history: [Tex; 2],
    spd_atomic: Tex,
    dilated_reactive_masks: Tex,
    auto_exposure: Tex,
    default_exposure: Tex,
    default_reactive: Tex,
    dummy_lut: Tex,
    output: Tex,
}

impl RefResources {
    fn new(device: &wgpu::Device, render: (u32, u32), display: (u32, u32)) -> RefResources {
        use wgpu::TextureFormat as F;
        let half = (render.0 / 2, render.1 / 2);
        let mips = 32 - half.0.max(half.1).max(1).leading_zeros();
        let two = |label, size, f| [tex(device, label, size, f, 1), tex(device, label, size, f, 1)];
        RefResources {
            prepared_input_color: tex(device, "ref prepared", render, F::Rgba16Float, 1),
            reconstructed_previous_depth: tex(device, "ref reconstructed depth", render, F::R32Uint, 1),
            dilated_motion_vectors: two("ref dilated mv", render, F::Rg16Float),
            dilated_depth: tex(device, "ref dilated depth", render, F::R32Float, 1),
            lock_status: two("ref lock status", display, F::Rg16Float),
            lock_input_luma: tex(device, "ref lock input luma", render, F::R16Float, 1),
            new_locks: tex(device, "ref new locks", display, F::R8Unorm, 1),
            internal_upscaled: two("ref upscaled", display, F::Rgba16Float),
            scene_luminance: tex(device, "ref luminance", half, F::R16Float, mips),
            scene_luminance_mips: mips,
            luma_history: two("ref luma history", display, F::Rgba8Unorm),
            spd_atomic: tex(device, "ref spd atomic", (1, 1), F::R32Uint, 1),
            dilated_reactive_masks: tex(device, "ref reactive", render, F::Rg8Unorm, 1),
            auto_exposure: tex(device, "ref auto exposure", (1, 1), F::Rg32Float, 1),
            default_exposure: tex(device, "ref default exposure", (1, 1), F::Rg32Float, 1),
            default_reactive: tex(device, "ref default reactive", (1, 1), F::R8Unorm, 1),
            dummy_lut: tex(device, "ref lut (unused)", (1, 1), F::R32Float, 1),
            output: tex(device, "ref output", display, F::Rgba16Float, 1),
        }
    }
}

/// The frame's inputs of the comparison: a picture panning, a square moving over it.
struct Frame {
    color: Vec<f32>,
    depth: Vec<f32>,
    motion: Vec<f32>,
}

fn synthetic(render: (u32, u32), t: u32, jitter: [f32; 2]) -> Frame {
    let (w, h) = (render.0 as usize, render.1 as usize);
    let (pan, speed) = (0.5f32, 1.3f32);
    let (mut color, mut depth, mut motion) = (Vec::with_capacity(w * h * 4), Vec::with_capacity(w * h), Vec::with_capacity(w * h * 2));
    for y in 0..h {
        for x in 0..w {
            // where the pixel's sample lies (jittered), in the world and on the square
            let (sx, sy) = (x as f32 + 0.5 - jitter[0], y as f32 + 0.5 - jitter[1]);
            let wx = sx + pan * t as f32;
            let left = 10.0 + (speed - pan) * t as f32;
            let on_square = sx >= left && sx < left + 20.0 && sy >= 20.0 && sy < 40.0;
            let (r, g, b, d, mv) = if on_square {
                (0.9, 0.85 * (0.5 + 0.5 * (sy * 0.9).sin()), 0.1, 0.6f32, -(speed - pan))
            } else {
                let checker = if ((wx * 0.5).floor() as i32 + (sy * 0.5).floor() as i32) % 2 == 0 { 0.8 } else { 0.2 };
                (checker * (0.5 + 0.5 * (wx * 0.11).sin()), 0.3 + 0.2 * (sy * 0.07).cos(), checker * 0.6, 0.2f32, pan)
            };
            color.extend([r, g, b, 1.0]);
            depth.push(d);
            // last frame's UV minus this one's
            motion.extend([mv / w as f32, 0.0]);
        }
    }
    Frame { color, depth, motion }
}

fn f16_bits(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let a = b & 0x7fff_ffff;
    if a < 0x3880_0000 {
        return sign | ((f32::from_bits(a) / 5.960_464_5e-8).round_ties_even() as u16);
    }
    let r = (a + 0xfff + ((a >> 13) & 1)) & 0xffff_e000;
    sign | (((r - 0x3800_0000) >> 13) as u16)
}

fn f16_value(h: u16) -> f32 {
    let (sign, exp, man) = ((h >> 15) as u32, ((h >> 10) & 0x1f) as i32, (h & 0x3ff) as f32);
    let v = match exp {
        0 => man * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + man / 1024.0) * 2f32.powi(e - 15),
    };
    if sign == 1 { -v } else { v }
}

fn upload(queue: &wgpu::Queue, t: &wgpu::Texture, data: &[u8], bytes_per_px: u32) {
    let (w, h) = (t.width(), t.height());
    queue.write_texture(t.as_image_copy(), data, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * bytes_per_px), rows_per_image: Some(h) }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
}

/// A texture read back as floats (RGBA16F: four halves a texel, RGBA8: four bytes).
fn read(device: &wgpu::Device, queue: &wgpu::Queue, t: &wgpu::Texture) -> Vec<f32> {
    let (w, h) = (t.width(), t.height());
    let bpp = t.format().block_copy_size(None).expect("plain format");
    let row = (w * bpp).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("read"), size: u64::from(row * h), usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(t.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    let data = buf.slice(..).get_mapped_range();
    let mut out = Vec::new();
    for y in 0..h as usize {
        let r = &data[y * row as usize..y * row as usize + (w * bpp) as usize];
        match t.format() {
            wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rg16Float | wgpu::TextureFormat::R16Float => out.extend(r.chunks_exact(2).map(|b| f16_value(u16::from_le_bytes([b[0], b[1]])))),
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rg8Unorm | wgpu::TextureFormat::R8Unorm => out.extend(r.iter().map(|b| *b as f32 / 255.0)),
            wgpu::TextureFormat::R32Float | wgpu::TextureFormat::Rg32Float | wgpu::TextureFormat::Rgba32Float => out.extend(r.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
            f => panic!("read {f:?}"),
        }
    }
    out
}

fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, value: [f64; 4]) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ref clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: value[0], g: value[1], b: value[2], a: value[3] }), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

/// The largest and the mean absolute difference of two equally long sets of values.
fn compare(a: &[f32], b: &[f32]) -> (f32, f32) {
    assert_eq!(a.len(), b.len());
    let (mut max, mut sum) = (0f32, 0f64);
    for (x, y) in a.iter().zip(b) {
        let d = (x - y).abs();
        max = max.max(d);
        sum += f64::from(d);
    }
    (max, (sum / a.len() as f64) as f32)
}

/// The reference's passes on `device`, and the resources they bind by name.
struct Reference {
    passes: HashMap<&'static str, (wgpu::ComputePipeline, Vec<wgpu::BindGroupLayout>, Vec<SpvBinding>)>,
    point: wgpu::Sampler,
    linear: wgpu::Sampler,
    cb: wgpu::Buffer,
    spd_cb: wgpu::Buffer,
    rcas_cb: wgpu::Buffer,
}

impl Reference {
    fn new(device: &wgpu::Device, spv: &HashMap<&'static str, Vec<u32>>) -> Reference {
        let mut passes = HashMap::new();
        for (pass, words) in spv {
            let mut words = words.clone();
            compact_bindings(&mut words);
            let mut bindings = reflect(&words);
            bindings.sort_by_key(|b| (b.set, b.binding));
            if std::env::var("OMSI_FSR2_REFERENCE_DETAIL").is_ok() {
                eprintln!("{pass}: {:?}", bindings.iter().map(|b| format!("{}@{}.{} {:?}", b.name, b.set, b.binding, b.kind)).collect::<Vec<_>>());
            }
            let mut layouts = Vec::new();
            for set in 0..2 {
                let entries: Vec<wgpu::BindGroupLayoutEntry> = bindings.iter().filter(|b| b.set == set).map(|b| {
                    let ty = match b.kind {
                        SpvKind::Uniform => wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                        SpvKind::Sampler => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        SpvKind::Image { sampled: 1, uint, .. } => wgpu::BindingType::Texture {
                            // (the 32-bit float ones cannot be filtered in WebGPU's terms; the
                            // reference samples none of them)
                            sample_type: if uint {
                                wgpu::TextureSampleType::Uint
                            } else {
                                wgpu::TextureSampleType::Float { filterable: !matches!(b.name.as_str(), "r_input_depth" | "r_dilatedDepth" | "r_lanczos_lut" | "r_upsample_maximum_bias_lut" | "r_input_exposure" | "r_auto_exposure") }
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        SpvKind::Image { .. } => wgpu::BindingType::StorageTexture { access: storage_access(&b.name), format: storage_format(&b.name), view_dimension: wgpu::TextureViewDimension::D2 },
                    };
                    wgpu::BindGroupLayoutEntry { binding: b.binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None }
                }).collect();
                layouts.push(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(pass), entries: &entries }));
            }
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(pass), bind_group_layouts: &[Some(&layouts[0]), Some(&layouts[1])], immediate_size: 0 });
            // SAFETY: glslang's own SPIR-V of the reference, its layout reflected from it
            let module = unsafe {
                device.create_shader_module_passthrough(wgpu::ShaderModuleDescriptorPassthrough { label: Some(pass), spirv: Some(std::borrow::Cow::Borrowed(&words)), ..Default::default() })
            };
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(pass), layout: Some(&pl), module: &module, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
            passes.insert(*pass, (pipeline, layouts, bindings));
        }
        let sampler = |filter| device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: filter, min_filter: filter, ..Default::default() });
        Reference {
            passes,
            point: sampler(wgpu::FilterMode::Nearest),
            linear: sampler(wgpu::FilterMode::Linear),
            cb: buffer(device, "ref cbFSR2", 128, wgpu::BufferUsages::UNIFORM),
            spd_cb: buffer(device, "ref cbSPD", 32, wgpu::BufferUsages::UNIFORM),
            rcas_cb: buffer(device, "ref cbRCAS", 16, wgpu::BufferUsages::UNIFORM),
        }
    }
}

/// The reference's UAVs that are read as well as written (the others write only).
fn storage_access(name: &str) -> wgpu::StorageTextureAccess {
    match name {
        "rw_reconstructed_previous_nearest_depth" | "rw_spd_global_atomic" => wgpu::StorageTextureAccess::Atomic,
        "rw_new_locks" | "rw_auto_exposure" | "rw_img_mip_shading_change" | "rw_img_mip_5" | "rw_lock_status" | "rw_luma_history" => wgpu::StorageTextureAccess::ReadWrite,
        _ => wgpu::StorageTextureAccess::WriteOnly,
    }
}

/// The reference's UAV formats (its resource table, ffx_fsr2.cpp).
fn storage_format(name: &str) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as F;
    match name {
        "rw_reconstructed_previous_nearest_depth" | "rw_spd_global_atomic" => F::R32Uint,
        "rw_dilated_motion_vectors" | "rw_lock_status" => F::Rg16Float,
        "rw_dilatedDepth" => F::R32Float,
        "rw_internal_upscaled_color" | "rw_prepared_input_color" | "rw_upscaled_output" => F::Rgba16Float,
        "rw_lock_input_luma" | "rw_img_mip_shading_change" | "rw_img_mip_5" => F::R16Float,
        "rw_new_locks" => F::R8Unorm,
        "rw_luma_history" => F::Rgba8Unorm,
        "rw_dilated_reactive_masks" => F::Rg8Unorm,
        "rw_auto_exposure" => F::Rg32Float,
        n => panic!("the reference's UAV {n} is not known here"),
    }
}

/// The comparison, frame by frame.
#[test]
#[ignore = "needs the reference's shaders (OMSI_FSR2_REFERENCE), glslangValidator and a Vulkan adapter with pass-through SPIR-V (lavapipe)"]
fn the_port_computes_what_the_reference_does() {
    let Ok(src) = std::env::var("OMSI_FSR2_REFERENCE") else {
        eprintln!("OMSI_FSR2_REFERENCE not set: skipped");
        return;
    };
    let out = std::env::temp_dir().join(format!("omsi-fsr2-reference-{}", std::process::id()));
    let spv = compile_reference(Path::new(&src), &out).expect("glslangValidator and the reference's shaders");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let wanted = wgpu::Features::PASSTHROUGH_SHADERS | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES | wgpu::Features::TEXTURE_ATOMIC;
    let adapter = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN)).into_iter().find(|a| a.features().contains(wanted)).expect("a Vulkan adapter with pass-through SPIR-V");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features: wanted, required_limits: adapter.limits(), ..Default::default() })).expect("device");
    // (OMSI_FSR2_REFERENCE_SIZE="rw,rh,dw,dh" another pair of sizes)
    let sizes: Vec<u32> = std::env::var("OMSI_FSR2_REFERENCE_SIZE").ok().map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect()).unwrap_or_default();
    let (render, display) = match sizes[..] {
        [rw, rh, dw, dh] => ((rw, rh), (dw, dh)),
        _ => ((96u32, 64u32), (144u32, 96u32)),
    };
    let reference = Reference::new(&device, &spv);
    let res = RefResources::new(&device, render, display);
    let mut port = Fsr2::new(&device, &queue, false);

    let input = |label, f| tex(&device, label, render, f, 1);
    let (color, depth, motion) = (input("color", wgpu::TextureFormat::Rgba16Float), input("depth", wgpu::TextureFormat::R32Float), input("motion", wgpu::TextureFormat::Rg16Float));
    upload(&queue, &res.default_exposure.texture, &[0u8; 8], 8);
    upload(&queue, &res.default_reactive.texture, &[0u8], 1);
    upload(&queue, &res.spd_atomic.texture, &[0u8; 4], 4);

    let mut history = params::History { first_execution: true, ..Default::default() };
    let mut worst = Vec::new();
    for t in 0..12u32 {
        let jitter = params::jitter_offset(t as i32, params::jitter_phase_count(render.0, display.0));
        let frame = synthetic(render, t, jitter);
        let halves = |v: &[f32]| v.iter().flat_map(|x| f16_bits(*x).to_le_bytes()).collect::<Vec<u8>>();
        upload(&queue, &color.texture, &halves(&frame.color), 8);
        upload(&queue, &motion.texture, &halves(&frame.motion), 4);
        upload(&queue, &depth.texture, &frame.depth.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>(), 4);
        let sharpness = if t % 2 == 0 { Some(0.8) } else { None };
        let reset = t == 7;

        // the port
        let only = std::env::var("OMSI_FSR2_REFERENCE_ONLY").unwrap_or_default();
        let mut enc = device.create_command_encoder(&Default::default());
        port.frame_jitter = Some(jitter);
        if only != "reference" { port.dispatch(&device, &queue, &mut enc, &Inputs { color: &color.view, depth: &depth.view, motion: &motion.view, render, display, jitter, near: 0.1, far: 500.0, fov_y: 1.0, frame_time_ms: 16.6, reset, sharpness }, None); }
        queue.submit([enc.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        eprintln!("frame {t}: port done");

        // the reference, as fsr2Dispatch does it
        let first = history.first_execution;
        let (constants, reset_accumulation) = params::dispatch_constants(&mut history, render, render, display, render, jitter, 0.1, 500.0, 1.0, 16.6, reset);
        queue.write_buffer(&reference.cb, 0, bytemuck::bytes_of(&constants));
        let (gx, gy, groups, mips) = params::spd_setup(render);
        queue.write_buffer(&reference.spd_cb, 0, bytemuck::bytes_of(&params::SpdConstants { mips, num_work_groups: groups, work_group_offset: [0, 0], render_size: [render.0, render.1], _pad: [0; 2] }));
        if let Some(s) = sharpness {
            queue.write_buffer(&reference.rcas_cb, 0, bytemuck::cast_slice(&params::rcas_config(s)));
        }
        let odd = history.resource_frame_index & 1 == 1;
        let (srv, uav) = if odd { (1, 0) } else { (0, 1) };
        let mut enc = device.create_command_encoder(&Default::default());
        if first {
            for v in [&res.lock_status[0].view, &res.lock_status[1].view, &res.prepared_input_color.view] {
                clear(&mut enc, v, [0.0; 4]);
            }
        }
        if reset_accumulation {
            clear(&mut enc, &res.lock_status[srv].view, [0.0; 4]);
            clear(&mut enc, &res.internal_upscaled[srv].view, [0.0; 4]);
            for m in 0..res.scene_luminance_mips {
                clear(&mut enc, &mip_view(&res.scene_luminance, m), [0.0; 4]);
            }
            clear(&mut enc, &res.auto_exposure.view, [-1.0, 1e8, 0.0, 0.0]);
        }
        let views: HashMap<&str, wgpu::TextureView> = [
            ("r_input_color_jittered", color.view.clone()),
            ("r_input_motion_vectors", motion.view.clone()),
            ("r_input_depth", depth.view.clone()),
            ("r_input_exposure", res.default_exposure.view.clone()),
            ("r_auto_exposure", res.auto_exposure.view.clone()),
            ("r_reactive_mask", res.default_reactive.view.clone()),
            ("r_transparency_and_composition_mask", res.default_reactive.view.clone()),
            ("r_reconstructed_previous_nearest_depth", res.reconstructed_previous_depth.view.clone()),
            ("rw_reconstructed_previous_nearest_depth", res.reconstructed_previous_depth.view.clone()),
            ("r_dilated_motion_vectors", res.dilated_motion_vectors[srv].view.clone()),
            ("rw_dilated_motion_vectors", res.dilated_motion_vectors[srv].view.clone()),
            ("r_previous_dilated_motion_vectors", res.dilated_motion_vectors[uav].view.clone()),
            ("r_dilatedDepth", res.dilated_depth.view.clone()),
            ("rw_dilatedDepth", res.dilated_depth.view.clone()),
            ("r_internal_upscaled_color", res.internal_upscaled[srv].view.clone()),
            ("rw_internal_upscaled_color", res.internal_upscaled[uav].view.clone()),
            ("r_rcas_input", res.internal_upscaled[uav].view.clone()),
            ("r_lock_status", res.lock_status[srv].view.clone()),
            ("rw_lock_status", res.lock_status[uav].view.clone()),
            ("r_lock_input_luma", res.lock_input_luma.view.clone()),
            ("rw_lock_input_luma", res.lock_input_luma.view.clone()),
            ("r_new_locks", res.new_locks.view.clone()),
            ("rw_new_locks", res.new_locks.view.clone()),
            ("r_prepared_input_color", res.prepared_input_color.view.clone()),
            ("rw_prepared_input_color", res.prepared_input_color.view.clone()),
            ("r_luma_history", res.luma_history[srv].view.clone()),
            ("rw_luma_history", res.luma_history[uav].view.clone()),
            ("r_imgMips", res.scene_luminance.view.clone()),
            ("rw_img_mip_shading_change", mip_view(&res.scene_luminance, params::SHADING_CHANGE_MIP_LEVEL as u32)),
            ("rw_img_mip_5", mip_view(&res.scene_luminance, 5)),
            ("rw_auto_exposure", res.auto_exposure.view.clone()),
            ("rw_spd_global_atomic", res.spd_atomic.view.clone()),
            ("r_dilated_reactive_masks", res.dilated_reactive_masks.view.clone()),
            ("rw_dilated_reactive_masks", res.dilated_reactive_masks.view.clone()),
            ("rw_upscaled_output", res.output.view.clone()),
            ("r_lanczos_lut", res.dummy_lut.view.clone()),
            ("r_upsample_maximum_bias_lut", res.dummy_lut.view.clone()),
        ]
        .into_iter()
        .collect();
        let src_groups = (render.0.div_ceil(8), render.1.div_ceil(8));
        let mut order: Vec<(&str, (u32, u32))> = vec![("compute_luminance_pyramid", (gx, gy)), ("reconstruct_previous_depth", src_groups), ("depth_clip", src_groups), ("lock", src_groups)];
        if sharpness.is_some() {
            order.extend([("accumulate_sharpen", (display.0.div_ceil(8), display.1.div_ceil(8))), ("rcas", (display.0.div_ceil(16), display.1.div_ceil(16)))]);
        } else {
            order.push(("accumulate", (display.0.div_ceil(8), display.1.div_ceil(8))));
        }
        {
            let mut cpass = enc.begin_compute_pass(&Default::default());
            let upto: usize = std::env::var("OMSI_FSR2_REFERENCE_UPTO").ok().and_then(|v| v.parse().ok()).unwrap_or(99);
            for (pass, (x, y)) in order.into_iter().filter(|_| only != "port").take(upto) {
                eprintln!("  reference pass {pass}");
                let (pipeline, layouts, bindings) = &reference.passes[pass];
                // (a pass that declares a resource as both an SRV and a UAV uses only the UAV:
                // the SRV gets a texel of its own, the two may not share a dispatch here)
                let both = |name: &str| name.strip_prefix("r_").is_some_and(|rest| bindings.iter().any(|o| o.name == format!("rw_{rest}") && views.get(o.name.as_str()).map(|v| v.texture()) == views.get(name).map(|v| v.texture())));
                let spare: HashMap<&str, wgpu::TextureView> = bindings.iter().filter(|b| both(&b.name)).map(|b| (b.name.as_str(), tex(&device, "ref spare", (1, 1), views[b.name.as_str()].texture().format(), 1).view)).collect();
                let groups: Vec<wgpu::BindGroup> = (0..2u32).map(|set| {
                    let entries: Vec<wgpu::BindGroupEntry> = bindings.iter().filter(|b| b.set == set).map(|b| wgpu::BindGroupEntry {
                        binding: b.binding,
                        resource: match (b.kind, b.name.as_str()) {
                            (SpvKind::Sampler, "s_PointClamp") => wgpu::BindingResource::Sampler(&reference.point),
                            (SpvKind::Sampler, _) => wgpu::BindingResource::Sampler(&reference.linear),
                            (SpvKind::Uniform, "cbFSR2") => reference.cb.as_entire_binding(),
                            (SpvKind::Uniform, "cbSPD") => reference.spd_cb.as_entire_binding(),
                            (SpvKind::Uniform, "cbRCAS") => reference.rcas_cb.as_entire_binding(),
                            (_, name) => wgpu::BindingResource::TextureView(spare.get(name).or_else(|| views.get(name)).unwrap_or_else(|| panic!("{pass}: {name}"))),
                        },
                    }).collect();
                    device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(pass), layout: &layouts[set as usize], entries: &entries })
                }).collect();
                cpass.set_pipeline(pipeline);
                cpass.set_bind_group(0, &groups[0], &[]);
                cpass.set_bind_group(1, &groups[1], &[]);
                cpass.dispatch_workgroups(x, y, 1);
            }
        }
        queue.submit([enc.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        eprintln!("frame {t}: reference done");
        history.resource_frame_index = (history.resource_frame_index + 1) % 16;

        let mine = port.res.as_ref().expect("the port's resources");
        if std::env::var("OMSI_FSR2_REFERENCE_DETAIL").is_ok() {
            // each intermediate: the reference's texture and channel count, the port's
            let pairs: [(&str, &wgpu::Texture, usize, &wgpu::Texture, usize); 10] = [
                ("prepared input colour", &res.prepared_input_color.texture, 4, &mine.prepared_input_color._texture, 4),
                ("dilated depth", &res.dilated_depth.texture, 1, &mine.dilated_depth._texture, 1),
                ("dilated motion vectors", &res.dilated_motion_vectors[srv].texture, 2, &mine.dilated_motion_vectors[srv]._texture, 4),
                ("lock input luma", &res.lock_input_luma.texture, 1, &mine.lock_input_luma._texture, 1),
                ("dilated reactive masks", &res.dilated_reactive_masks.texture, 2, &mine.dilated_reactive_masks._texture, 4),
                ("new locks", &res.new_locks.texture, 1, &mine.new_locks._texture, 1),
                ("lock status", &res.lock_status[uav].texture, 2, &mine.lock_status[uav]._texture, 4),
                ("luma history", &res.luma_history[uav].texture, 4, &mine.luma_history[uav]._texture, 4),
                ("auto exposure", &res.auto_exposure.texture, 2, &mine.output._texture, 0),
                ("history", &res.internal_upscaled[uav].texture, 4, &mine.internal_upscaled[uav]._texture, 4),
            ];
            eprintln!("    auto exposure: ref {:?}", read(&device, &queue, &res.auto_exposure.texture));
            eprintln!("    input colour at 2000: {:?}", &read(&device, &queue, &color.texture)[8000..8004]);
            for (name, a, ac, b, bc) in pairs {
                if bc == 0 {
                    continue;
                }
                let (va, vb) = (read(&device, &queue, a), read(&device, &queue, b));
                let n = ac.min(bc);
                let pick = |v: &[f32], c: usize| v.chunks_exact(c).flat_map(|px| px[..n].to_vec()).collect::<Vec<f32>>();
                let (max, mean) = compare(&pick(&va, ac), &pick(&vb, bc));
                let nz = |v: &[f32]| v.iter().filter(|x| **x != 0.0).count();
                eprintln!("    {name}: max {max:.5} mean {mean:.6}; at 2000: ref {:?} port {:?}; non-zero ref {} port {}", &va[2000 * ac..2000 * ac + ac], &vb[2000 * bc..2000 * bc + bc], nz(&va), nz(&vb));
            }
        }
        let (o_max, o_mean) = compare(&read(&device, &queue, &res.output.texture), &read(&device, &queue, &mine.output._texture));
        let (u_max, u_mean) = compare(&read(&device, &queue, &res.internal_upscaled[uav].texture), &read(&device, &queue, &mine.internal_upscaled[uav]._texture));
        eprintln!("frame {t:2}{}{}: output max {o_max:.5} mean {o_mean:.6}; history max {u_max:.5} mean {u_mean:.6}", if reset { " (reset)" } else { "" }, if sharpness.is_some() { " RCAS" } else { "" });
        worst.push((o_max, o_mean));
    }
    let _ = std::fs::remove_dir_all(&out);
    for (t, (max, mean)) in worst.iter().enumerate() {
        assert!(*mean < 2e-3 && *max < 0.05, "frame {t}: the output differs from the reference's (max {max}, mean {mean})");
    }
}
