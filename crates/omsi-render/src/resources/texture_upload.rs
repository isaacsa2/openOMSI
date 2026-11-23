use super::*;

pub(crate) fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &omsi_texture::Image,
    mipmaps: bool,
) -> GpuTexture {
    upload_texture_format(
        device,
        queue,
        img,
        mipmaps,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
}

pub(crate) fn upload_texture_format(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &omsi_texture::Image,
    mipmaps: bool,
    format: wgpu::TextureFormat,
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
    let Some(texture) = try_create_texture(
        device,
        &wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
    ) else {
        return stand_in_texture(device, queue);
    };
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
        if format.is_srgb() {
            // Match GPU colour mips, including coverage weighting at clear edges.
            let (next, nw, nh) = omsi_texture::bc::downsample_alpha_weighted(&level, w, h);
            (level, w, h) = (next, nw, nh);
            continue;
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
        bytes: texture_bytes(format, img.width, img.height, mip_count),
        generation: next_gen(),
    }
}

/// A texture bigger than the graphics chip takes (`max` texels a side - 16384 on most, 2048
/// or 4096 on older ones) made to fit: its smaller levels when it has them, else the picture
/// halved until it fits. None when it fits as it is. A too big texture used to be a device
/// error, and the material that used it one too.
pub fn fit_texture(
    data: &omsi_texture::TextureData,
    max: u32,
) -> Option<omsi_texture::TextureData> {
    use omsi_texture::PixelFormat;
    let (w, h) = (data.width.max(1), data.height.max(1));
    if w <= max && h <= max {
        return None;
    }
    let mut k = 0u32;
    while (w >> k).max(1) > max || (h >> k).max(1) > max {
        k += 1;
    }
    if (k as usize) < data.levels.len() {
        return Some(omsi_texture::TextureData {
            width: (w >> k).max(1),
            height: (h >> k).max(1),
            levels: data.levels[k as usize..].to_vec(),
            format: data.format,
            has_alpha: data.has_alpha,
            gpu_mips: data.gpu_mips,
        });
    }
    // one level only: decode it and halve it
    let mut rgba = match data.levels.first() {
        Some(l) => data.format.decode_level(l, w, h, true),
        None => return None,
    };
    let (mut cw, mut ch) = (w, h);
    for _ in 0..k {
        let (next, nw, nh) = omsi_texture::bc::downsample_alpha_weighted(&rgba, cw, ch);
        rgba = next;
        (cw, ch) = (nw, nh);
    }
    Some(omsi_texture::TextureData {
        width: cw,
        height: ch,
        format: PixelFormat::Rgba8,
        levels: vec![rgba],
        has_alpha: data.has_alpha,
        gpu_mips: true,
    })
}

/// A size halved (both sides, as `fit_texture` halves a picture) until neither passes `max`.
pub(crate) fn fit_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    let (mut w, mut h) = (w, h);
    while w > max || h > max {
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
    }
    (w, h)
}

/// An RGBA picture of the game's own (a script's or a sign's text, an HTML display) larger
/// than the graphics chip takes, halved until it fits (None when it fits as it is). Made at
/// its full size, the texture was a device error, and so was every frame's write into it.
pub(crate) fn fit_image(img: &omsi_texture::Image, max: u32) -> Option<omsi_texture::Image> {
    if img.width <= max && img.height <= max {
        return None;
    }
    let data = omsi_texture::TextureData {
        width: img.width,
        height: img.height,
        format: omsi_texture::PixelFormat::Rgba8,
        levels: vec![img.rgba.clone()],
        has_alpha: img.has_alpha,
        gpu_mips: true,
    };
    let small = fit_texture(&data, max)?;
    Some(omsi_texture::Image {
        width: small.width,
        height: small.height,
        rgba: small.levels.into_iter().next().unwrap_or_default(),
        has_alpha: img.has_alpha,
    })
}

