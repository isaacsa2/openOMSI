//! Conservative map drawing from tagged/textured road faces, with no inferred AI links.
use crate::{navigation::Navigation, texture_settings::Presets, three_ds::Object, Report};
use omsi_o3d::Material;
const LIMIT: usize = 32_768;

fn named(s: &str, terms: &[&str]) -> bool {
    s.split(|c: char| !c.is_alphanumeric()).any(|part| {
        let part = part
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .to_lowercase();
        terms.contains(&part.as_str())
    })
}
fn road(s: &str) -> bool {
    named(
        s,
        &[
            "road", "roads", "asphalt", "asfalto", "rua", "avenida", "estrada", "pista", "dirtroad",
        ],
    )
}

pub(crate) fn collect(
    object: &Object,
    faces: &[u16],
    materials: &[Material],
    presets: &Presets,
    nav: &mut Navigation,
    report: &mut Report,
) {
    if named(
        &object.name,
        &[
            "sidewalk", "calcada", "calçada", "roof", "telhado", "wall", "muro", "sign", "placa",
            "grass", "grama",
        ],
    ) {
        return;
    }
    let named = road(&object.name);
    for (face, &material) in object.faces.iter().zip(faces) {
        let material = &materials[material as usize];
        if !named && !road(&material.texture) && !presets.is_road(&material.texture) {
            continue;
        }
        if nav.road_triangles.len() >= LIMIT {
            if !report
                .warnings
                .iter()
                .any(|w| w.starts_with("3D road overview reached"))
            {
                report.warnings.push("3D road overview reached its 32768-face budget; remaining faces stay in scenery but are omitted from the navigator".into());
            }
            break;
        }
        let [Some(a), Some(b), Some(c)] = face.map(|i| object.positions.get(i as usize).copied())
        else {
            continue;
        };
        let normal = (b - a).cross(c - a);
        // Road decks and ramps only: not asphalt-textured walls or degenerate faces.
        if normal.length_squared() < 0.001 || normal.z.abs() < normal.length() * 0.7 {
            continue;
        }
        nav.road_triangles.push([
            a.truncate().to_array(),
            b.truncate().to_array(),
            c.truncate().to_array(),
        ]);
        report.road_surface_triangles = nav.road_triangles.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    #[test]
    fn road_decks_are_visual_only_and_walls_and_sidewalks_are_excluded() {
        let mut object = Object {
            name: "Asfalto04".into(),
            positions: vec![
                Vec3::ZERO,
                Vec3::new(20., 0., 0.),
                Vec3::new(0., 20., 0.),
                Vec3::new(0., 0., 20.),
            ],
            faces: vec![[0, 1, 2], [0, 1, 3]],
            ..Default::default()
        };
        let mut nav = Navigation::default();
        let mut report = Report::default();
        collect(
            &object,
            &[0, 0],
            &[Material::default()],
            &Presets::default(),
            &mut nav,
            &mut report,
        );
        assert_eq!(nav.road_triangles.len(), 1);
        assert!(nav.roads.is_empty());
        object.name = "sidewalk_asphalt".into();
        collect(
            &object,
            &[0, 0],
            &[Material::default()],
            &Presets::default(),
            &mut nav,
            &mut report,
        );
        assert_eq!(nav.road_triangles.len(), 1);
    }
}
