use super::{graphics_mode, Value};
use serde_json::json;

/// Quality presets for the default renderer, Vanilla+.
pub fn graphics_presets() -> [(&'static str, Value); 4] {
    graphics_presets_for("vanilla_plus")
}

/// Quality levels within the explicitly selected renderer, shared by both interfaces.
/// A quality change never opts into Enhanced or ray tracing. Texture memory stays
/// automatic, and the renderer retains its device/format limits and fallback policy.
pub fn graphics_presets_for(mode: &str) -> [(&'static str, Value); 4] {
    let mode = graphics_mode(mode);
    let classic = mode == "vanilla";
    let traced = mode == "enhanced_plus";
    let preset = |msaa,
                  anisotropy,
                  shadow_size,
                  ssao,
                  shadows,
                  detail,
                  clouds,
                  distance,
                  size,
                  object_distance,
                  mirror_size,
                  mirror_refresh,
                  scale| {
        json!({"graphics": mode, "msaa": msaa, "anisotropy": anisotropy,
            "shadow_size": shadow_size, "ssao": traced || (ssao && !classic),
            "shadows": traced || (shadows && !classic),
            "detail_textures": detail && !classic, "clouds": clouds, "windy_trees": clouds,
            "reflections": traced || detail, "view_distance": distance, "min_obj_size": size,
            "max_obj_dist": object_distance, "mirror_size": mirror_size,
            "mirror_refresh": mirror_refresh, "render_scale": scale, "texture_memory": 0})
    };
    let mut levels = [
        (
            "Low",
            preset(
                1, 2, 1024, false, false, false, false, "600", 0.03, "500", 128, "eco", "0.75",
            ),
        ),
        (
            "Medium",
            preset(
                2, 4, 1024, false, true, true, true, "900", 0.02, "750", 256, "eco", "auto",
            ),
        ),
        (
            "High",
            preset(
                4, 8, 2048, true, true, true, true, "auto", 0.013, "auto", 256, "full", "auto",
            ),
        ),
        (
            "Ultra",
            preset(
                4, 8, 4096, true, true, true, true, "2000", 0.005, "1500", 512, "full", "auto",
            ),
        ),
    ];
    for (i, (_, settings)) in levels.iter_mut().enumerate() {
        match mode {
            "enhanced" => {
                settings["msaa"] = json!([1, 1, 2, 4][i]);
                settings["render_scale"] = json!(["0.67", "0.85", "auto", "auto"][i]);
            }
            "enhanced_plus" => {
                settings["msaa"] = json!([1, 1, 1, 2][i]);
                settings["shadow_size"] = json!([1024, 1024, 2048, 2048][i]);
                settings["mirror_size"] = json!([128, 128, 256, 512][i]);
                settings["mirror_refresh"] = json!(["eco", "eco", "eco", "full"][i]);
                settings["render_scale"] = json!(["0.5", "0.67", "0.85", "auto"][i]);
            }
            _ => {}
        }
        if matches!(mode, "enhanced" | "enhanced_plus") && i == 2 {
            settings["view_distance"] = json!("1200");
            settings["max_obj_dist"] = json!("900");
        }
    }
    levels
}
