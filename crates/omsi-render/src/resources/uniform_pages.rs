//! The uniform values of the materials, in pages shared by many of them.
//!
//! Every material that looked unlike the ones before it got a GPU buffer of its own, a couple
//! of hundred bytes, and the scene made some thousands of them: each one an object of the
//! device (and of its memory allocator) to make, to keep and to give back. Now a material is
//! given a slice of a page - one buffer of some kilobytes holding the values of many - and its
//! bind group binds that slice (a buffer, an offset, a size). The values of a material never
//! change once it is made, and a slice is never given out twice: a page stays as long as one
//! bind group refers to it, and frees itself with the last.
//!
//! The pages are small on purpose: a long-lived material keeps its whole page, so a big one
//! would hold on to memory that the materials of a tile that was unloaded no longer need.
//! `OMSI_NO_UNIFORM_PAGES` makes every page one slice (a buffer per material, as before).

use super::materials::MaterialUniform;
use crate::{Renderer, Scene};
use std::num::NonZeroU64;

/// The bytes of a material's values.
pub(crate) const UNIFORM_BYTES: u64 = std::mem::size_of::<MaterialUniform>() as u64;

/// How big a page is.
const PAGE_BYTES: u64 = 16 * 1024;

/// Where a material's values are: a page and the offset in it.
#[derive(Clone)]
pub(crate) struct UniformSlot {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) offset: u64,
}

impl UniformSlot {
    /// What the material's bind group binds.
    pub(crate) fn binding(&self) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.buffer, offset: self.offset, size: NonZeroU64::new(UNIFORM_BYTES) })
    }
}

/// The page being filled.
#[derive(Default)]
pub(crate) struct UniformPages {
    page: Option<wgpu::Buffer>,
    /// Where the next slice goes in it.
    next: u64,
    /// Pages made and slices given out, in all.
    pub(crate) made: (u32, u32),
}

/// The distance between two slices: the values' size up to what the device asks of the offset
/// of a uniform binding.
pub(crate) fn stride(offset_alignment: u32) -> u64 {
    UNIFORM_BYTES.next_multiple_of(offset_alignment.max(1) as u64)
}

impl Renderer {
    /// A slice of a page holding `uniform`, for a material's bind group.
    pub(crate) fn uniform_slot(&self, scene: &mut Scene, uniform: &MaterialUniform) -> UniformSlot {
        let stride = stride(self.device.limits().min_uniform_buffer_offset_alignment);
        let pages = &mut scene.uniform_pages;
        let page_bytes = if omsi_cfg::flags::OMSI_NO_UNIFORM_PAGES.is_set() { stride } else { PAGE_BYTES.max(stride) };
        if pages.page.is_none() || pages.next + stride > page_bytes {
            pages.page = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("material uniforms"),
                size: page_bytes / stride * stride,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            pages.next = 0;
            pages.made.0 += 1;
        }
        let buffer = pages.page.clone().expect("a page was just made");
        let offset = pages.next;
        pages.next += stride;
        pages.made.1 += 1;
        self.queue.write_buffer(&buffer, offset, bytemuck::bytes_of(uniform));
        UniformSlot { buffer, offset }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_are_spaced_by_what_the_device_asks() {
        assert_eq!(UNIFORM_BYTES % 16, 0, "a vec4 multiple, as the shader's struct");
        assert_eq!(stride(1), UNIFORM_BYTES);
        assert_eq!(stride(16), UNIFORM_BYTES);
        assert_eq!(stride(64), UNIFORM_BYTES.next_multiple_of(64));
        assert_eq!(stride(256), 256);
        assert_eq!(stride(0), UNIFORM_BYTES, "a limit of 0 is not a divide by zero");
        for a in [4u32, 16, 32, 64, 128, 256] {
            assert_eq!(stride(a) % a as u64, 0);
            assert!(stride(a) >= UNIFORM_BYTES);
        }
        // a page holds a good few of them at the largest alignment
        assert!(PAGE_BYTES / stride(256) >= 32);
    }
}
