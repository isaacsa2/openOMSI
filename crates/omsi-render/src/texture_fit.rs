//! A texture or picture larger than the graphics chip takes, made to fit it: its smaller
//! levels when it has them, else halved until it fits.

/// A texture bigger than the graphics chip takes (`max` texels a side - 16384 on most, 2048
/// or 4096 on older ones) made to fit: its smaller levels when it has them, else the picture
/// halved until it fits. None when it fits as it is. A too big texture used to be a device
/// error, and the material that used it one too.
pub fn fit_texture(data: &omsi_texture::TextureData, max: u32) -> Option<omsi_texture::TextureData> {
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
        return Some(omsi_texture::TextureData { width: (w >> k).max(1), height: (h >> k).max(1), levels: data.levels[k as usize..].to_vec(), ..data.clone() });
    }
    // one level only: decode it and halve it
    let mut rgba = match (data.format, data.levels.first()) {
        (PixelFormat::Rgba8, Some(l)) => l.clone(),
        (f, Some(l)) => omsi_texture::bc::decode(l, w, h, match f {
            PixelFormat::Bc1 => omsi_texture::bc::Bc::Bc1 { punch: true },
            PixelFormat::Bc2 => omsi_texture::bc::Bc::Bc2,
            _ => omsi_texture::bc::Bc::Bc3,
        }),
        _ => return None,
    };
    let (mut cw, mut ch) = (w, h);
    for _ in 0..k {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let at = |xx: u32, yy: u32| rgba[((yy.min(ch - 1) * cw + xx.min(cw - 1)) * 4 + c) as usize] as u32;
                    let v = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                    next[((y * nw + x) * 4 + c) as usize] = (v / 4) as u8;
                }
            }
        }
        rgba = next;
        (cw, ch) = (nw, nh);
    }
    Some(omsi_texture::TextureData { width: cw, height: ch, format: PixelFormat::Rgba8, levels: vec![rgba], has_alpha: data.has_alpha, gpu_mips: true })
}

/// A size halved (both sides, as `fit_texture` halves a picture) until neither passes `max`.
pub(super) fn fit_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    let (mut w, mut h) = (w, h);
    while w > max || h > max {
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
    }
    (w, h)
}

/// An RGBA picture of the game's own (a script's or a sign's text, an HTML display) larger
/// than the graphics chip takes, halved until it fits (None when it fits as it is). Made at
/// its full size, the texture was a device error, and so was every frame's write into it.
pub(super) fn fit_image(img: &omsi_texture::Image, max: u32) -> Option<omsi_texture::Image> {
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
    Some(omsi_texture::Image { width: small.width, height: small.height, rgba: small.levels.into_iter().next().unwrap_or_default(), has_alpha: img.has_alpha })
}

#[cfg(test)]
mod fit_tests {
    #[test]
    fn a_big_picture_is_halved_until_it_fits() {
        let data = omsi_texture::TextureData { width: 8, height: 4, format: omsi_texture::PixelFormat::Rgba8, levels: vec![vec![200; 8 * 4 * 4]], has_alpha: false, gpu_mips: true };
        let small = super::fit_texture(&data, 2).unwrap();
        assert_eq!((small.width, small.height), (2, 1));
        assert_eq!(small.levels[0].len(), 2 * 4);
        assert!(super::fit_texture(&data, 8).is_none());
        // a picture of the game's own, and a size, the same way
        let img = omsi_texture::Image { width: 5000, height: 300, rgba: vec![9; 5000 * 300 * 4], has_alpha: true };
        let small = super::fit_image(&img, 2048).unwrap();
        assert_eq!((small.width, small.height), (1250, 75));
        assert_eq!(small.rgba.len(), 1250 * 75 * 4);
        assert_eq!(super::fit_size(5000, 300, 2048), (1250, 75));
        assert_eq!(super::fit_size(2048, 16, 2048), (2048, 16));
        assert!(super::fit_image(&small, 2048).is_none());
    }
}
