//! Map-authored static AI vehicles. Installed Grundorf supplies missing categories.
use crate::{
    child, files, ini,
    settings::{label, number, value},
    Report,
};
use anyhow::{ensure, Result};
use glam::Vec3;
use omsi_o3d::Vertex;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "proton-import/traffic.jsonoi";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub vehicles: Vec<Vehicle>,
    pub native_cars: bool,
    pub native_buses: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Vehicle {
    pub file: String,
    pub buses: bool,
    pub weight: f32,
    pub speed_kmh: f32,
}

impl Manifest {
    pub fn load(map: &Path) -> Result<Self> {
        let bytes = omsi_cfg::vfs::read(&map.join(FILE))?;
        ensure!(bytes.len() <= 1024 * 1024, "AI manifest exceeds 1 MiB");
        let data: Self = serde_json::from_slice(&bytes)?;
        ensure!(
            data.version == 1 && data.vehicles.len() <= 256,
            "unsupported AI manifest"
        );
        for v in &data.vehicles {
            ensure!(
                v.file.starts_with("maps/")
                    && !v.file.contains(['\\', ':', '\n', '\r'])
                    && !v.file.split('/').any(|p| p == "..")
                    && v.file.ends_with(".ovhoi"),
                "unsafe AI vehicle path"
            );
            ensure!(
                v.weight.is_finite()
                    && (0.0..=100.0).contains(&v.weight)
                    && v.speed_kmh.is_finite()
                    && (1.0..=180.0).contains(&v.speed_kmh),
                "invalid AI vehicle settings"
            );
        }
        Ok(data)
    }
}

pub(crate) struct Local<'a> {
    pub builder: crate::optimise::Builder,
    pub registry: &'a crate::materials::Registry,
}
impl crate::convert::Faces for Local<'_> {
    fn face(&mut self, _: (i32, i32), vertices: [Vertex; 3], material: u16) -> Result<()> {
        self.builder.face_with_mode(
            vertices,
            material,
            &self.registry.materials[material as usize],
            self.registry.modes[material as usize],
        );
        Ok(())
    }
}

