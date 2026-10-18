//! Windows BGFX native-window milestone for the openOMSI port.
//!
//! A real window can display a .o3d mesh with shaderc-produced BGFX shaders.
//! This is a mesh VIEWER, not yet the full openOMSI driving application.
//! No OMSI models or other original assets are bundled with this repository.

#[cfg(windows)]
mod win {
    use std::{collections::HashSet, ffi::c_void, path::PathBuf, ptr, time::Instant};
    use glam::{Mat4, Vec3};
    use openomsi_bgfx_port::{Backend, gpu_mesh::{GpuMesh, Program}, o3d_bridge::{load_file_with_textures, PackedO3d, VERTEX_STRIDE}, geometry_bridge::pack_generated, map_bridge::{load_terrain_preview, MapPreview, Selection}, texture_bridge::{Texture, Sampler}, drive_bridge::{DrivePreview, DriverKeys}, native_backend::BgfxNativeBackend, frame_contract::{NativeRenderer, FrameCommands, ViewCamera, View, Draw, MeshUpload, ResourceId, BlendMode, DepthMode}};
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
        map_path: Option<PathBuf>,
        bus_definition: Option<PathBuf>,
        map_center: Option<(i32, i32)>,
        map_radius: u8,
        exit_after_frames: Option<u32>,
        vertex_shader: Option<PathBuf>,
        fragment_shader: Option<PathBuf>,
        texture_path: Option<PathBuf>,
        contract_demo: bool,
    }

    fn parse() -> Result<Settings, String> {
        let mut backend = Backend::Auto;
        let mut path = None;
        let mut terrain_path = None;
        let mut map_path = None;
        let mut bus_definition = None;
        let mut tile_x = None;
        let mut tile_y = None;
        let mut map_radius = 1;
        let mut exit_after_frames = None;
        let mut vertex_shader = None;
        let mut fragment_shader = None;
        let mut texture_path = None;
        let mut contract_demo = false;
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            if flag == "--contract-demo" {
                contract_demo = true;
                continue;
            }
            let val = match flag.as_str() {
                "-h" | "--help" => return Err("Usage: bgfx-window [--backend dx11|dx12|vulkan|gl|auto] [--mesh PATH.o3d | --terrain PATH.map.terrain | --map PATH/global.cfg] [--bus-definition PATH.bus] [--vs PATH.bin --fs PATH.bin] [--frames N]\nKeys: left/right rotate, up/down zoom, Esc exit. Shader binaries must match selected backend.".into()),
                "--backend" | "--mesh" | "--terrain" | "--map" | "--tile-x" | "--tile-y" | "--radius" | "--vs" | "--fs" | "--frames" | "--texture" | "--bus-definition" => args.next().ok_or_else(|| format!("{flag} requires a value"))?,
                _ => return Err(format!("unrecognized flag: {flag}")),
            };
            match flag.as_str() {
                "--backend" => backend = Backend::parse(&val).ok_or_else(|| format!("invalid graphics backend: {val}"))?,
                "--mesh" => path = Some(PathBuf::from(val)),
                "--terrain" => terrain_path = Some(PathBuf::from(val)),
                "--texture" => texture_path = Some(PathBuf::from(val)),
                "--map" => map_path = Some(PathBuf::from(val)),
                "--bus-definition" => bus_definition = Some(PathBuf::from(val)),
                "--tile-x" => tile_x = Some(val.parse::<i32>().map_err(|_| "invalid tile X coordinate")?),
                "--tile-y" => tile_y = Some(val.parse::<i32>().map_err(|_| "invalid tile Y coordinate")?),
                "--radius" => map_radius = val.parse::<u8>().map_err(|_| "invalid tile radius")?,
                "--frames" => {
                    let n = val.parse::<u32>().map_err(|_| "--frames must be a positive integer")?;
                    if !(1..=10_000).contains(&n) { return Err("--frames must be in 1..=10000".into()); }
                    exit_after_frames = Some(n);
                },
                "--vs" => vertex_shader = Some(PathBuf::from(val)),
                "--fs" => fragment_shader = Some(PathBuf::from(val)),
                _ => unreachable!(),
            }
        }
        let selected = usize::from(path.is_some()) + usize::from(terrain_path.is_some()) + usize::from(map_path.is_some());
        if bus_definition.is_some() {
            if map_path.is_none() || path.is_none() || terrain_path.is_some() {
                return Err("--bus-definition requires both --map and --mesh, without --terrain".into());
            }
        } else if selected > 1 {
            return Err("choose one of --mesh, --terrain or --map unless using --bus-definition".into());
        }
        if tile_x.is_some() != tile_y.is_some() {
            return Err("--tile-x and --tile-y must be supplied together".into());
        }
        if (tile_x.is_some() || tile_y.is_some()) && map_path.is_none() {
            return Err("--tile-x/--tile-y are valid only with --map".into());
        }
        if map_radius > 2 {
            return Err("--radius must be between 0 and 2 tiles".into());
        }
        if contract_demo && selected != 0 {
            return Err("--contract-demo cannot be mixed with map/mesh preview".into());
        }
        if (selected > 0 || contract_demo) && (vertex_shader.is_none() || fragment_shader.is_none()) {
            return Err("--mesh/--terrain/--map need --vs and --fs matching the backend".into());
        }
        let map_center = tile_x.zip(tile_y);
        Ok(Settings { backend, path, terrain_path, map_path, bus_definition, map_center, map_radius,
            exit_after_frames, vertex_shader, fragment_shader, texture_path, contract_demo })
    }

    struct Viewer {
        config: Settings,
        window: Option<Window>,
        initialized: bool,
        mesh: Option<GpuMesh>,
        map_meshes: Vec<GpuMesh>,
        map_preview: Option<MapPreview>,
        driving: Option<DrivePreview>,
        program: Option<Program>,
        texture: Option<Texture>,
        material_textures: Vec<Option<Texture>>,
        sampler: Option<Sampler>,
        contract: Option<BgfxNativeBackend>,
        contract_mesh: Option<ResourceId>,
        contract_failed: bool,
        center: Vec3,
        distance: f32,
        yaw: f32,
        drawn_frames: u32,
        map_camera: Vec3,
        map_mode: bool,
        held_keys: HashSet<KeyCode>,
        last_frame: Instant,
    }

    impl Viewer {
        fn new(config: Settings) -> Self {
            Self {
                config, window: None, initialized: false,
                mesh: None, map_meshes: Vec::new(), map_preview: None, driving: None, program: None, texture: None, material_textures: Vec::new(), sampler: None, contract: None, contract_mesh: None, contract_failed: false, center: Vec3::ZERO,
                distance: 24., yaw: 0., drawn_frames: 0,
                map_camera: Vec3::new(150., 150., 120.), map_mode: false,
                held_keys: HashSet::new(), last_frame: Instant::now(),
            }
        }

        fn open_contract_demo(&mut self) -> Result<(), String> {
            let mut backend = BgfxNativeBackend::new(
                self.config.vertex_shader.as_ref().ok_or("--vs needed")?,
                self.config.fragment_shader.as_ref().ok_or("--fs needed")?,
            )?;
            // Synthetic data, in openOMSI's world coordinate system.
            // The demo validates the cross-backend frame contract without
            // requiring any original bus/map content or touching the game.
            let vertices: [[f32; 8]; 3] = [
                [-2., 0., 0., 0., -1., 0., 0., 0.],
                [ 2., 0., 0., 0., -1., 0., 1., 0.],
                [ 0., 0., 4., 0., -1., 0., 0.5, 1.],
            ];
            let mut packed = Vec::with_capacity(3 * 32);
            for vertex in vertices {
                for value in vertex {
                    packed.extend_from_slice(&value.to_le_bytes());
                }
            }
            let mesh_id = backend.upload_mesh(
                MeshUpload {
                    vertex_stride: 32, vertex_count: 3, index_count: 3,
                    index_32bit: false, one_sided: false,
                },
                &packed, &[0u8, 0, 1, 0, 2, 0],
            )?;
            println!("BGFX_CONTRACT_UPLOAD_OK mesh_id={} vertices=3 indices=3", mesh_id.0);
            self.contract = Some(backend);
            self.contract_mesh = Some(mesh_id);
            Ok(())
        }

        fn create_material_preview(&mut self) -> Result<(), String> {
            if self.texture.is_none() {
                let tex = if let Some(ref path) = self.config.texture_path {
                    Texture::from_file(path)?
                } else {
                    Texture::white()?
                };
                self.texture = Some(tex);
            }
            if self.sampler.is_none() {
                self.sampler = Some(Sampler::new()?);
            }
            Ok(())
        }

        fn set_mesh(&mut self, model: PackedO3d, one_sided: bool, texture_paths: &[Option<PathBuf>]) -> Result<(), String> {
            self.create_material_preview()?;
            self.material_textures.clear();
            if self.config.texture_path.is_none() {
                let mut bytes = 0usize;
                // This preview limits texture residency to avoid exhausting
                // smaller cards: normal openOMSI uses a streaming budget.
                for path in texture_paths.iter().take(256) {
                    let picture = path.as_ref().and_then(|p| Texture::from_file(p).ok());
                    let picture = match picture {
                        Some(t) if bytes.saturating_add(usize::from(t.width) * usize::from(t.height) * 4)
                            <= 96 * 1024 * 1024 => {
                                bytes += usize::from(t.width) * usize::from(t.height) * 4;
                                Some(t)
                            },
                        _ => None,
                    };
                    self.material_textures.push(picture);
                }
                println!("BGFX material preview: {}/{} textures loaded, decoded RGBA budget {:.1}MiB",
                    self.material_textures.iter().filter(|p| p.is_some()).count(),
                    texture_paths.len(), bytes as f64 / 1048576.0);
            }
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

        fn load_preview_map(&mut self, path: &std::path::Path) -> Result<(), String> {
            self.create_material_preview()?;
            let preview = load_terrain_preview(path, Selection {
                center: self.config.map_center,
                radius: self.config.map_radius,
            })?;
            let mut native_meshes = Vec::with_capacity(preview.tiles.len());
            for terrain in &preview.tiles {
                let mut mesh = unsafe { GpuMesh::upload(&terrain.geometry)? };
                mesh.one_sided = false;
                native_meshes.push(mesh);
            }
            let program = unsafe { Program::load(
                self.config.vertex_shader.as_ref().ok_or("missing --vs")?,
                self.config.fragment_shader.as_ref().ok_or("missing --fs")?,
            )? };
            println!("BGFX map preview: {} center=({},{}) radius={} tiles={}",
                preview.map_name, preview.center.0, preview.center.1,
                self.config.map_radius, native_meshes.len());
            println!("Only terrain meshes are rendered; this is NOT a drivable OMSI session.");
            self.map_camera = Vec3::new(preview.tile_size * 0.5, preview.tile_size * 0.5, 120.);
            self.map_meshes = native_meshes;
            self.map_preview = Some(preview);
            self.program = Some(program);
            self.map_mode = true;
            self.distance = 40.;
            Ok(())
        }

        fn tick_camera(&mut self) {
            let now = Instant::now();
            let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
            self.last_frame = now;
            if let (Some(bus), Some(map)) = (self.driving.as_mut(), self.map_preview.as_ref()) {
                let keys = DriverKeys {
                    accelerate: self.held_keys.contains(&KeyCode::KeyW),
                    brake: self.held_keys.contains(&KeyCode::KeyS),
                    left: self.held_keys.contains(&KeyCode::KeyA),
                    right: self.held_keys.contains(&KeyCode::KeyD),
                };
                let height = map.ground_height(bus.pos);
                bus.advance(dt, keys, height);
                self.yaw = bus.yaw_deg.to_radians();
                let back = Vec3::new(-self.yaw.sin() * 12.0, -self.yaw.cos() * 12.0, 5.0);
                self.map_camera = bus.pos + back;
                return;
            }
            if !self.map_mode { return; }
            let forward = Vec3::new(self.yaw.sin(), self.yaw.cos(), 0.);
            let right = Vec3::new(self.yaw.cos(), -self.yaw.sin(), 0.);
            let mut step = Vec3::ZERO;
            if self.held_keys.contains(&KeyCode::KeyW) { step += forward; }
            if self.held_keys.contains(&KeyCode::KeyS) { step -= forward; }
            if self.held_keys.contains(&KeyCode::KeyD) { step += right; }
            if self.held_keys.contains(&KeyCode::KeyA) { step -= right; }
            if self.held_keys.contains(&KeyCode::KeyE) { step += Vec3::Z; }
            if self.held_keys.contains(&KeyCode::KeyQ) { step -= Vec3::Z; }
            if step.length_squared() > 0. {
                let fast = self.held_keys.contains(&KeyCode::ShiftLeft)
                    || self.held_keys.contains(&KeyCode::ShiftRight);
                self.map_camera += step.normalize() * dt * if fast { 75. } else { 15. };
            }
        }

        fn draw(&mut self) {
            self.tick_camera();
            let Some(window) = self.window.as_ref() else { return };
            let size = window.inner_size();
            if size.width == 0 || size.height == 0 { return; }
            let w = size.width.min(u16::MAX as u32) as u16;
            let h = size.height.min(u16::MAX as u32) as u16;
            if let (Some(renderer), Some(mesh)) = (self.contract.as_mut(), self.contract_mesh) {
                let eye = Vec3::new(0., -12., 4.);
                let view = Mat4::look_at_rh(eye, Vec3::new(0., 0., 1.6), Vec3::Z);
                let projection = Mat4::perspective_rh(
                    60_f32.to_radians(), w as f32 / h as f32, 0.1, 1000.,
                );
                let frame = FrameCommands {
                    viewport: (u32::from(w), u32::from(h)),
                    cameras: vec![ViewCamera {
                        view: View::Main,
                        world_to_camera: view.to_cols_array(),
                        camera_to_clip: projection.to_cols_array(),
                        reversed_z: false,
                    }],
                    draws: vec![Draw {
                        mesh,
                        material: ResourceId(0),
                        view: View::Main,
                        blend: BlendMode::Opaque,
                        depth: DepthMode::TestWrite,
                        range: None,
                        transform: Mat4::IDENTITY.to_cols_array(),
                        authored_order: 0,
                    }],
                };
                if let Err(e) = renderer.draw_frame(&frame) {
                    eprintln!("BGFX_CONTRACT_FRAME_FAILED: {e}");
                    self.contract_failed = true;
                    return;
                }
                self.drawn_frames += 1;
                if self.drawn_frames == 1 {
                    println!("BGFX_CONTRACT_FRAME_OK draws={} viewport={}x{}", frame.draws.len(), w, h);
                }
                return;
            }
            // For OMSI's coordinate convention Z is up and Y goes forwards.
            let (eye, target) = if self.map_mode {
                let target = if let Some(bus) = self.driving.as_ref() {
                    bus.pos + Vec3::new(0.0, 0.0, 1.8)
                } else {
                    self.map_camera + Vec3::new(self.yaw.sin(), self.yaw.cos(), -0.35)
                };
                (self.map_camera, target)
            } else {
                let eye = self.center + Vec3::new(self.yaw.sin() * self.distance,
                    -self.yaw.cos() * self.distance, self.distance * 0.35);
                (eye, self.center)
            };
            let view = Mat4::look_at_rh(eye, target, Vec3::Z).to_cols_array();
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
                if let (Some(program), Some(tex), Some(sampler)) =
                    (self.program.as_ref(), self.texture.as_ref(), self.sampler.as_ref()) {
                    for mesh in &self.map_meshes {
                        mesh.draw(program.handle(), tex, sampler, &self.material_textures);
                    }
                    if let Some(mesh) = self.mesh.as_ref() {
                        if let Some(bus) = self.driving.as_ref() {
                            let model = bus.model_transform();
                            mesh.draw_with_transform(program.handle(), tex, sampler, &self.material_textures, &model);
                        } else {
                            mesh.draw(program.handle(), tex, sampler, &self.material_textures);
                        }
                    }
                }
                bgfx_sys::bgfx_frame(false);
            }
            self.drawn_frames += 1;
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
            if self.config.contract_demo {
                if let Err(e) = self.open_contract_demo() {
                    eprintln!("BGFX_CONTRACT_INIT_FAILED: {e}");
                    event_loop.exit();
                    return;
                }
            }
            if let Some(path) = self.config.path.clone() {
                match load_file_with_textures(&path)
                    .and_then(|(mesh, textures)| self.set_mesh(mesh, true, &textures)) {
                    Ok(()) => println!("Loaded O3D mesh from {}", path.display()),
                    Err(e) => { eprintln!("O3D mesh viewer initialization: {e}"); event_loop.exit(); return; },
                }
            }
            if let Some(path) = self.config.terrain_path.clone() {
                let result = omsi_map::Terrain::load(&path).map_err(|e| e.to_string())
                    .and_then(|terrain| pack_generated(&omsi_geometry::build_terrain_mesh(&terrain)))
                    .and_then(|data| self.set_mesh(data.mesh, data.one_sided, &[]));
                match result {
                    Ok(()) => println!("Loaded OMSI terrain from {}", path.display()),
                    Err(e) => { eprintln!("Terrain viewer initialization: {e}"); event_loop.exit(); return; }
                }
            }
            if let Some(path) = self.config.map_path.clone() {
                if let Err(e) = self.load_preview_map(&path) {
                    eprintln!("Map preview initialization: {e}");
                    event_loop.exit();
                    return;
                }
            }
            if let Some(path) = self.config.bus_definition.clone() {
                let Some(map) = self.map_preview.as_ref() else {
                    eprintln!("Cannot start driving preview without map heightfields");
                    event_loop.exit();
                    return;
                };
                let mut start = Vec3::new(map.tile_size * 0.5, map.tile_size * 0.5, 0.0);
                let Some(ground) = map.ground_height(start) else {
                    eprintln!("No terrain height available at initial bus location");
                    event_loop.exit();
                    return;
                };
                start.z = ground;
                match DrivePreview::from_bus_file(&path, start) {
                    Ok(driving) => {
                        println!("BGFX_DRIVE_PREVIEW_START bus={} (kinematic physics test; no OMSI scripts, doors, collisions or complete game)", path.display());
                        self.driving = Some(driving);
                        self.map_camera = start + Vec3::new(0.0, -12.0, 5.0);
                    },
                    Err(e) => {
                        eprintln!("Bus preview initialization: {e}");
                        event_loop.exit();
                        return;
                    }
                }
            }
            self.window = Some(window);
            self.window.as_ref().unwrap().request_redraw();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            if self.window.as_ref().is_none_or(|w| w.id() != id) { return; }
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Focused(false) => self.held_keys.clear(),
                WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => unsafe {
                    bgfx_sys::bgfx_reset(size.width, size.height, bgfx_sys::BGFX_RESET_VSYNC,
                        bgfx_sys::BGFX_TEXTURE_FORMAT_COUNT);
                    self.window.as_ref().unwrap().request_redraw();
                },
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(key) = event.physical_key {
                        if event.state == ElementState::Pressed {
                            self.held_keys.insert(key);
                            if !event.repeat {
                                match key {
                                    KeyCode::Escape => event_loop.exit(),
                                    KeyCode::ArrowLeft => self.yaw -= 0.12,
                                    KeyCode::ArrowRight => self.yaw += 0.12,
                                    KeyCode::ArrowUp if !self.map_mode => self.distance = (self.distance * 0.9).max(0.1),
                                    KeyCode::ArrowDown if !self.map_mode => self.distance *= 1.1,
                                    _ => {}
                                }
                            }
                        } else {
                            self.held_keys.remove(&key);
                        }
                        if let Some(w) = self.window.as_ref() { w.request_redraw(); }
                    }
                }
                WindowEvent::RedrawRequested => {
                    self.draw();
                    if self.contract_failed {
                        event_loop.exit();
                    } else if self.config.exit_after_frames.is_some_and(|n| self.drawn_frames >= n) {
                        println!("BGFX_NATIVE_FRAME_SMOKE_OK rendered_frames={}", self.drawn_frames);
                        event_loop.exit();
                    } else if let Some(w) = self.window.as_ref() { w.request_redraw(); }
                }
                _ => {}
            }
        }
    }

    impl Drop for Viewer {
        fn drop(&mut self) {
            // The mesh/program must die *before* bgfx, not afterwards.
            self.contract_mesh.take();
            self.contract.take();
            self.mesh.take();
            self.map_meshes.clear();
            self.map_preview.take();
            self.driving.take();
            self.program.take();
            self.texture.take();
            self.material_textures.clear();
            self.sampler.take();
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
