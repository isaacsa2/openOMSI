//! A concrete BGFX implementation of the renderer-independent FrameCommands
//! contract. This is the bridge that lets the production scene eventually
//! submit draw commands without naming wgpu in its public API.
//!
//! Only opaque/alpha blend/additive materials and a single RGBA8 sampler are
//! supported here. This is NOT a replacement for the shipping renderer yet;
//! image readback, mip streaming, OMSI alpha tests, mirrors, screens, XR and
//! their lighting passes still require native implementations.
//!
//! IMPORTANT: bgfx is initialized by the native window owner on one graphics
//! API thread. Create/drop this object before bgfx_shutdown. Do not send it
//! across threads or invoke calls from a Rayon worker.

use std::{collections::HashMap, path::Path};

use crate::{
    frame_contract::{BlendMode, DepthMode, Draw, FrameCommands, MeshUpload, NativeRenderer, ResourceId, View},
    gpu_mesh::{GpuMesh, Program},
    o3d_bridge::{MaterialRun, PackedO3d, VERTEX_STRIDE},
    texture_bridge::{Sampler, Texture},
};

/// One renderer instance per initialized BGFX session. Resource IDs never
/// alias previously freed resources during this session.
pub struct BgfxNativeBackend {
    program: Program,
    sampler: Sampler,
    fallback: Texture,
    meshes: HashMap<ResourceId, GpuMesh>,
    next_id: u64,
    /// bgfx is a global C API. This marker prevents claiming Send/Sync until
    /// the render-thread lifetime and dispatch model are proven safe.
    _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl BgfxNativeBackend {
    pub fn new(vs: &Path, fs: &Path) -> Result<Self, String> {
        // SAFETY: caller has initialized bgfx and is on its designated
        // graphics thread. The owned GPU handles are dropped before shutdown.
        let program = unsafe { Program::load(vs, fs)? };
        let sampler = Sampler::new()?;
        let fallback = Texture::white()?;
        Ok(Self {
            program, sampler, fallback, meshes: HashMap::new(), next_id: 1,
            _not_send: std::marker::PhantomData,
        })
    }

    pub fn mesh_count(&self) -> usize { self.meshes.len() }

    pub fn free_mesh(&mut self, id: ResourceId) -> Result<(), String> {
        self.meshes.remove(&id)
            .map(drop)
            .ok_or_else(|| format!("unknown BGFX mesh resource id {}", id.0))
    }

    /// Upload by the native BGFX texture API, using the real openOMSI decoder
    /// at the caller. Material binding/texture stores are not part of this
    /// first FrameCommands trait revision.
    pub fn create_texture(&self, width: u32, height: u32, rgba: &[u8]) -> Result<Texture, String> {
        Texture::from_rgba(width, height, rgba)
    }
}

fn checked_mesh_buffers(desc: MeshUpload, vertices: &[u8], indices: &[u8]) -> Result<(), String> {
    desc.validate().map_err(str::to_owned)?;
    if usize::from(desc.vertex_stride) != VERTEX_STRIDE {
        return Err(format!("expected OMSI position/normal/UV vertex stride {VERTEX_STRIDE}"));
    }
    let vbytes = (desc.vertex_count as usize)
        .checked_mul(usize::from(desc.vertex_stride))
        .ok_or("vertex staging length overflow")?;
    let index_stride = if desc.index_32bit { 4 } else { 2 };
    let ibytes = (desc.index_count as usize)
        .checked_mul(index_stride)
        .ok_or("index staging length overflow")?;
    if vbytes != vertices.len() || ibytes != indices.len() {
        return Err("vertex/index buffer lengths do not match upload descriptor".into());
    }
    if vbytes.checked_add(ibytes).is_none_or(|n| n > 256 * 1024 * 1024) {
        return Err("single GPU mesh exceeds staging budget of 256MiB".into());
    }
    // Invalid pointers are fatal inside BGFX's C++ mesh pipeline. Reject
    // corrupt game/mod indices before uploading any GPU memory.
    for chunk in indices.chunks_exact(index_stride) {
        let idx = if desc.index_32bit {
            u32::from_le_bytes(chunk.try_into().expect("4-byte chunk"))
        } else {
            u16::from_le_bytes(chunk.try_into().expect("2-byte chunk")) as u32
        };
        if idx >= desc.vertex_count {
            return Err(format!("mesh index {idx} exceeds vertex count {}", desc.vertex_count));
        }
    }
    // No NaNs or infinity in any position, normal or UV.
    for entry in vertices.chunks_exact(4) {
        if !f32::from_le_bytes(entry.try_into().expect("4-byte f32")).is_finite() {
            return Err("mesh contains non-finite vertex value".into());
        }
    }
    Ok(())
}

pub fn native_view_id(view: View) -> Result<u16, String> {
    match view {
        View::Main => Ok(0),
        View::Mirror(i) if i < 32 => Ok(1 + u16::from(i)),
        View::Mirror(_) => Err("too many mirror views (max 32)".into()),
        View::Navigation => Ok(64),
        View::Ui => Ok(65),
        View::Photo => Ok(66),
        View::VrEye(0) => Ok(67),
        View::VrEye(1) => Ok(68),
        View::VrEye(_) => Err("only two headset eye views are mapped".into()),
    }
}

pub fn native_draw_state(draw: &Draw) -> Result<u64, String> {
    use bgfx_sys::*;
    let mut state = BGFX_STATE_DEFAULT;
    match draw.depth {
        DepthMode::TestWrite => {}
        DepthMode::TestOnly => state &= !BGFX_STATE_WRITE_Z,
        DepthMode::Disabled => {
            state &= !BGFX_STATE_WRITE_Z;
            state &= !(BGFX_STATE_DEPTH_TEST_MASK as u64);
        }
    }
    match draw.blend {
        BlendMode::Opaque => {}
        BlendMode::AlphaTest => return Err("OMSI alpha-test shader pipeline is not implemented in BGFX".into()),
        BlendMode::AlphaBlend => {
            // bgfx-sys 0.15.0 exports the packed blend-factor enums, not the
            // BGFX_STATE_BLEND_ALPHA C macro. BGFX_STATE_BLEND_FUNC packs
            // src in the low blend nibble, dst four bits above it.
            let src = BGFX_STATE_BLEND_SRC_ALPHA as u64;
            let dst = BGFX_STATE_BLEND_INV_SRC_ALPHA as u64;
            state = (state & !(BGFX_STATE_BLEND_MASK as u64)) | src | (dst << 4);
        }
        BlendMode::Additive => {
            let one = BGFX_STATE_BLEND_ONE as u64;
            state = (state & !(BGFX_STATE_BLEND_MASK as u64)) | one | (one << 4);
        }
    }
    Ok(state)
}

impl NativeRenderer for BgfxNativeBackend {
    type Error = String;

    fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 || width > 65535 || height > 65535 {
            return Err("invalid BGFX back buffer dimensions (1..65535)".into());
        }
        // SAFETY: bgfx is live; this must execute on the native graphics thread.
        unsafe { bgfx_sys::bgfx_reset(width, height, bgfx_sys::BGFX_RESET_VSYNC, bgfx_sys::BGFX_TEXTURE_FORMAT_COUNT); }
        Ok(())
    }

    fn upload_mesh(&mut self, desc: MeshUpload, vertices: &[u8], indices: &[u8]) -> Result<ResourceId, String> {
        checked_mesh_buffers(desc, vertices, indices)?;
        let id = ResourceId(self.next_id);
        let next = self.next_id.checked_add(1).ok_or("BGFX resource ID space exhausted")?;
        let input = PackedO3d {
            vertices: vertices.to_vec(),
            indices: indices.to_vec(),
            vertex_count: desc.vertex_count,
            index_count: desc.index_count,
            index32: desc.index_32bit,
            runs: vec![MaterialRun { material: 0, first_index: 0, index_count: desc.index_count }],
            model_transform: glam::Mat4::IDENTITY.to_cols_array(),
        };
        // SAFETY: input buffers have passed validation and bgfx copies them.
        let gpu_mesh = unsafe { GpuMesh::upload(&input)? };
        self.meshes.insert(id, gpu_mesh);
        self.next_id = next;
        Ok(id)
    }

