//! The scene shader's read-only arrays (model matrices, instance parameters, draw list,
//! point lights and their grid): a storage buffer, or a texture where the device cannot read
//! one (see `ArrayPath`), with the layout entry that binds it.

use super::device_caps::{array_path, ArrayPath};

/// Texels per row of an array kept as a texture (within every device's 2048).
pub(super) const ARRAY_TEX_WIDTH: u32 = 2048;

/// A read-only array of the scene shader: a storage buffer, or where the device cannot
/// read one (see `ArrayPath`) a texture of one value per texel - four floats
/// (`Rgba32Float`, 16 bytes) or one `u32` (`R32Uint`) - in rows of `ARRAY_TEX_WIDTH`.
pub(super) enum GpuArray {
    Buffer(wgpu::Buffer),
    Texture { texture: wgpu::Texture, view: wgpu::TextureView, texel: u32 },
    /// Not bound on this device (the lights under `ArrayPath::NoStorage`): takes any write.
    Unused,
}

impl GpuArray {
    /// An array of `size` bytes of `texel`-byte values (16 or 4), a texture where the
    /// vertex shader reads it on a device without vertex storage (`vertex`), or without any.
    pub(super) fn new(device: &wgpu::Device, label: &str, size: u64, texel: u32, vertex: bool) -> GpuArray {
        let path = array_path();
        if path == ArrayPath::Storage || (!vertex && path == ArrayPath::VertexTextures) {
            return GpuArray::Buffer(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if !vertex {
            return GpuArray::Unused;
        }
        let texels = size.div_ceil(texel as u64).max(1);
        let max_rows = device.limits().max_texture_dimension_2d;
        let rows = (texels.div_ceil(ARRAY_TEX_WIDTH as u64) as u32).clamp(1, max_rows);
        if rows == max_rows {
            log::warn!("{label}: {texels} values do not fit a texture of {ARRAY_TEX_WIDTH} x {max_rows}; the rest are not drawn");
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: ARRAY_TEX_WIDTH, height: rows, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if texel == 16 { wgpu::TextureFormat::Rgba32Float } else { wgpu::TextureFormat::R32Uint },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        GpuArray::Texture { texture, view, texel }
    }

    /// Its room in bytes.
    pub(super) fn size(&self) -> u64 {
        match self {
            GpuArray::Buffer(b) => b.size(),
            GpuArray::Texture { texture, texel, .. } => texture.width() as u64 * texture.height() as u64 * *texel as u64,
            GpuArray::Unused => u64::MAX,
        }
    }

    /// Bytes it takes on the GPU (for the memory summary).
    pub(super) fn gpu_bytes(&self) -> u64 {
        match self {
            GpuArray::Unused => 0,
            a => a.size(),
        }
    }

    /// Write `data` at byte `offset` (both whole values); what runs past its end is dropped.
    pub(super) fn write(&self, queue: &wgpu::Queue, offset: u64, data: &[u8]) {
        match self {
            GpuArray::Buffer(b) => queue.write_buffer(b, offset, data),
            GpuArray::Texture { texture, texel, .. } => {
                let t = *texel as usize;
                let total = texture.width() as u64 * texture.height() as u64;
                let mut data = data;
                for (x, y, width, rows) in array_tex_spans(offset / t as u64, (data.len() / t) as u64, total) {
                    let bytes = (width * rows) as usize * t;
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
                        &data[..bytes],
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * t as u32), rows_per_image: None },
                        wgpu::Extent3d { width, height: rows, depth_or_array_layers: 1 },
                    );
                    data = &data[bytes..];
                }
            }
            GpuArray::Unused => {}
        }
    }

    pub(super) fn binding(&self) -> wgpu::BindingResource<'_> {
        match self {
            GpuArray::Buffer(b) => b.as_entire_binding(),
            GpuArray::Texture { view, .. } => wgpu::BindingResource::TextureView(view),
            GpuArray::Unused => unreachable!("an unused array is not bound"),
        }
    }
}

/// The rectangles (x, y, width, rows) of an array texture `ARRAY_TEX_WIDTH` wide and
/// `total` texels big that `n` values from value `at` on fill, in order: the rest of a row,
/// whole rows, the start of the last row. What runs past the end is left out.
pub(super) fn array_tex_spans(mut at: u64, n: u64, total: u64) -> Vec<(u32, u32, u32, u32)> {
    let w = ARRAY_TEX_WIDTH as u64;
    let mut left = n.min(total.saturating_sub(at));
    let mut out = Vec::new();
    while left > 0 {
        let (x, y) = (at % w, at / w);
        let (width, rows) = if x == 0 && left >= w { (w, left / w) } else { ((w - x).min(left), 1) };
        out.push((x as u32, y as u32, width as u32, rows as u32));
        at += width * rows;
        left -= width * rows;
    }
    out
}

/// The bind group layout entry of a scene array read in `stage` (see `GpuArray`):
/// `float4` for vec4 values, else u32.
pub(super) fn array_layout_entry(binding: u32, stage: wgpu::ShaderStages, float4: bool) -> wgpu::BindGroupLayoutEntry {
    array_layout_entry_on(array_path(), binding, stage, float4)
}

/// The same on a device whose arrays take `path`.
pub(super) fn array_layout_entry_on(path: ArrayPath, binding: u32, stage: wgpu::ShaderStages, float4: bool) -> wgpu::BindGroupLayoutEntry {
    let texture = match path {
        ArrayPath::Storage => false,
        ArrayPath::VertexTextures => stage.contains(wgpu::ShaderStages::VERTEX),
        ArrayPath::NoStorage => true,
    };
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty: if texture {
            wgpu::BindingType::Texture {
                sample_type: if float4 { wgpu::TextureSampleType::Float { filterable: false } } else { wgpu::TextureSampleType::Uint },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            }
        } else {
            wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }
        },
        count: None,
    }
}
