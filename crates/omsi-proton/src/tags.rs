//! Mesh-name conventions documented by the Proton/Ciano map loader.
//! https://www.busmods.com/ciano/help/map-models.html
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mode {
    #[default]
    Auto,
    Blend,
    Cutout,
    Collision,
    Emissive,
    EmissiveBlend,
    EmissiveCutout,
    Additive,
}

pub(crate) enum ObjectKind {
    Mesh(Mode),
    Helper,
    EmbeddedTree,
}

pub(crate) fn classify(name: &str) -> ObjectKind {
    // These are gameplay volumes, even when they also carry a collision tag.
    if ["_low_speed_zone_", "_force_exit_", "_spawn_blocker_"]
        .iter()
        .any(|tag| name.contains(tag))
    {
        return ObjectKind::Helper;
    }
    if name.contains("_invisible_") {
        return if name.contains("_gencol_") {
            ObjectKind::Mesh(Mode::Collision)
        } else {
            ObjectKind::Helper
        };
    }
    // The source box is only a placement marker for a built-in game asset.
    if name.match_indices("_utree").any(|(i, _)| {
        name.as_bytes()
            .get(i + 6..i + 10)
            .is_some_and(|s| s[..3].iter().all(u8::is_ascii_digit) && s[3] == b'_')
    }) {
        return ObjectKind::EmbeddedTree;
    }
    ObjectKind::Mesh(if name.contains("_additive_") {
        Mode::Additive
    } else if name.contains("_emissive_") {
        if name.contains("_transparent_") { Mode::EmissiveBlend }
        else if name.contains("_cutout_") { Mode::EmissiveCutout }
        else { Mode::Emissive }
    } else if name.contains("_transparent_") {
        Mode::Blend
    } else if name.contains("_cutout_") {
        Mode::Cutout
    } else {
        Mode::Auto
    })
}
