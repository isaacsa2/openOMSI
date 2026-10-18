//! Windows BGFX native-window milestone for the openOMSI port.
//!
//! A real window can display a .o3d mesh with shaderc-produced BGFX shaders.
//! This is a mesh VIEWER, not yet the full openOMSI driving application.
//! No OMSI models or other original assets are bundled with this repository.

#[cfg(windows)]
mod win {
    use std::{ffi::c_void, path::PathBuf, ptr};
    use glam::{Mat4, Vec3};
    use openomsi_bgfx_port::{Backend, gpu_mesh::{GpuMesh, Program}, o3d_bridge::{load_file, PackedO3d, VERTEX_STRIDE}, geometry_bridge::pack_generated};
    use winit::{
        application::ApplicationHandler,
        dpi::PhysicalSize,
        event::{WindowEvent, ElementState},
        event_loop::{ActiveEventLoop, EventLoop},
        keyboard::{KeyCode, PhysicalKey},
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::{Window, WindowId},
    };

    struct Settings {
        backend: Backend,
        path: Option<PathBuf>,
        terrain_path: Option<PathBuf>,
        vertex_shader: Option<PathBuf>,
        fragment_shader: Option<PathBuf>,
    }

    fn parse() -> Result<Settings, String> {
        let mut backend = Backend::Auto;
        let mut path = None;
        let mut terrain_path = None;
        let mut vertex_shader = None;
        let mut fragment_shader = None;
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let val = match flag.as_str() {
                "-h" | "--help" => return Err("Usage: bgfx-window [--backend dx11|dx12|vulkan|gl|auto] [--mesh PATH.o3d | --terrain PATH.map.terrain] [--vs PATH.bin --fs PATH.bin]\nKeys: left/right rotate, up/down zoom, Esc exit. Shader binaries must match selected backend.".into()),
                "--backend" | "--mesh" | "--terrain" | "--vs" | "--fs" => args.next().ok_or_else(|| format!("{flag} requires a value"))?,
                _ => return Err(format!("unrecognized flag: {flag}")),
            };
            match flag.as_str() {
                "--backend" => backend = Backend::parse(&val).ok_or_else(|| format!("invalid graphics backend: {val}"))?,
                "--mesh" => path = Some(PathBuf::from(val)),
                "--terrain" => terrain_path = Some(PathBuf::from(val)),
                "--vs" => vertex_shader = Some(PathBuf::from(val)),
                "--fs" => fragment_shader = Some(PathBuf::from(val)),
                _ => unreachable!(),
            }
        }
        if path.is_some() && terrain_path.is_some() {
            return Err("--mesh and --terrain cannot be combined in this single-mesh viewer".into());
        }
        if (path.is_some() || terrain_path.is_some()) && (vertex_shader.is_none() || fragment_shader.is_none()) {
            return Err("--mesh/--terrain need both --vs and --fs compiled for the selected BGFX backend".into());
        }
        Ok(Settings { backend, path, terrain_path, vertex_shader, fragment_shader })
    }

    struct Viewer {
        config: Settings,
        window: Option<Window>,
        initialized: bool,
        mesh: Option<GpuMesh>,
        program: Option<Program>,
        center: Vec3,
        distance: f32,
        yaw: f32,
    }

    impl Viewer {
        fn new(config: Settings) -> Self {
            Self {
                config, window: None, initialized: false,
                mesh: None, program: None, center: Vec3::ZERO,
                distance: 24., yaw: 0.,
            }
        }

        fn set_mesh(&mut self, model: PackedO3d, one_sided: bool) -> Result<(), String> {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            let transform = Mat4::from_cols_array(&model.model_transform);
            for vertex in model.vertices.chunks_exact(VERTEX_STRIDE) {
                let float = |at: usize| f32::from_le_bytes(vertex[at..at+4].try_into().unwrap());
                let p = transform.transform_point3(Vec3::new(float(0), float(4), float(8)));
                if p.is_finite() {
                    min = min.min(p);
                    max = max.max(p);
                }
            }
            if min.is_finite() && max.is_finite() {
                self.center = (min + max) * 0.5;
                self.distance = ((max - min).length() * 1.8).clamp(4., 10000.);
            }
            let mut gpu = unsafe { GpuMesh::upload(&model)? };
            gpu.one_sided = one_sided;
            self.mesh = Some(gpu);
            self.program = Some(unsafe { Program::load(
                self.config.vertex_shader.as_ref().expect("checked by parse"),
                self.config.fragment_shader.as_ref().expect("checked by parse"),
            )? });
            Ok(())
        }

        fn draw(&mut self) {
            let Some(window) = self.window.as_ref() else { return };
            let size = window.inner_size();
            if size.width == 0 || size.height == 0 { return; }
            let w = size.width.min(u16::MAX as u32) as u16;
            let h = size.height.min(u16::MAX as u32) as u16;
            // For OMSI's coordinate convention Z is up and Y goes forwards.
            let eye = self.center + Vec3::new(self.yaw.sin() * self.distance,
                -self.yaw.cos() * self.distance, self.distance * 0.35);
            let view = Mat4::look_at_rh(eye, self.center, Vec3::Z).to_cols_array();
            let projection = Mat4::perspective_rh(
                60_f32.to_radians(), w as f32 / h as f32, 0.05,
                (self.distance * 100.).max(1000.),
            ).to_cols_array();
            unsafe {
                bgfx_sys::bgfx_set_view_rect(0, 0, 0, w, h);
                bgfx_sys::bgfx_set_view_clear(0,
                    (bgfx_sys::BGFX_CLEAR_COLOR | bgfx_sys::BGFX_CLEAR_DEPTH) as u16,
                    0x17212cff, 1., 0);
                bgfx_sys::bgfx_set_view_transform(
                    0, view.as_ptr() as *const c_void, projection.as_ptr() as *const c_void);
                // Author-ordered glass/terrain submissions may not be automatically
                // sorted as opaque draw calls. Preserve order in this experiment.
                bgfx_sys::bgfx_set_view_mode(0, bgfx_sys::BGFX_VIEW_MODE_SEQUENTIAL);
                bgfx_sys::bgfx_touch(0);
                if let (Some(mesh), Some(program)) = (self.mesh.as_ref(), self.program.as_ref()) {
                    mesh.draw(program.handle());
                }
                bgfx_sys::bgfx_frame(false);
            }
        }
    }

    impl ApplicationHandler for Viewer {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() { return; }
            let attrs = Window::default_attributes()
                .with_title("openOMSI experimental BGFX O3D viewer (not playable)")
                .with_inner_size(PhysicalSize::new(1280, 720));
            let window = match event_loop.create_window(attrs) {
                Ok(w) => w,
                Err(e) => { eprintln!("window creation: {e}"); event_loop.exit(); return; },
            };
            let raw = match window.window_handle() {
                Ok(handle) => handle.as_raw(),
                Err(e) => { eprintln!("window handle: {e}"); event_loop.exit(); return; },
            };
            let hwnd = match raw {
                RawWindowHandle::Win32(h) => h.hwnd.get() as *mut c_void,
                _ => {
                    eprintln!("Windows Win32 HWND required");
                    event_loop.exit();
                    return;
                },
            };
            unsafe {
                let mut init = std::mem::MaybeUninit::<bgfx_sys::bgfx_init_t>::uninit();
                bgfx_sys::bgfx_init_ctor(init.as_mut_ptr());
                let mut init = init.assume_init();
                init.type_ = match self.config.backend {
                    Backend::Auto => bgfx_sys::BGFX_RENDERER_TYPE_COUNT,
                    Backend::Direct3D11 => bgfx_sys::BGFX_RENDERER_TYPE_DIRECT3D11,
                    Backend::Direct3D12 => bgfx_sys::BGFX_RENDERER_TYPE_DIRECT3D12,
                    Backend::Vulkan => bgfx_sys::BGFX_RENDERER_TYPE_VULKAN,
                    Backend::OpenGl => bgfx_sys::BGFX_RENDERER_TYPE_OPENGL,
                    Backend::Metal => bgfx_sys::BGFX_RENDERER_TYPE_METAL,
                };
                init.platformData.nwh = hwnd;
                init.platformData.ndt = ptr::null_mut();
                init.resolution.width = window.inner_size().width.max(1);
                init.resolution.height = window.inner_size().height.max(1);
                init.resolution.reset = bgfx_sys::BGFX_RESET_VSYNC;
                if !bgfx_sys::bgfx_init(&init) {
                    eprintln!("BGFX native window/device initialization failed");
                    event_loop.exit();
                    return;
                }
                self.initialized = true;
                println!("BGFX initialized: {:?}", bgfx_sys::bgfx_get_renderer_type());
            }
            if let Some(path) = self.config.path.clone() {
                match load_file(&path).and_then(|data| self.set_mesh(data, true)) {
                    Ok(()) => println!("Loaded O3D mesh from {}", path.display()),
                    Err(e) => { eprintln!("O3D mesh viewer initialization: {e}"); event_loop.exit(); return; },
                }
            }
            if let Some(path) = self.config.terrain_path.clone() {
                let result = omsi_map::Terrain::load(&path).map_err(|e| e.to_string())
                    .and_then(|terrain| pack_generated(&omsi_geometry::build_terrain_mesh(&terrain)))
                    .and_then(|data| self.set_mesh(data.mesh, data.one_sided));
                match result {
                    Ok(()) => println!("Loaded OMSI terrain from {}", path.display()),
                    Err(e) => { eprintln!("Terrain viewer initialization: {e}"); event_loop.exit(); return; }
                }
            }
            self.window = Some(window);
            self.window.as_ref().unwrap().request_redraw();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            if self.window.as_ref().is_none_or(|w| w.id() != id) { return; }
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => unsafe {
                    bgfx_sys::bgfx_reset(size.width, size.height, bgfx_sys::BGFX_RESET_VSYNC,
                        bgfx_sys::BGFX_TEXTURE_FORMAT_COUNT);
                    self.window.as_ref().unwrap().request_redraw();
                },
                WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                        PhysicalKey::Code(KeyCode::ArrowLeft) => self.yaw -= 0.12,
                        PhysicalKey::Code(KeyCode::ArrowRight) => self.yaw += 0.12,
                        PhysicalKey::Code(KeyCode::ArrowUp) => self.distance = (self.distance * 0.9).max(0.1),
                        PhysicalKey::Code(KeyCode::ArrowDown) => self.distance *= 1.1,
                        _ => {}
                    }
                    if let Some(w) = self.window.as_ref() { w.request_redraw(); }
                }
                WindowEvent::RedrawRequested => {
                    self.draw();
                    if let Some(w) = self.window.as_ref() { w.request_redraw(); }
                }
                _ => {}
            }
        }
    }

    impl Drop for Viewer {
        fn drop(&mut self) {
            // The mesh/program must die *before* bgfx, not afterwards.
            self.mesh.take();
            self.program.take();
            if self.initialized { unsafe { bgfx_sys::bgfx_shutdown() }; }
        }
    }

    pub fn main() -> Result<(), String> {
        let config = parse()?;
        let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
        let mut viewer = Viewer::new(config);
        event_loop.run_app(&mut viewer).map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
fn main() {
    if let Err(e) = win::main() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This first native-window bgfx viewer supports Windows only; cross-platform window handles are not implemented yet.");
    std::process::exit(2);
}
