//! Authored phase-4 instances feed the same bounded scene converter as tile meshes.
use crate::{
    child, ini,
    settings::{number, position, value},
    Report,
};
use anyhow::{ensure, Context, Result};
use glam::{EulerRot, Mat4, Quat, Vec3};
use std::path::{Path, PathBuf};

pub(crate) struct Placement {
    pub model: PathBuf,
    pub transform: Mat4,
}

pub(crate) fn read(base: &Path, models: &Path, report: &mut Report) -> Result<Vec<Placement>> {
    let file = child(models, "prefabs.txt")?;
    if !file.is_file() {
        return Ok(Vec::new());
    }
    let cfg = ini(&file)?;
    let count = number(&cfg, "config", "count", 10.0)?;
    ensure!(
        count.fract() == 0.0 && (0.0..=50_000.0).contains(&count),
        "invalid prefab count"
    );
    let folder = child(base, "prefabs")?;
    let mut out = Vec::new();
    for index in 1..=count as usize {
        let section = format!("prefab{index}");
        if !cfg.contains_key(&section) {
            continue;
        }
        let name = value(&cfg, &section, "prefab").context("prefab instance has no definition")?;
        ensure!(
            !name.contains(['/', '\\', ':']) && name != "." && name != "..",
            "unsafe prefab name"
        );
        let def = child(&folder, &format!("{name}.txt"))?;
        if !def.is_file() {
            report.warnings.push(format!(
                "{}: missing prefab {name}; instance {index} omitted",
                file.display()
            ));
            continue;
        }
        let def = ini(&def)?;
        let path = value(&def, "prefab", "model1").context("prefab has no model1")?;
        let model = child(&folder, path)?;
        if !model.is_file() {
            report.warnings.push(format!(
                "prefab {name}: missing model {path}; instance omitted"
            ));
            continue;
        }
        let pos = Vec3::from(position(&cfg, &section)?);
        let angles = Vec3::new(
            number(&cfg, &section, "rotx", 0.0)?,
            number(&cfg, &section, "roty", 0.0)?,
            number(&cfg, &section, "rotz", 0.0)?,
        ) * std::f32::consts::PI
            / 180.0;
        // Unity's Euler Z-X-Y order, reflected from Y-up to the engine's Z-up.
        let rotation = Quat::from_euler(EulerRot::ZXY, -angles.y, -angles.x, -angles.z);
        let scale = if value(&cfg, &section, "usecustomscale") == Some("1") {
            Vec3::new(
                number(&cfg, &section, "scalex", 1.0)?,
                number(&cfg, &section, "scalez", 1.0)?,
                number(&cfg, &section, "scaley", 1.0)?,
            )
        } else {
            Vec3::ONE
        };
        ensure!(
            scale.is_finite()
                && scale.abs().min_element() >= 0.0001
                && scale.abs().max_element() <= 10_000.0,
            "invalid prefab scale"
        );
        out.push(Placement {
            model,
            transform: Mat4::from_scale_rotation_translation(scale, rotation, pos),
        });
    }
    report.prefabs_imported = out.len();
    Ok(out)
}

pub(crate) fn transform(object: &mut crate::three_ds::Object, matrix: Mat4) -> Result<()> {
    for p in &mut object.positions {
        *p = matrix.transform_point3(*p);
        ensure!(
            p.is_finite() && p.abs().max_element() < 1_000_000.0,
            "prefab position exceeds map bounds"
        );
    }
    // Recomputed face normals inherit the affine transform. A mirrored instance
    // needs reversed winding so the visible side and its collision agree.
    if matrix.determinant() < 0.0 {
        for face in &mut object.faces {
            face.swap(1, 2);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{setup, Fixture};
    #[test]
    fn authored_instance_transforms_and_mirrored_winding_are_preserved() {
        let f = Fixture::new();
        setup(&f);
        std::fs::create_dir_all(f.0.join("Town/prefabs")).unwrap();
        std::fs::write(
            f.0.join("Town/prefabs/tree.txt"),
            "[prefab]\nmodel1=tree.3ds\n",
        )
        .unwrap();
        std::fs::write(f.0.join("Town/prefabs/tree.3ds"), crate::tests::model()).unwrap();
        std::fs::write(f.0.join("Town/tiles/0x0/prefabs.txt"), "[config]\ncount=1\n[prefab1]\nprefab=tree\nposX=10\nposY=30\nposZ=20\nrotY=90\nuseCustomScale=1\nscaleX=-2\nscaleY=3\nscaleZ=4\n").unwrap();
        let mut report = Report::default();
        let placements = read(&f.0.join("Town"), &f.0.join("Town/tiles/0x0"), &mut report).unwrap();
        assert_eq!(report.prefabs_imported, 1);
        let p = placements[0].transform.transform_point3(Vec3::X);
        assert!((p - Vec3::new(10.0, 22.0, 30.0)).length() < 1e-4);
        let mut scene = crate::three_ds::Scene::open(&placements[0].model, &|| false).unwrap();
        scene
            .visit_import(&|| false, |mut object, _| {
                let face = object.faces[0];
                transform(&mut object, placements[0].transform)?;
                assert_eq!(object.faces[0], [face[0], face[2], face[1]]);
                assert_eq!(object.name, "road");
                Ok(())
            })
            .unwrap();
        std::fs::write(
            f.0.join("Town/tiles/0x0/prefabs.txt"),
            "[config]\ncount=1\n[prefab1]\nprefab=../escape\n",
        )
        .unwrap();
        assert!(read(&f.0.join("Town"), &f.0.join("Town/tiles/0x0"), &mut report).is_err());
    }
}
