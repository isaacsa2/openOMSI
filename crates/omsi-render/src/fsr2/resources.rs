//! FSR 2's internal resources (the reference's internalSurfaceDesc, ffx_fsr2.cpp), made for one
//! maximum render size and display size. Formats the reference uses that WebGPU cannot write
//! from a shader are held in the next wider one (see the passes' notes): R16F and RG16F in
//! R32F and RGBA16F, RG8 in RGBA8, R8 in R32F; the reconstructed depth (R32_UINT with image
//! atomics) and the exposure (RG32F) in storage buffers.

use super::params;

/// One texture and its view.
pub(crate) struct Image {
    pub(crate) _texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
}

fn image(device: &wgpu::Device, label: &str, size: (u32, u32), format: wgpu::TextureFormat, extra: wgpu::TextureUsages) -> Image {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        // (copied from in the tests' comparison with the reference)
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | extra,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Image { _texture: texture, view }
}

/// The resources of one size; the pairs are a frame's input and output, swapped every frame.
pub(crate) struct Resources {
    pub(crate) max_render: (u32, u32),
    pub(crate) display: (u32, u32),
    pub(crate) prepared_input_color: Image,
    /// FSR2_ReconstructedPrevNearestDepth: one u32 per pixel of the maximum render size.
    pub(crate) reconstructed_previous_depth: wgpu::Buffer,
    pub(crate) dilated_motion_vectors: [Image; 2],
    pub(crate) dilated_depth: Image,
    pub(crate) lock_status: [Image; 2],
    pub(crate) lock_input_luma: Image,
    pub(crate) new_locks: Image,
    pub(crate) internal_upscaled: [Image; 2],
    /// The shading change luma: mip 4 of the reference's FSR2_ExposureMips, as its own texture.
    pub(crate) luma_shading_change: Image,
    /// Mip 5 of FSR2_ExposureMips, read by the second luminance dispatch.
    pub(crate) luma_mip5: Image,
    pub(crate) luma_history: [Image; 2],
    pub(crate) dilated_reactive_masks: Image,
    /// FSR2_AutoExposure: (exposure, average log luma).
    pub(crate) auto_exposure: wgpu::Buffer,
    /// The upscaled output the window's present pass copies.
    pub(crate) output: Image,
}

/// The texture formats the passes write (all of WebGPU's core storage formats).
pub(crate) const STORAGE_FORMATS: [wgpu::TextureFormat; 3] = [wgpu::TextureFormat::Rgba16Float, wgpu::TextureFormat::R32Float, wgpu::TextureFormat::Rgba8Unorm];

impl Resources {
    pub(crate) fn new(device: &wgpu::Device, max_render: (u32, u32), display: (u32, u32)) -> Resources {
        use wgpu::TextureFormat as F;
        let none = wgpu::TextureUsages::empty();
        // (cleared on a reset or each frame, by a render pass's clear: `clear_texture` is an
        // optional feature)
        let cleared = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let pair = |label: &str, size, format| [image(device, label, size, format, cleared), image(device, label, size, format, cleared)];
        Resources {
            max_render,
            display,
            prepared_input_color: image(device, "FSR2_PreparedInputColor", max_render, F::Rgba16Float, cleared),
            reconstructed_previous_depth: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("FSR2_ReconstructedPrevNearestDepth"),
                size: u64::from(max_render.0.max(1)) * u64::from(max_render.1.max(1)) * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            dilated_motion_vectors: pair("FSR2_InternalDilatedVelocity", max_render, F::Rgba16Float),
            dilated_depth: image(device, "FSR2_DilatedDepth", max_render, F::R32Float, none),
            lock_status: pair("FSR2_LockStatus", display, F::Rgba16Float),
            lock_input_luma: image(device, "FSR2_LockInputLuma", max_render, F::R32Float, none),
            new_locks: image(device, "FSR2_NewLocks", display, F::R32Float, cleared),
            internal_upscaled: pair("FSR2_InternalUpscaled", display, F::Rgba16Float),
            luma_shading_change: image(device, "FSR2_ExposureMips (shading change)", params::luma_mip_dimensions(max_render), F::Rgba16Float, cleared),
            luma_mip5: image(device, "FSR2_ExposureMips (mip 5)", params::luma_mip5_dimensions(max_render), F::R32Float, cleared),
            luma_history: pair("FSR2_LumaHistory", display, F::Rgba8Unorm),
            dilated_reactive_masks: image(device, "FSR2_DilatedReactiveMasks", max_render, F::Rgba8Unorm, none),
            auto_exposure: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("FSR2_AutoExposure"),
                size: 8,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            output: image(device, "FSR2 upscaled output", display, F::Rgba16Float, none),
        }
    }

    /// Video memory the resources take, roughly (for the logs).
    pub(crate) fn bytes(&self) -> u64 {
        let px = |s: (u32, u32)| u64::from(s.0) * u64::from(s.1);
        let (r, d) = (px(self.max_render), px(self.display));
        r * (8 + 4 + 2 * 8 + 4 + 4 + 4) + d * (2 * 8 + 4 + 2 * 8 + 2 * 4 + 8)
    }
}
