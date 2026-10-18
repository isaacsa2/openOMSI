//! First real bgfx GPU upload/draw path for an OMSI .o3d mesh.
//!
//! This uses a simple NORMAL-as-colour shader. No textures, lighting parity,
//! animation, world streaming, bus physics or UI: this is a GPU geometry milestone.
//! Access only from the bgfx API thread. The owner must be destroyed before
//! calling bgfx_shutdown. All bgfx C calls are unsafe global-state operations.

use crate::o3d_bridge::{PackedO3d, VERTEX_STRIDE};
use std::{ffi::c_void, fs, path::Path, mem::MaybeUninit};

pub struct GpuMesh {
    vertex: bgfx_sys::bgfx_vertex_buffer_handle_t,
    index: bgfx_sys::bgfx_index_buffer_handle_t,
    pub indices: u32,
    pub index32: bool,
    /// False for generated terrain and spline geometry that can be seen from both sides.
    pub one_sided: bool,
    pub ranges: Vec<crate::o3d_bridge::MaterialRun>,
    pub model_transform: [f32; 16],
}

impl GpuMesh {
    /// Upload source file data to the initialized bgfx backend.
    /// All CPU buffers are copied by bgfx_copy before this call returns.
    pub unsafe fn upload(mesh: &PackedO3d) -> Result<Self, String> {
        use bgfx_sys::*;
        if mesh.vertices.len() != mesh.vertex_count as usize * VERTEX_STRIDE
            || mesh.indices.len() != mesh.index_count as usize * if mesh.index32 { 4 } else { 2 }
        {
            return Err("packed mesh buffer length mismatch".into());
        }
        let mut layout = MaybeUninit::<bgfx_vertex_layout_t>::uninit();
        bgfx_vertex_layout_begin(layout.as_mut_ptr(), bgfx_get_renderer_type());
        let mut layout = layout.assume_init();
        bgfx_vertex_layout_add(&mut layout, BGFX_ATTRIB_POSITION, 3, BGFX_ATTRIB_TYPE_FLOAT, false, false);
        bgfx_vertex_layout_add(&mut layout, BGFX_ATTRIB_NORMAL, 3, BGFX_ATTRIB_TYPE_FLOAT, false, false);
        bgfx_vertex_layout_add(&mut layout, BGFX_ATTRIB_TEXCOORD0, 2, BGFX_ATTRIB_TYPE_FLOAT, false, false);
        bgfx_vertex_layout_end(&mut layout);
        if usize::from(layout.stride) != VERTEX_STRIDE {
            return Err(format!("bgfx vertex layout stride: expected {VERTEX_STRIDE}, got {}", layout.stride));
        }
        let vmemory = bgfx_copy(mesh.vertices.as_ptr() as *const c_void, mesh.vertices.len() as u32);
        if vmemory.is_null() {
            return Err("bgfx_copy failed for vertex data".into());
        }
        let vertex = bgfx_create_vertex_buffer(vmemory, &layout, BGFX_BUFFER_NONE as u16);
        if vertex.idx == u16::MAX {
            return Err("bgfx_create_vertex_buffer failed".into());
        }
        let imemory = bgfx_copy(mesh.indices.as_ptr() as *const c_void, mesh.indices.len() as u32);
        if imemory.is_null() {
            bgfx_destroy_vertex_buffer(vertex);
            return Err("bgfx_copy failed for index data".into());
        }
        let index = bgfx_create_index_buffer(
            imemory,
            if mesh.index32 { BGFX_BUFFER_INDEX32 as u16 } else { BGFX_BUFFER_NONE as u16 },
        );
        if index.idx == u16::MAX {
            bgfx_destroy_vertex_buffer(vertex);
            return Err("bgfx_create_index_buffer failed".into());
        }
        Ok(Self {
            vertex, index, indices: mesh.index_count, index32: mesh.index32,
            one_sided: true,
            ranges: mesh.runs.clone(), model_transform: mesh.model_transform,
        })
    }

    /// Submit each material run *in original authored order*. Sorting by
    /// material breaks some glass/transmap content. The proof-of-concept
    /// shader does not reproduce material appearance yet.
    pub unsafe fn draw(&self, program: bgfx_sys::bgfx_program_handle_t) {
        use bgfx_sys::*;
        for range in &self.ranges {
            bgfx_set_transform(self.model_transform.as_ptr() as *const c_void, 1);
            bgfx_set_vertex_buffer(0, self.vertex, 0, u32::MAX);
            bgfx_set_index_buffer(self.index, range.first_index, range.index_count);
            let state = if self.one_sided {
                BGFX_STATE_DEFAULT
            } else {
                BGFX_STATE_DEFAULT & !(BGFX_STATE_CULL_MASK as u64)
            };
            bgfx_set_state(state, 0);
            bgfx_submit(0, program, 0, BGFX_DISCARD_ALL as u8);
        }
    }
}

impl Drop for GpuMesh {
    fn drop(&mut self) {
        unsafe {
            bgfx_sys::bgfx_destroy_index_buffer(self.index);
            bgfx_sys::bgfx_destroy_vertex_buffer(self.vertex);
        }
    }
}

pub struct Program(bgfx_sys::bgfx_program_handle_t);

impl Program {
    pub unsafe fn load(vs: &Path, fs_path: &Path) -> Result<Self, String> {
        use bgfx_sys::*;
        fn bytes(path: &Path) -> Result<Vec<u8>, String> {
            let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            if data.is_empty() || data.len() > 32 * 1024 * 1024 {
                return Err(format!("{}: shader must be 1..33554432 bytes", path.display()));
            }
            Ok(data)
        }
        unsafe fn make_shader(bin: &[u8]) -> Result<bgfx_shader_handle_t, String> {
            let memory = bgfx_copy(bin.as_ptr() as *const c_void, bin.len() as u32);
            if memory.is_null() {
                return Err("could not allocate bgfx shader memory".into());
            }
            let shader = bgfx_create_shader(memory);
            if shader.idx == u16::MAX {
                return Err("bgfx rejected compiled shader".into());
            }
            Ok(shader)
        }
        let vertex = make_shader(&bytes(vs)?)?;
        let fragment = match make_shader(&bytes(fs_path)?) {
            Ok(shader) => shader,
            Err(e) => {
                bgfx_destroy_shader(vertex);
                return Err(e);
            }
        };
        let program = bgfx_create_program(vertex, fragment, true);
        if program.idx == u16::MAX {
            // destroyShaders=true: the program owns both shaders on success.
            // On failure the bgfx API may already release them; do not double free.
            return Err("vertex/fragment shaders could not be linked".into());
        }
        Ok(Self(program))
    }

    pub fn handle(&self) -> bgfx_sys::bgfx_program_handle_t { self.0 }
}

impl Drop for Program {
    fn drop(&mut self) {
        unsafe { bgfx_sys::bgfx_destroy_program(self.0) }
    }
}