pub(crate) fn export(
    dir: &Path,
    base: &Path,
    texture_dir: &Path,
    installed: &str,
    config: &crate::Ini,
    report: &mut Report,
    bytes: &mut u64,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    let mut data = Manifest {
        version: 1,
        ..Default::default()
    };
    for (kind, buses) in [("aicars_v1", false), ("aibuses_v1", true)] {
        let folder = child(base, kind)?;
        if !folder.is_dir() {
            continue;
        }
        // Definitions live directly in this directory; nested credits and licenses
        // are not vehicle definitions and must never be parsed or discarded.
        for file in files(&folder, "txt")?
            .into_iter()
            .filter(|p| p.parent() == Some(folder.as_path()))
        {
            ensure!(!cancel(), "map import cancelled");
            let cfg = ini(&file)?;
            let Some(model) = value(&cfg, "model", "model1") else {
                continue;
            };
            ensure!(data.vehicles.len() < 256, "too many map AI vehicles");
            let model = child(&folder, model)?;
            if !model.is_file() {
                report.warnings.push(format!(
                    "{}: missing AI model; definition omitted",
                    file.display()
                ));
                continue;
            }
            let textures = if let Some(path) = value(&cfg, "config", "textures") {
                child(&folder, path)?
            } else {
                texture_dir.to_path_buf()
            };
            let id = data.vehicles.len();
            let target = dir.join(format!("proton-import/ai/{id}"));
            let models = target.join("Model");
            std::fs::create_dir_all(models.join("texture"))?;
            let presets = crate::texture_settings::Presets::read(&textures, report)?;
            let mut registry = crate::materials::Registry::default();
            let mut scene = crate::three_ds::Scene::open(&model, cancel)?;
            let (names, default) = registry.register(&scene, &presets)?;
            let mut model_cfg = String::new();
            let mut skin_slots = crate::materials::SkinSlots::default();
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let mut object_index = 0;
            scene.visit_import(cancel, |object, repaired| {
                if repaired > 0 {
                    report.warnings.push(format!(
                        "AI {}: repaired {repaired} non-finite UV pairs",
                        object.name
                    ));
                }
                if object.name.contains("_meshcollider") || object.name.contains("_hitcollider") {
                    return Ok(());
                }
                let crate::tags::ObjectKind::Mesh(mode) = crate::tags::classify(&object.name)
                else {
                    return Ok(());
                };
                if mode == crate::tags::Mode::Collision || object.faces.is_empty() {
                    return Ok(());
                }
                let faces = registry.object_faces(&object, &names, default, mode)?;
                let mut local = Local {
                    builder: Default::default(),
                    registry: &registry,
                };
                crate::convert::append_object_local(
                    &object,
                    &faces,
                    &registry.mappings,
                    &mut local,
                    bytes,
                    cancel,
                )?;
                let mut builder = local.builder;
                if builder.mesh.triangles.is_empty() {
                    return Ok(());
                }
                crate::textures(
                    &textures,
                    &models.join("texture"),
                    &mut builder.mesh.materials,
                    &mut report.warnings,
                    cancel,
                )?;
                for v in &builder.mesh.vertices {
                    lo = lo.min(v.position);
                    hi = hi.max(v.position);
                }
                if object.name.contains("_skin001_") {
                    for material in &mut builder.mesh.materials {
                        skin_slots.replace(material, &models.join("texture"))?;
                    }
                }
                let name = format!("body_{object_index}.o3d");
                crate::convert::write_mesh(
                    &models.join(&name),
                    &builder.mesh,
                    &builder.mesh.materials,
                )?;
                model_cfg.push_str(&format!("[mesh]\n{name}\n"));
                crate::export::material_overrides(
                    &mut model_cfg,
                    &builder.mesh.materials,
                    &builder.modes,
                );
                object_index += 1;
                Ok(())
            })?;
            if object_index == 0 {
                report.warnings.push(format!(
                    "{}: no visible AI geometry; definition omitted",
                    file.display()
                ));
                std::fs::remove_dir_all(&target)?;
                continue;
            }
            skins(&folder, &cfg, &target, &skin_slots.names, &mut model_cfg, cancel)?;
            crate::text(&models.join("model.cfg"), &model_cfg)?;
            let size = (hi - lo).max(Vec3::splat(0.1));
            let centre = (hi + lo) * 0.5;
            let mass = number(&cfg, "config", "mass", if buses { 18000.0 } else { 1500.0 })?
                .clamp(100.0, 100_000.0)
                / 1000.0;
            let vehicle_name = label(&file.file_stem().unwrap_or_default().to_string_lossy())?;
            let mut def = format!("[type]\n0\n[ai_veh_type]\n{}\n[mass]\n{mass}\n[boundingbox]\n{}\n{}\n{}\n{}\n{}\n{}\n[model]\nModel/model.cfg\n",if buses {2} else {0},size.x,size.y,size.z,centre.x,centre.y,centre.z);
            sounds(&folder, &cfg, &target, &mut def, report)?;
            crate::text(&target.join("vehicle.ovh"), &def)?;
            data.vehicles.push(Vehicle {
                file: format!("{installed}/proton-import/ai/{id}/vehicle.ovhoi"),
                buses,
                weight: number(&cfg, "config", "priority", 1.0)?.clamp(0.0, 100.0),
                speed_kmh: (number(&cfg, "driving", "speedlimit", 13.0)? * 3.6).clamp(1.0, 180.0),
            });
            report.warnings.push(format!("AI {vehicle_name}: model/materials, skins and basic sound loops imported; driving uses native traffic, not Proton physics or passenger/door scripts"));
        }
    }
    let cars = data.vehicles.iter().any(|v| !v.buses);
    data.native_cars = !cars || value(config, "ai_traffic", "disablenativetraffic") != Some("1");
    data.native_buses = !data.vehicles.iter().any(|v| v.buses);
    report.ai_vehicles_imported = data.vehicles.len();
    let mut writer = std::io::BufWriter::new(std::fs::File::create(
        dir.join("proton-import/traffic.json"),
    )?);
    serde_json::to_writer(&mut writer, &data)?;
    std::io::Write::flush(&mut writer)?;
    Ok(())
}

