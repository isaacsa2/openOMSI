//! Read-only preparation for vehicle support. This does not convert or install a bus.
use crate::{
    child, ini,
    settings::{label, number, position, value},
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Inspection {
    pub name: String,
    pub version: f32,
    pub legacy_ini: bool,
    pub base: PathBuf,
    pub template: String,
    pub mass_kg: f32,
    pub driver_camera: [f32; 3],
    pub components: Vec<Component>,
    pub models: Vec<PathBuf>,
    pub doors: Vec<Door>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Component {
    pub role: String,
    pub file: PathBuf,
    pub present: bool,
}
#[derive(Debug, Serialize)]
pub struct Door {
    pub name: String,
    pub position: [f32; 3],
    pub entrance: bool,
    pub exit: bool,
    pub from_model: bool,
}

/// Inspect the creator's files without writing a declaration, output, or source file.
pub fn inspect(file: &Path) -> Result<Inspection> {
    let filename = file
        .file_name()
        .context("vehicle config has no filename")?
        .to_string_lossy()
        .to_ascii_lowercase();
    let legacy_ini = filename.ends_with(".ini");
    ensure!(
        legacy_ini || filename.ends_with(".bus.txt"),
        "select a Proton .bus.txt or legacy .ini vehicle configuration"
    );
    let cfg = ini(file)?;
    let version = number(&cfg, "mod", "busmodversion", 1.)?;
    ensure!(
        (1.0..=2.0).contains(&version),
        "unsupported Proton vehicle version {version}"
    );
    let parent = file.parent().unwrap_or(Path::new("."));
    let base = child(
        parent,
        value(&cfg, "config", "basedir").context("missing vehicle baseDir")?,
    )?;
    ensure!(base.is_dir(), "missing vehicle base folder");
    let mut report = Inspection {
        name: label(value(&cfg, "mod", "name").unwrap_or(&filename))?,
        version,
        legacy_ini,
        base: base.clone(),
        template: value(&cfg, "data", "template").unwrap_or("").into(),
        mass_kg: number(&cfg, "data", "mass1", 0.)?,
        driver_camera: position(&cfg, "posdriverhead")?,
        components: vec![],
        models: vec![],
        doors: vec![],
        warnings: vec![],
    };
    ensure!(
        report.mass_kg >= 0. && report.mass_kg <= 200_000.,
        "invalid vehicle mass"
    );
    for key in [
        "models1",
        "engine1auto",
        "engine1manual",
        "wheels1",
        "lights1",
        "sounds1",
        "seats1",
    ] {
        let Some(path) = value(&cfg, "config", key) else {
            continue;
        };
        let path = child(&base, path)?;
        let present = path.is_file();
        if !present {
            report
                .warnings
                .push(format!("missing {key}: {}", path.display()));
        }
        report.components.push(Component {
            role: key.into(),
            file: path.clone(),
            present,
        });
        if key != "models1" || !present {
            continue;
        }
        let models = ini(&path)?;
        for (section, entries) in &models {
            if !(section.starts_with("part") && section.ends_with("_3ds")) && section != "model" {
                continue;
            }
            let mut ordered: Vec<_> = entries
                .iter()
                .filter_map(|(key, path)| {
                    key.strip_prefix("model")
                        .and_then(|n| n.parse::<u16>().ok())
                        .map(|n| (n, key, path))
                })
                .collect();
            ordered.sort_by_key(|(n, _, _)| *n);
            for (_, key, path) in ordered {
                if !key
                    .strip_prefix("model")
                    .is_some_and(|n| n.parse::<u16>().is_ok())
                {
                    continue;
                }
                ensure!(
                    report.models.len() < 64,
                    "too many vehicle model references"
                );
                let model = child(&base, path)?;
                if !model.is_file() {
                    report
                        .warnings
                        .push(format!("missing vehicle model: {}", model.display()));
                }
                report.models.push(model);
            }
        }
    }
    ensure!(
        !report.components.iter().all(|c| c.role != "models1"),
        "vehicle has no models1 definition"
    );
    if let Some(doors) = cfg.get("doors") {
        ensure!(doors.len() <= 32, "too many vehicle doors");
        for (name, path) in doors {
            if !name.starts_with("rightdoor") && !name.starts_with("leftdoor") {
                continue;
            }
            let file = child(&base, path)?;
            report.components.push(Component {
                role: name.clone(),
                present: file.is_file(),
                file: file.clone(),
            });
            if !file.is_file() {
                report.warnings.push(format!("missing door {name}"));
                continue;
            }
            let door = ini(&file)?;
            ensure!(door.contains_key(name), "door file has no [{name}] section");
            report.doors.push(Door {
                name: name.clone(),
                position: position(&door, name)?,
                entrance: number(&door, name, "isentrance", 1.)? != 0.,
                exit: number(&door, name, "isexit", 0.)? != 0.,
                from_model: number(&door, name, "getpathsfrommodel", 1.)? != 0.,
            });
        }
    }
    for key in ["textures", "skins"] {
        if let Some(path) = value(&cfg, "folders", key) {
            let path = child(&base, path)?;
            report.components.push(Component {
                role: key.into(),
                present: path.is_dir(),
                file: path,
            });
        }
    }
    report.warnings.push("Read-only vehicle inspection; no drivable bus, engine scripts, wheel/door animation or passenger cabin is generated".into());
    if legacy_ini {
        report.warnings.push(
            "Legacy .ini configuration detected; do not treat it as a native OMSI vehicle".into(),
        );
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_vehicle_sections_resolve_safely_without_generating_files() {
        let f = crate::tests::Fixture::new();
        std::fs::create_dir_all(f.0.join("Bus/Scripts")).unwrap();
        let main = f.0.join("Bus.ini");
        let text="[mod]\nname=Fixture\nbusModVersion=2\n[config]\nbaseDir=Bus\nmodels1=Scripts/models.txt\n[data]\nmass1=12000\ntemplate=4x2\n[doors]\nrightdoor1=Scripts/door.txt\n[posDriverHead]\nposX=-1\nposY=2\nposZ=4\n";
        std::fs::write(&main, text).unwrap();
        std::fs::write(
            f.0.join("Bus/Scripts/models.txt"),
            "[part1_3ds]\nmodel1=body.3ds\n",
        )
        .unwrap();
        std::fs::write(f.0.join("Bus/body.3ds"), crate::tests::model()).unwrap();
        std::fs::write(
            f.0.join("Bus/Scripts/door.txt"),
            "[rightdoor1]\nposX=1\nposY=0\nposZ=3\nisEntrance=1\nisExit=0\n",
        )
        .unwrap();
        let report = inspect(&main).unwrap();
        assert!(report.legacy_ini);
        assert_eq!(report.models.len(), 1);
        assert_eq!(report.mass_kg, 12000.);
        assert_eq!(report.driver_camera, [-1., 4., 2.]);
        assert!(report.doors[0].entrance && !report.doors[0].exit);
        assert_eq!(std::fs::read_to_string(&main).unwrap(), text);
        assert!(!f.0.join("proton-import").exists());
        std::fs::write(
            f.0.join("Bus/Scripts/models.txt"),
            "[part1_3ds]\nmodel1=../external.3ds\n",
        )
        .unwrap();
        assert!(inspect(&main).is_err());
    }
}
