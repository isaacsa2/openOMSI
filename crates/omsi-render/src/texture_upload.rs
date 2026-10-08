//! Textures put on the GPU: with their levels from the content (block-compressed, or RGBA
//! that wants no GPU-made chain) on a worker thread ([`prepare_texture`]), or a picture of
//! the game's own with or without a mip chain ([`upload_texture`]).

use super::texture_fit::fit_texture;
#[cfg(doc)]
use super::Renderer;
use super::{gl_worker_turn, next_gen, texture_bytes, GpuTexture};

/// A texture on the GPU, made on a worker thread; [`Renderer::add_prepared_texture`] puts it
/// into a scene.
pub struct PreparedTexture(pub(super) GpuTexture);

impl PreparedTexture {
    pub fn bytes(&self) -> u64 {
        self.0.bytes
    }
}

/// Make a texture that carries its levels (blocks, or RGBA that wants no GPU-made chain) on
/// any thread; None for anything else (an RGBA picture whose chain the GPU makes, a block
/// format the device does not take).
pub fn prepare_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &omsi_texture::TextureData,
) -> Option<PreparedTexture> {
    if let Some(small) = fit_texture(data, device.limits().max_texture_dimension_2d) {
        return prepare_texture(device, queue, &small);
    }
    let _turn = gl_worker_turn();
    use omsi_texture::PixelFormat;
    let format = match data.format {
        PixelFormat::Rgba8 => wgpu::TextureFormat::Rgba8UnormSrgb,
        PixelFormat::Bc1 => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
        PixelFormat::Bc2 => wgpu::TextureFormat::Bc2RgbaUnormSrgb,
        PixelFormat::Bc3 => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
    };
    let (w, h) = (data.width.max(1), data.height.max(1));
    if data.levels.is_empty()
        || (data.format == PixelFormat::Rgba8
            && data.levels.len() == 1
            && data.gpu_mips
            && w > 1
            && h > 1)
    {
        return None;
    }
    if data.format.is_compressed()
        && (!device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
            || w % 4 != 0
            || h % 4 != 0)
    {
        return None;
    }
    let levels = data.levels.len() as u32;
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let (bw, bh) = format.block_dimensions();
    let block = format.block_copy_size(None).unwrap_or(4);
    for (l, bytes) in data.levels.iter().enumerate() {
        let (lw, lh) = ((w >> l).max(1), (h >> l).max(1));
        // compressed levels are written whole blocks at a time, also below 4x4
        let phys = wgpu::Extent3d {
            width: lw.div_ceil(bw) * bw,
            height: lh.div_ceil(bh) * bh,
            depth_or_array_layers: 1,
        };
        let need = ((phys.width / bw) * (phys.height / bh) * block) as usize;
        if bytes.len() < need {
            log::warn!(
                "texture level {l} of {w}x{h} {:?} has {} bytes, not {need}",
                data.format,
                bytes.len()
            );
            break;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: l as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes[..need],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((phys.width / bw) * block),
                rows_per_image: Some(phys.height / bh),
            },
            phys,
        );
    }
    Some(PreparedTexture(GpuTexture::new(
        texture,
        (w, h),
        texture_bytes(format, w, h, levels),
    )))
}

pub(super) fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &omsi_texture::Image,
    mipmaps: bool,
) -> GpuTexture {
    let mip_count = if mipmaps {
        (32 - img.width.max(img.height).leading_zeros()).max(1)
    } else {
        1
    };
    let size = wgpu::Extent3d {
        width: img.width,
        height: img.height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // CPU box-filter mip chain
    let mut level: Vec<u8> = img.rgba.clone();
    let (mut w, mut h) = (img.width, img.height);
    for mip in 0..mip_count {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        if mip + 1 == mip_count {
            break;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    let mut n = 0;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = (x * 2 + dx).min(w - 1);
                            let sy = (y * 2 + dy).min(h - 1);
                            sum += level[((sy * w + sx) * 4 + c) as usize] as u32;
                            n += 1;
                        }
                    }
                    next[((y * nw + x) * 4 + c) as usize] = (sum / n) as u8;
                }
            }
        }
        level = next;
        w = nw;
        h = nh;
    }
    let view = texture.create_view(&Default::default());
    GpuTexture {
        texture,
        view,
        size: (img.width, img.height),
        bytes: texture_bytes(
            wgpu::TextureFormat::Rgba8UnormSrgb,
            img.width,
            img.height,
            mip_count,
        ),
        gen: next_gen(),
    }
}