    fn draw_frame(&mut self, frame: &FrameCommands) -> Result<(), String> {
        frame.validate().map_err(str::to_owned)?;
        let (width, height) = frame.viewport;
        if width > u16::MAX as u32 || height > u16::MAX as u32 {
            return Err("BGFX view dimensions exceed u16".into());
        }
        // Validate entire frame first. A bad material/mesh must not leave
        // the API halfway through a frame with dangling encoder state.
        for draw in frame.ordered_draws() {
            native_view_id(draw.view)?;
            native_draw_state(draw)?;
            if !self.meshes.contains_key(&draw.mesh) {
                return Err(format!("BGFX frame references unknown mesh {}", draw.mesh.0));
            }
        }
        let mut configured = [false; 256];
        unsafe {
            use bgfx_sys::*;
            bgfx_set_view_rect(0, 0, 0, width as u16, height as u16);
            bgfx_set_view_clear(0, (BGFX_CLEAR_COLOR | BGFX_CLEAR_DEPTH) as u16, 0x17212cff, 1.0, 0);
            bgfx_set_view_mode(0, BGFX_VIEW_MODE_SEQUENTIAL);
            bgfx_touch(0);
            configured[0] = true;
            for camera in &frame.cameras {
                let view = native_view_id(camera.view).expect("camera frame prevalidated");
                bgfx_set_view_rect(view, 0, 0, width as u16, height as u16);
                bgfx_set_view_mode(view, BGFX_VIEW_MODE_SEQUENTIAL);
                bgfx_set_view_transform(
                    view,
                    camera.world_to_camera.as_ptr() as *const std::ffi::c_void,
                    camera.camera_to_clip.as_ptr() as *const std::ffi::c_void,
                );
                configured[view as usize] = true;
            }
            for draw in frame.ordered_draws() {
                let view = native_view_id(draw.view).expect("frame prevalidated");
                let state = native_draw_state(draw).expect("frame prevalidated");
                if !configured[view as usize] {
                    bgfx_set_view_rect(view, 0, 0, width as u16, height as u16);
                    bgfx_set_view_mode(view, BGFX_VIEW_MODE_SEQUENTIAL);
                    configured[view as usize] = true;
                }
                let mesh = self.meshes.get(&draw.mesh).expect("frame prevalidated");
                bgfx_set_transform(draw.transform.as_ptr() as *const std::ffi::c_void, 1);
                bgfx_set_vertex_buffer(0, mesh.vertex_handle(), 0, u32::MAX);
                bgfx_set_index_buffer(mesh.index_handle(), 0, mesh.indices);
                bgfx_set_state(if mesh.one_sided { state } else { state & !BGFX_STATE_CULL_MASK }, 0);
                bgfx_set_texture(0, self.sampler.handle(), self.fallback.handle(), u32::MAX);
                bgfx_submit(view, self.program.handle(), 0, BGFX_DISCARD_ALL as u8);
            }
            bgfx_frame(false);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> [f32; 16] { glam::Mat4::IDENTITY.to_cols_array() }

    #[test]
    fn rejects_corrupted_gpu_mesh_data_before_native_upload() {
        let d = MeshUpload { vertex_stride: 32, vertex_count: 3, index_count: 3, index_32bit: false };
        let v = vec![0u8; 3 * 32];
        assert!(checked_mesh_buffers(d, &v, &[0, 0, 1, 0, 2, 0]).is_ok());
        assert!(checked_mesh_buffers(d, &v, &[0, 0, 3, 0, 2, 0]).is_err());
        assert!(checked_mesh_buffers(d, &v, &[0, 0, 1, 0]).is_err());
        let mut bad = v.clone();
        bad[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(checked_mesh_buffers(d, &bad, &[0, 0, 1, 0, 2, 0]).is_err());
    }

    #[test]
    fn alpha_test_explicitly_unsupported_until_shader_port_is_correct() {
        let d = Draw {
            mesh: ResourceId(1), material: ResourceId(1), view: View::Main,
            blend: BlendMode::AlphaTest, depth: DepthMode::TestWrite,
            transform: identity(), authored_order: 0,
        };
        assert!(native_draw_state(&d).is_err());
        assert_eq!(native_view_id(View::VrEye(1)), Ok(68));
        assert!(native_view_id(View::Mirror(32)).is_err());
    }
}
