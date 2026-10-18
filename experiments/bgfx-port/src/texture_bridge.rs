//! First native BGFX texture path, reusing openOMSI's original image decoders.
use std::{ffi::c_void, path::Path};

pub struct Texture {
    handle: bgfx_sys::bgfx_texture_handle_t,
    pub width: u16,
    pub height: u16,
}

impl Texture {
    /// Reuse openOMSI's format sniffing and BMP/DDS/TGA decoding, never a second
    /// incompatible image loader. Data is copied into BGFX-owned memory.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let image = omsi_texture::decode_file(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_rgba(image.width, image.height, &image.rgba)
    }

    pub fn white() -> Result<Self, String> {
        Self::from_rgba(1, 1, &[255, 255, 255, 255])
    }

    pub fn from_rgba(width: u32, height: u32, pixels: &[u8]) -> Result<Self, String> {
        let w = u16::try_from(width).map_err(|_| "texture width exceeds native 16-bit BGFX limit")?;
        let h = u16::try_from(height).map_err(|_| "texture height exceeds native 16-bit BGFX limit")?;
        if w == 0 || h == 0 {
            return Err("texture dimensions must be nonzero".into());
        }
        let bytes = (width as usize).checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("texture dimensions overflow")?;
        if pixels.len() != bytes || bytes > 256 * 1024 * 1024 {
            return Err("RGBA pixels must match width * height * 4 and stay under 256MiB".into());
        }
        // SAFETY: bgfx is initialized on the graphics thread by the caller;
        // bgfx_copy duplicates the pixel array before it goes out of scope.
        let handle = unsafe {
            let mem = bgfx_sys::bgfx_copy(pixels.as_ptr() as *const c_void, bytes as u32);
            if mem.is_null() {
                return Err("bgfx_copy failed for texture pixels".into());
            }
            bgfx_sys::bgfx_create_texture_2d(
                w, h, false, 1, bgfx_sys::BGFX_TEXTURE_FORMAT_RGBA8,
                u64::from(bgfx_sys::BGFX_TEXTURE_NONE), mem,
            )
        };
        if handle.idx == u16::MAX {
            return Err("bgfx_create_texture_2d rejected the image".into());
        }
        Ok(Self { handle, width: w, height: h })
    }

    pub fn handle(&self) -> bgfx_sys::bgfx_texture_handle_t {
        self.handle
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        // SAFETY: GPU objects are destroyed by the owner before bgfx_shutdown.
        unsafe { bgfx_sys::bgfx_destroy_texture(self.handle); }
    }
}

pub struct Sampler(bgfx_sys::bgfx_uniform_handle_t);

impl Sampler {
    pub fn new() -> Result<Self, String> {
        // Name must equal SAMPLER2D(s_texColor, 0) in fs_mesh.sc.
        let h = unsafe {
            bgfx_sys::bgfx_create_uniform(
                c"s_texColor".as_ptr(), bgfx_sys::BGFX_UNIFORM_TYPE_SAMPLER, 1,
            )
        };
        if h.idx == u16::MAX {
            return Err("bgfx_create_uniform failed for s_texColor".into());
        }
        Ok(Self(h))
    }
    pub fn handle(&self) -> bgfx_sys::bgfx_uniform_handle_t { self.0 }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe { bgfx_sys::bgfx_destroy_uniform(self.0); }
    }
}

#[cfg(test)]
mod tests {
    // GPU calls require a live native renderer. Keep invalid-input unit tests
    // in a separate validation layer; never initialize BGFX globally here.
    use super::*;

    #[test]
    fn texture_validation_rejects_bad_pixels_without_gpu_initialization() {
        assert!(Texture::from_rgba(0, 1, &[]).is_err());
        assert!(Texture::from_rgba(1, 1, &[0, 0, 0]).is_err());
        assert!(Texture::from_rgba(65536, 1, &[]).is_err());
    }
}