fn skins(
    folder: &Path,
    cfg: &crate::Ini,
    target: &Path,
    slots: &[String],
    out: &mut String,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    let Some(path) = value(cfg, "config", "randomskins") else {
        return Ok(());
    };
    if slots.is_empty() {
        return Ok(());
    }
    let folder = child(folder, path)?;
    if !folder.is_dir() {
        return Ok(());
    }
    let skins = files(&folder, "png")?;
    ensure!(skins.len() <= 256, "too many AI skins");
    std::fs::create_dir(target.join("Skins"))?;
    for (i, slot) in slots.iter().enumerate() {
        out.push_str(&format!("[CTCTexture]\nskin_{i}\n{slot}\n"));
    }
    let mut bytes = 0u64;
    for (i, file) in skins.iter().enumerate() {
        ensure!(!cancel(), "map import cancelled");
        bytes += file.metadata()?.len();
        ensure!(bytes <= 256 * 1024 * 1024, "AI skin images exceed 256 MiB");
        let name = format!("{i}.png");
        std::fs::copy(file, target.join("Skins").join(&name))?;
        let paint = label(&file.file_stem().unwrap_or_default().to_string_lossy())?;
        for slot in 0..slots.len() {
            out.push_str(&format!("[item]\n{paint}\nskin_{slot}\nSkins/{name}\n"));
        }
    }
    Ok(())
}

fn sounds(
    folder: &Path,
    cfg: &crate::Ini,
    target: &Path,
    def: &mut String,
    report: &mut Report,
) -> Result<()> {
    let mut out = String::new();
    let mut found = false;
    for (key, relation) in [("stopped", 4), ("moving", 3)] {
        let Some(path) = value(cfg, "sounds_v1", key) else {
            continue;
        };
        let file = child(folder, path)?;
        if !file.is_file() {
            report.warnings.push(format!("missing AI sound {path}"));
            continue;
        }
        ensure!(
            file.metadata()?.len() <= 64 * 1024 * 1024,
            "AI sound exceeds 64 MiB"
        );
        let name = format!(
            "{key}.{}",
            file.extension().unwrap_or_default().to_string_lossy()
        );
        std::fs::create_dir_all(target.join("Sound"))?;
        std::fs::copy(file, target.join("Sound").join(&name))?;
        let volume = number(cfg, "sounds_v1", "volume", 0.8)?.clamp(0.0, 4.0);
        let pos = crate::settings::position(cfg, "sounds_v1")?;
        let range = number(cfg, "sounds_v1", "maxdistance", 250.0)?.clamp(1.0, 1000.0);
        out.push_str(&format!("[loopsound]\n{name}\n44100\n\n1\n{volume}\n[3d]\n{}\n{}\n{}\n{range}\n[conditionSingle]\nvelocity\n2\n{relation}\n",pos[0],pos[1],pos[2]));
        found = true;
    }
    if found {
        crate::text(&target.join("Sound/sound.cfg"), &out)?;
        def.push_str("[sound_ai]\nSound/sound.cfg\n");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{setup, Fixture};
    #[test]
    fn source_ai_is_an_oi_vehicle_with_relative_model_and_sounds() {
        let f = Fixture::new();
        setup(&f);
        let ai = f.0.join("Town/aibuses_v1");
        std::fs::create_dir(&ai).unwrap();
        std::fs::write(ai.join("Bus.txt"), "[model]\nmodel1=bus.3ds\n[config]\nmass=18000\npriority=2\n[driving]\nspeedLimit=10\n[sounds_v1]\nmoving=engine.wav\n").unwrap();
        std::fs::write(ai.join("bus.3ds"), crate::vehicle_tests::local_model()).unwrap();
        std::fs::write(ai.join("engine.wav"), b"fixture").unwrap();
        let report = crate::import(&f.0, "maps/Proton-Town", || false).unwrap();
        assert_eq!(report.ai_vehicles_imported, 1);
        let manifest = Manifest::load(&f.0).unwrap();
        assert!(manifest.native_cars && !manifest.native_buses);
        assert_eq!(manifest.vehicles[0].speed_kmh, 36.0);
        assert_eq!(manifest.vehicles[0].weight, 2.0);
        let base = f.0.join("proton-import/ai/0");
        let def = omsi_vehicle::Vehicle::load(&base.join("vehicle.ovhoi")).unwrap();
        assert_eq!(def.ai_veh_type, 2);
        assert_eq!(def.mass, 18.0);
        assert_eq!(def.model.as_deref(), Some("Model/model.cfgoi"));
        assert_eq!(def.sound_ai.as_deref(), Some("Sound/sound.cfgoi"));
        let cfg = omsi_cfg::vfs::read_text(&base.join("Model/model.cfgoi")).unwrap();
        assert!(cfg.contains("body_0.o3doi"));
        let mesh = omsi_o3d::load_mesh(&base.join("Model/body_0.o3doi")).unwrap();
        assert!(mesh.vertices.iter().all(|v| v.position.abs().max_element() < 10.));
        assert_eq!(
            std::fs::read(base.join("Sound/moving.wav")).unwrap(),
            b"fixture"
        );
    }
}
