/// Layout and physical pixels per UI pixel, chosen before applying UI zoom.
pub(super) struct Viewport {
    pub compact: bool,
    pub scale: f32,
}

pub(super) fn viewport(width: f32, height: f32, dpi: f32, mobile: bool) -> Viewport {
    let (width, height) = (width / dpi, height / dpi);
    let compact = mobile || width < 1360.0 || height < 760.0;
    let zoom = if mobile {
        (height / 400.0).clamp(1.0, 1.35)
    } else if compact {
        1.0
    } else {
        (width / 1440.0).min(height / 820.0).clamp(0.8, 2.2)
    };
    Viewport {
        compact,
        scale: dpi * zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_windows_use_compact_layout_at_system_text_size() {
        for (width, height, dpi) in [
            (1280.0, 800.0, 1.0),
            (1280.0, 800.0, 2.0),
            (1024.0, 600.0, 1.0),
            (1920.0, 1080.0, 1.5),
        ] {
            let view = viewport(width, height, dpi, false);
            assert!(view.compact);
            assert_eq!(view.scale, dpi);
        }
    }

    #[test]
    fn desktop_layout_keeps_its_existing_zoom() {
        for (width, height, dpi) in [
            (1440.0, 880.0, 1.0),
            (1920.0, 1080.0, 1.0),
            (3840.0, 2160.0, 2.0),
        ] {
            let view = viewport(width, height, dpi, false);
            assert!(!view.compact);
            assert_eq!(
                view.scale,
                dpi * (width / dpi / 1440.0)
                    .min(height / dpi / 820.0)
                    .clamp(0.8, 2.2)
            );
        }
    }

    #[test]
    fn mobile_zoom_and_pre_zoom_breakpoints_are_stable() {
        for height in [400.0, 800.0, 1600.0] {
            let view = viewport(1600.0, height, 2.0, true);
            assert!(view.compact);
            assert_eq!(view.scale, 2.0 * (height / 2.0 / 400.0).clamp(1.0, 1.35));
        }
        assert!(viewport(1359.0, 900.0, 1.0, false).compact);
        assert!(!viewport(1360.0, 900.0, 1.0, false).compact);
        assert!(viewport(1440.0, 759.0, 1.0, false).compact);
        assert!(!viewport(1440.0, 760.0, 1.0, false).compact);
    }
}
