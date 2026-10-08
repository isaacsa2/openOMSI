//! Finds what in the scene shader makes the D3D compiler (fxc) overflow its stack under
//! ANGLE/D3D11: the scene module's GLSL ES is linked through ANGLE on WARP once whole, then
//! with each function's body emptied in turn, every variant in a process of its own (an
//! overflow ends the process). Run by release.yml only after the startup regression failed.

#[cfg(windows)]
use std::process::Command;
use wgpu::naga;

#[cfg(windows)]
const CASE: &str = "ANGLE_BISECT_VARIANT";

/// Rewrites of the scene WGSL tried as variants of their own ("patch:<name>"): each a
/// hypothesis about which construct the compiler cannot take.
const PATCHES: &[(&str, &str, &str)] = &[
    ("rt_all_taps", "if (!(d < tol * 0.3)) {", "if (true) {"),
    ("rt_centre_only", "if (!(d < tol * 0.3)) {", "if (false) {"),
    (
        "rt_no_fetch",
        "let s = textureLoad(t_ao, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0);",
        "let s = vec4<f32>(vec2<f32>(p), 0.5, 1.0);",
    ),
    (
        "finite_by_compare",
        "return all(e != vec3<u32>(0x7f800000u));",
        "return all(abs(v) <= vec3<f32>(3.4e38)) && all(e == e);",
    ),
];

/// `vs_main` and `fs_main` as the OpenGL backend gets them on ANGLE (no storage buffers),
/// with the scene WGSL rewritten by `patch` (see `PATCHES`).
fn stages_patched(patch: Option<&str>) -> [String; 2] {
    let mut wgsl = crate::scene_shader_text(true);
    if let Some(name) = patch {
        let (_, from, to) = PATCHES.iter().find(|p| p.0 == name).expect("patch");
        assert!(wgsl.contains(from), "patch {name}: text not found");
        wgsl = wgsl.replace(from, to);
    }
    let src = crate::arrays_as_textures(&wgsl, crate::ArrayPath::NoStorage);
    let module = naga::front::wgsl::parse_str(&src).expect("scene WGSL");
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .expect("validate");
    let constants = [("ALPHA_TEST".into(), 0.0), ("ALPHA_TO_COVERAGE".into(), 0.0), ("TERRAIN_PAINT".into(), 0.0)]
        .into_iter()
        .collect();
    let (module, info) = naga::back::pipeline_constants::process_overrides(&module, &info, None, &constants).expect("overrides");
    let options = naga::back::glsl::Options {
        version: naga::back::glsl::Version::Embedded { version: 300, is_webgl: false },
        ..Default::default()
    };
    ["vs_main", "fs_main"].map(|name| {
        let entry = module.entry_points.iter().find(|e| e.name == name).expect("entry point");
        let pipeline = naga::back::glsl::PipelineOptions { shader_stage: entry.stage, entry_point: name.into(), multiview: None };
        let mut out = String::new();
        naga::back::glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, Default::default())
            .and_then(|mut w| w.write())
            .expect("GLSL ES");
        out
    })
}

fn stages() -> [String; 2] {
    stages_patched(None)
}

/// The functions defined in `glsl`: (name, return type, line of the header).
fn functions(glsl: &str) -> Vec<(String, String, usize)> {
    glsl.lines()
        .enumerate()
        .filter(|(_, l)| !l.starts_with(' ') && !l.starts_with("struct") && l.ends_with(") {"))
        .filter_map(|(i, l)| {
            let head = &l[..l.find('(')?];
            let (ret, name) = head.rsplit_once(' ')?;
            Some((name.to_string(), ret.trim().to_string(), i))
        })
        .collect()
}

/// `glsl` with the body of `name` emptied (a value of its type left undefined).
fn stubbed(glsl: &str, name: &str) -> String {
    let lines: Vec<&str> = glsl.lines().collect();
    let Some((_, ret, start)) = functions(glsl).into_iter().find(|(n, _, _)| n == name) else {
        return glsl.to_string();
    };
    let end = start + lines[start..].iter().position(|l| *l == "}").expect("end of function");
    let body = match (name, ret.as_str()) {
        ("main", _) if glsl.contains("gl_Position") => "    gl_Position = vec4(0.0);".to_string(),
        (_, "void") => String::new(),
        (_, ret) => format!("    {ret} stub_value;\n    return stub_value;"),
    };
    let mut out: Vec<String> = lines[..=start].iter().map(|l| l.to_string()).collect();
    if !body.is_empty() {
        out.push(body);
    }
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    out.join("\n") + "\n"
}

