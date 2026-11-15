//! One mesh at a time, shared 3DS/material/O3D conversion, native vehicle definitions.
use crate::{
    ini,
    settings::{number, position},
    vehicle::Inspection,
    vehicle_package::{self, Report},
    vehicle_physics::Physics,
};
use anyhow::{ensure, Context, Result};
use omsi_cfg::oi::{self, Kind};
use std::path::Path;

pub(crate) fn export(
    file: &Path,
    info: &Inspection,
    target: &Path,
    cancel: &impl Fn() -> bool,
) -> Result<Report> {
    let cfg = ini(file)?;
    let physics = Physics::read(info)?;
    // Reject unsupported parts rather than accidentally showing a rear section at the origin.
    ensure!(
        number(&cfg, "config", "offloadanimations", 0.)? == 0.,
        "offloaded/script vehicle animations are not supported"
    );
    let models_file = info
        .components
        .iter()
        .find(|c| c.role == "models1" && c.present)
        .context("missing models1")?;
    let models_cfg = ini(&models_file.file)?;
    ensure!(
        !models_cfg
            .keys()
            .any(|n| n.starts_with("part") && n.ends_with("_3ds") && n != "part1_3ds"),
        "articulated models need separate parts; conversion blocked"
    );
    let mut report = Report::default();
    let mut material_report = crate::Report::default();
    let texture_dir = info
        .components
        .iter()
        .find(|c| c.role == "textures" && c.present)
        .context("missing vehicle texture folder")?
        .file
        .clone();
    let presets = crate::texture_settings::Presets::read(&texture_dir, &mut material_report)?;
    let folder = target.join("Model");
    std::fs::create_dir_all(folder.join("Texture"))?;
    std::fs::create_dir_all(target.join("Script"))?;
    let mut model = "[texttexture]\nIBIS_terminus_name\nAnnax Small\n1024\n128\n0\n255\n180\n0\n[texttexture]\nSetLineTo\nAnnax Large\n256\n128\n0\n255\n180\n0\n".to_string();
    let mut registry = crate::materials::Registry::default();
    let mut bytes = 0;
    let mut cabin = crate::vehicle_cabin::Cabin::default();
    let mut wipers = [0; 2];
    let mut skin_slots = crate::materials::SkinSlots::default();
    for (model_index, file) in info.models.iter().enumerate() {
        ensure!(!cancel(), "vehicle conversion cancelled");
        let mut scene = crate::three_ds::Scene::open(file, cancel)?;
        let (names, default) = registry.register(&scene, &presets)?;
        let key = format!("model{}", model_index + 1);
        let section = if models_cfg.contains_key("part1_3ds") {
            "part1_3ds"
        } else {
            "model"
        };
        let offset = glam::Vec3::new(
            number(&models_cfg, section, &format!("{key}_posx"), 0.)?,
            number(&models_cfg, section, &format!("{key}_posz"), 0.)?,
            number(&models_cfg, section, &format!("{key}_posy"), 0.)?,
        );
        scene.visit_import(cancel, |mut object, repaired| {
            for p in &mut object.positions {
                *p += offset;
            }
            if repaired > 0 {
                report.warnings.push(format!(
                    "{}: repaired {repaired} non-finite UV pairs",
                    object.name
                ));
            }
            let name = object.name.trim().to_ascii_lowercase();
            ensure!(
                !name.contains("_t1_") && !name.contains("_t2_"),
                "articulated mesh requires separate body: {}",
                object.name
            );
            if cabin.marker(&object)? {
                return Ok(());
            }
            if let Some((i, n)) = crate::vehicle_model::wiper_frame(&name) {
                wipers[i] = wipers[i].max(n + 1);
            }
            if name.contains("_meshcollider")
                || name.contains("_hitcollider")
                || name.contains("_windows_wet_")
            {
                return Ok(());
            }
            let crate::tags::ObjectKind::Mesh(mut mode) = crate::tags::classify(&name) else {
                return Ok(());
            };
            if mode == crate::tags::Mode::Collision || object.faces.is_empty() {
                return Ok(());
            }
            if name.contains("_glass") && mode == crate::tags::Mode::Auto {
                mode = crate::tags::Mode::Blend;
            }
            let faces = registry.object_faces(&object, &names, default, mode)?;
            let mut local = crate::traffic::Local {
                builder: Default::default(),
                registry: &registry,
            };
            crate::convert::append_object_local(
                &object,
                &faces,
                &registry.mappings,
                &mut local,
                &mut bytes,
                cancel,
            )?;
            ensure!(
                bytes <= 512 * 1024 * 1024,
                "vehicle geometry exceeds 512 MiB output budget"
            );
            let mut builder = local.builder;
            if builder.mesh.triangles.is_empty() {
                return Ok(());
            }
            if let Some(index) = mirror_camera(&name, &cfg) {
                for material in &mut builder.mesh.materials {
                    material.texture = format!("reflexion{index}.bmp");
                }
            } else {
                crate::textures(
                    &texture_dir,
                    &folder.join("Texture"),
                    &mut builder.mesh.materials,
                    &mut report.warnings,
                    cancel,
                )?;
            }
            if name.contains("_skin001_") {
                for mat in &mut builder.mesh.materials {
                    skin_slots.replace(mat, &folder.join("Texture"))?;
                }
            }
            let name_file = format!("mesh_{}", report.meshes);
            let raw = folder.join(format!("{name_file}.o3d"));
            crate::convert::write_mesh(&raw, &builder.mesh, &builder.mesh.materials)?;
            oi::encode_file(&raw, &folder.join(format!("{name_file}.o3doi")), Kind::Mesh)?;
            std::fs::remove_file(raw)?;
            model.push_str(&format!("[mesh]\n{name_file}.o3doi\n"));
            crate::export::material_overrides(&mut model, &builder.mesh.materials, &builder.modes);
            crate::vehicle_model::append(
                &mut model,
                &object.name,
                &physics,
                info,
                &models_cfg,
                &builder.mesh.materials,
            )?;
            report.meshes += 1;
            report.triangles += builder.mesh.triangles.len();
            ensure!(report.meshes <= 8192, "too many vehicle meshes");
            Ok(())
        })?;
    }
    ensure!(report.meshes > 0, "vehicle contains no visible geometry");
    let (cabin_cfg, paths) = cabin.definitions(info, &cfg, &mut report)?;
    let (script, vars, strings) = crate::vehicle_script::generate(&physics, info, &cabin, wipers);
    skins(info, target, &skin_slots.names, &mut model, cancel)?;
    lights(info, &mut model)?;
    let sounds = crate::vehicle_sounds::export(info, target, &mut report)?;
    let mut def = physics.definition(&cfg, info)?;
    if !sounds.is_empty() {
        def.push_str("[sound]\nSound/sound.cfgoi\n[sound_ai]\nSound/sound.cfgoi\n");
    }
    for (path, text) in [
        ("Model/model.cfgoi", model),
        ("Model/cabin.cfgoi", cabin_cfg),
        ("Model/paths.cfgoi", paths),
        ("Script/main.cfgoi", script),
        ("Script/variables.cfgoi", vars),
        ("Script/strings.cfgoi", strings),
        ("Sound/sound.cfgoi", sounds),
    ] {
        vehicle_package::text(&target.join(path), &text, Kind::Config)?;
    }
    vehicle_package::text(&target.join("vehicle.busoi"), &def, Kind::Vehicle)?;
    report.doors = info.doors.len();
    report.warnings.extend(material_report.warnings);
    report.warnings.push("Rigid buses use native physics and an approximate automatic transmission. C#/Proton scripts, articulated parts, custom dashboard logic, dynamic destination images, ramps and wet-glass masks are not translated".into());
    Ok(report)
}
fn skins(
    info: &Inspection,
    target: &Path,
    slots: &[String],
    out: &mut String,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    let Some(c) = info
        .components
        .iter()
        .find(|c| c.role == "skins" && c.present)
    else {
        return Ok(());
    };
    if slots.is_empty() {
        return Ok(());
    }
    let files = crate::files(&c.file, "png")?;
    ensure!(files.len() <= 256, "too many bus skins");
    std::fs::create_dir(target.join("Skins"))?;
    for (i, slot) in slots.iter().enumerate() {
        out.push_str(&format!("[CTCTexture]\nskin_{i}\n{slot}\n"));
    }
    let mut bytes = 0;
    for (i, file) in files.iter().enumerate() {
        ensure!(!cancel(), "vehicle conversion cancelled");
        bytes += file.metadata()?.len();
        ensure!(bytes <= 256 * 1024 * 1024, "bus skins exceed 256 MiB");
        let name = format!("{i}.png");
        std::fs::copy(file, target.join("Skins").join(&name))?;
        let paint =
            crate::settings::label(&file.file_stem().unwrap_or_default().to_string_lossy())?;
        for slot in 0..slots.len() {
            out.push_str(&format!("[item]\n{paint}\nskin_{slot}\nSkins/{name}\n"));
        }
    }
    Ok(())
}
fn lights(info: &Inspection, out: &mut String) -> Result<()> {
    let Some(c) = info
        .components
        .iter()
        .find(|c| c.role == "lights1" && c.present)
    else {
        return Ok(());
    };
    let cfg = ini(&c.file)?;
    for (section, _) in &cfg {
        if section.starts_with("interior_light_") {
            let p = position(&cfg, section)?;
            out.push_str(&format!(
                "[interiorlight]\nlights_interior\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
                number(&cfg, section, "range", 3.)?.clamp(0.5, 20.),
                number(&cfg, section, "colorr", 0.8)?.clamp(0., 1.) * 255.,
                number(&cfg, section, "colorg", 0.8)?.clamp(0., 1.) * 255.,
                number(&cfg, section, "colorb", 0.8)?.clamp(0., 1.) * 255.,
                p[0],
                p[1],
                p[2]
            ));
            continue;
        }
        let var = if section.starts_with("low_beam_spotlight_") {
            "lights_abblend"
        } else if section.starts_with("high_beam_spotlight_") {
            "lights_fern"
        } else if section.starts_with("reverse_spotlight_") {
            "oi_reverse"
        } else {
            continue;
        };
        let p = position(&cfg, section)?;
        let rot = number(&cfg, section, "roty", 0.)?.to_radians();
        let color = [
            number(&cfg, section, "colorr", 1.)?,
            number(&cfg, section, "colorg", 1.)?,
            number(&cfg, section, "colorb", 1.)?,
        ]
        .map(|v| v.clamp(0., 1.) * 255.);
        out.push_str(&format!(
            "[spotlight_2]\n{}\n{}\n{}\n{}\n{}\n0\n{}\n{}\n{}\n{}\n{}\n{}\n{var}\n1\n",
            p[0],
            p[1],
            p[2],
            rot.sin(),
            rot.cos(),
            color[0],
            color[1],
            color[2],
            number(&cfg, section, "range", 30.)?.clamp(1., 100.),
            20.,
            number(&cfg, section, "spotangle", 60.)?.clamp(20., 160.)
        ));
    }
    Ok(())
}

fn mirror_camera(name: &str, cfg: &crate::Ini) -> Option<usize> {
    let sections = [
        ("_mirror1_", "leftmirror"),
        ("_mirror2_", "rightmirror"),
        ("_mirror3_", "middlemirror"),
        ("_reverse_camera_screen_", "reversecamera"),
    ];
    let mut i = 0;
    for (tag, section) in sections {
        if !cfg.contains_key(section) {
            continue;
        }
        if name.contains(tag) {
            return Some(i);
        }
        i += 1;
    }
    None
}
