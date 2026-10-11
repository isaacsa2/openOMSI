//! Export one consolidated tile at a time, then publish the map manifest last.
use crate::{
    convert::{self, EDGE},
    spool::TileMeshes,
    tags::Mode,
    text, Spawn,
};
use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn tile(
    dir: &Path,
    installed_rel: &str,
    key: (i32, i32),
    tile: &TileMeshes,
    lights: &[crate::streetlights::Lamp],
    i: usize,
) -> Result<()> {
    let (tx, ty) = key;
    let objects = dir.join("proton-import");
    let models = objects.join("model");
    std::fs::create_dir_all(&models)?;
    let stem = format!("tile_{tx}_{ty}");
    let mesh = &tile.visible;
    let materials = &mesh.materials;
    // Keep the standard per-tile file even when empty: the streaming estimator can
    // recognise collision-only tiles without mistaking their CPU colliders for GPU data.
    convert::write_mesh(&models.join(format!("{stem}.o3d")), mesh, materials)?;
    // The engine probes a terrain even on tiles with only scenery. Keep that fallback
    // below the mesh: a zero-height fallback would cover Goianira's negative-height roads.
    let floor = mesh
        .vertices
        .iter()
        .chain(&tile.collision.vertices)
        .map(|v| v.position.z)
        .fold(f32::INFINITY, f32::min)
        - 50.0;
    let mut terrain = 60u32.to_le_bytes().to_vec();
    for _ in 0..61 * 61 {
        terrain.extend_from_slice(&floor.to_le_bytes());
    }
    std::fs::write(dir.join(format!("{stem}.map.terrain")), terrain)?;
    let mut map = "[version]\n14\n".to_owned();
    if !mesh.triangles.is_empty() || !lights.is_empty() {
        let mut sco = format!("[absheight]\n[fixed]\n[surface]\n[mesh]\n{stem}.o3d\n");
        material_overrides(&mut sco, materials, &tile.modes);
        crate::streetlights::append(&mut sco, lights);
        std::fs::write(objects.join(format!("{stem}.sco")), sco)?;
        map.push_str(&object(installed_rel, &stem, i * 2 + 1));
    }
    if !tile.collision.triangles.is_empty() {
        let collision = format!("{stem}_collision");
        convert::write_mesh(
            &models.join(format!("{collision}.o3d")),
            &tile.collision,
            &tile.collision.materials,
        )?;
        // No [mesh]: this geometry must never be rendered or cast shadows. The scene
        // loader recognises this generated wrapper as both wheel support and a collider.
        std::fs::write(
            objects.join(format!("{collision}.sco")),
            format!("[absheight]\n[fixed]\n[collision_mesh]\n{collision}.o3d\n"),
        )?;
        map.push_str(&object(installed_rel, &collision, i * 2 + 2));
    }
    text(&dir.join(format!("{stem}.map")), &map)?;
    Ok(())
}

fn object(installed_rel: &str, stem: &str, id: usize) -> String {
    format!("[object]\n0\n{installed_rel}/proton-import/{stem}.sco\n{id}\n0\n0\n0\n0\n0\n0\n0\n")
}

pub(crate) fn material_overrides(sco: &mut String, materials: &[omsi_o3d::Material], modes: &[Mode]) {
    for (index, (m, mode)) in materials.iter().zip(modes).enumerate() {
        let alpha = match mode {
            Mode::Blend | Mode::EmissiveBlend | Mode::Additive => 2,
            Mode::Cutout | Mode::EmissiveCutout => 1,
            _ if m.diffuse[3] < 1.0 => 2,
            _ if m.texture.to_lowercase().ends_with(".png") => 1,
            _ if matches!(mode, Mode::Emissive) => 0,
            _ => continue,
        };
        // The occurrence index names repeated textures, not the material slot.
        let occurrence = materials[..index]
            .iter()
            .filter(|other| other.texture.eq_ignore_ascii_case(&m.texture))
            .count();
        sco.push_str(&format!(
            "[matl]\n{}\n{occurrence}\n[matl_alpha]\n{alpha}\n",
            m.texture
        ));
        if *mode == Mode::Additive {
            // Native equivalent until arbitrary mesh additive blending is available:
            // self-lit alpha blending without depth writes (no invisible depth panel).
            sco.push_str("[matl_noZwrite]\n");
        }
        if matches!(mode, Mode::Emissive | Mode::EmissiveBlend | Mode::EmissiveCutout | Mode::Additive) {
            sco.push_str(&format!("[matl_allcolor]\n0\n0\n0\n1\n0\n0\n0\n0\n0\n0\n{}\n{}\n{}\n0\n",m.emissive[0],m.emissive[1],m.emissive[2]));
        }
    }
}

pub(crate) fn global(
    dir: &Path,
    installed_rel: &str,
    keys: &[(i32, i32)],
    spawns: &[Spawn],
) -> Result<()> {
    let mut global = format!("[name]\n{}\n[friendlyname]\n{} (Proton Bus)\n[version]\n14\n[description]\nProton Bus import: free driving, authored traffic/pedestrian paths, bus stops and signal cycles. Native installed vehicles and humans required. Timetables and Proton vehicle mods are not imported.\n[end]\n[years]\n1980\n2100\n", installed_rel.rsplit('/').next().unwrap_or(installed_rel), installed_rel.trim_start_matches("maps/Proton-"));
    for &(tx, ty) in keys {
        global.push_str(&format!("[map]\n{tx}\n{ty}\ntile_{tx}_{ty}.map\n"));
    }
    global.push_str(&format!("[entrypoints]\n{}\n", spawns.len()));
    for (i, spawn) in spawns.iter().enumerate() {
        let [x, y, z] = spawn.pos;
        let key = (
            (x / EDGE as f64).floor() as i32,
            (y / EDGE as f64).floor() as i32,
        );
        let group = keys
            .iter()
            .position(|k| *k == key)
            .context("entry point is outside imported scenery")?;
        let half = spawn.heading.to_radians() * 0.5;
        // An id that has no scenery object makes the game use this explicit entry position.
        global.push_str(&format!(
            "{i}\n{}\n0\n{}\n{z}\n{}\n0\n{}\n0\n{}\n{group}\n{}\n",
            -(i as i64) - 1,
            x - key.0 as f64 * EDGE as f64,
            y - key.1 as f64 * EDGE as f64,
            half.sin(),
            half.cos(),
            spawn.name
        ));
    }
    text(&dir.join("global.cfg"), &global)?;
    Ok(())
}