#[cfg(windows)]
/// Compiles and links one variant ("full", "vs:<fn>" or "fs:<fn>") through ANGLE on WARP.
fn link_variant(variant: &str) {
    use glow::HasContext;
    let patch = variant.strip_prefix("patch:");
    let [mut vs, mut fs] = stages_patched(patch);
    if let Some((stage, name)) = variant.split_once(':').filter(|_| patch.is_none()) {
        let target = if stage == "vs" { &mut vs } else { &mut fs };
        *target = stubbed(target, name);
    }
    let directory = std::env::var("ANGLE_TEST_LIBRARY_DIRECTORY").expect("ANGLE DLL directory");
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backend_options.gl.angle.device_type = wgpu::AngleDeviceType::Warp;
    descriptor.backend_options.gl.angle.library_directory = Some(directory);
    let instance = crate::angle::instance(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).expect("ANGLE adapter");
    let limits = adapter.limits();
    let (device, _queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: limits, ..Default::default() }))
        .expect("ANGLE device");
    // SAFETY: the context is locked (made current) for these GL calls only
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Gles>() }.expect("GL device");
    let gl = hal.context().lock();
    unsafe {
        if gl.supported_extensions().contains("GL_KHR_parallel_shader_compile") {
            gl.max_shader_compiler_threads(0);
        }
        let program = gl.create_program().expect("program");
        for (kind, src) in [(glow::VERTEX_SHADER, &vs), (glow::FRAGMENT_SHADER, &fs)] {
            let shader = gl.create_shader(kind).expect("shader");
            gl.shader_source(shader, src);
            gl.compile_shader(shader);
            assert!(gl.get_shader_compile_status(shader), "{variant}: {}", gl.get_shader_info_log(shader));
            gl.attach_shader(program, shader);
        }
        gl.link_program(program);
        assert!(gl.get_program_link_status(program), "{variant}: {}", gl.get_program_info_log(program));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "diagnostic: requires Windows and the packaged ANGLE DLLs"]
fn angle_scene_shader_bisect() {
    if let Ok(variant) = std::env::var(CASE) {
        std::thread::Builder::new()
            .stack_size(32 << 20)
            .spawn(move || link_variant(&variant))
            .expect("compiler thread")
            .join()
            .expect("variant failed without crashing");
        return;
    }
    let [vs, fs] = stages();
    let mut variants = vec!["full".to_string()];
    variants.extend(PATCHES.iter().map(|p| format!("patch:{}", p.0)));
    for (stage, glsl) in [("vs", &vs), ("fs", &fs)] {
        variants.extend(functions(glsl).into_iter().map(|(name, _, _)| format!("{stage}:{name}")));
    }
    eprintln!("BISECT vs_main {} lines, fs_main {} lines, {} variants", vs.lines().count(), fs.lines().count(), variants.len());
    for variant in variants {
        let status = Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", "angle_bisect::angle_scene_shader_bisect", "--ignored", "--nocapture"])
            .env(CASE, &variant)
            .output()
            .expect("variant process");
        let outcome = match status.status.code() {
            Some(0) => "ok".to_string(),
            Some(c) if c as u32 == 0xc00000fd => "STACK OVERFLOW".to_string(),
            other => format!("failed ({other:?}): {}", String::from_utf8_lossy(&status.stderr).lines().rev().find(|l| l.contains(':')).unwrap_or("")),
        };
        eprintln!("BISECT {variant} => {outcome}");
    }
}

#[test]
fn stubbing_keeps_the_rest_of_the_shader() {
    let glsl = "struct S {\n    float a;\n};\nfloat f(float x) {\n    return x * 2.0;\n}\nvoid main() {\n    float y = f(1.0);\n}\n";
    let out = stubbed(glsl, "f");
    assert!(out.contains("float f(float x) {\n    float stub_value;\n    return stub_value;\n}"));
    assert!(out.contains("void main() {\n    float y = f(1.0);\n}"));
    assert_eq!(functions(glsl).into_iter().map(|f| f.0).collect::<Vec<_>>(), ["f", "main"]);
}

/// Every function of the real scene stages can be emptied without losing another.
#[test]
fn every_scene_function_can_be_stubbed() {
    for glsl in stages() {
        let names: Vec<_> = functions(&glsl).into_iter().map(|f| f.0).collect();
        assert!(names.len() > 20 && names.contains(&"main".to_string()));
        for name in &names {
            let after: Vec<_> = functions(&stubbed(&glsl, name)).into_iter().map(|f| f.0).collect();
            assert_eq!(after, names, "stubbing {name}");
        }
    }
}

/// Every patch still applies to the scene shader and leaves it valid.
#[test]
fn every_patch_applies() {
    for (name, _, _) in PATCHES {
        let [vs, fs] = stages_patched(Some(name));
        assert!(vs.contains("void main") && fs.contains("void main"), "{name}");
    }
}
