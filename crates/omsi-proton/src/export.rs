//! Export one consolidated tile at a time, then publish the map manifest last.
use crate::{
    convert::{self, EDGE},
    text, Spawn,
};
use anyhow::{Context, Result};
use omsi_o3d::Mesh;
use std::path::Path;

pub(crate) fn tile(
    dir: &Path,
    installed_rel: &str,
    key: (i32, i32),
    mesh: &Mesh,
    i: usize,
) -> Result<()> {
    let (tx, ty) = key;
    let objects = dir.join("proton-import");
    let models = objects.join("model");
    std::fs::create_dir_all(&models)?;
    let stem = format!("tile_{tx}_{ty}");
    let materials = &mesh.materials;
    convert::write_mesh(&models.join(format!("{stem}.o3d")), mesh, materials)?;
    // The engine probes a terrain even on tiles with only scenery. Keep that fallback
    // below the mesh: a zero-height fallback would cover Goianira's negative-height roads.
    let floor = mesh
        .vertices
        .iter()
        .map(|v| v.position.z)
        .fold(f32::INFINITY, f32::min)
        - 50.0;
    let mut terrain = 60u32.to_le_bytes().to_vec();
    for _ in 0..61 * 61 {
        terrain.extend_from_slice(&floor.to_le_bytes());
    }
    std::fs::write(dir.join(format!("{stem}.map.terrain")), terrain)?;
    let mut sco = format!("[absheight]\n[fixed]\n[surface]\n[mesh]\n{stem}.o3d\n");
    // Preparation has already removed unreferenced material slots.
    for (index, m) in materials.iter().enumerate() {
        // Basic PNG cutouts keep foliage and fences; explicit material opacity is blended.
        if m.diffuse[3] < 1.0 || m.texture.to_lowercase().ends_with(".png") {
            // The occurrence index names repeated textures, not the material slot.
            let occurrence = materials[..index]
                .iter()
                .filter(|other| other.texture.eq_ignore_ascii_case(&m.texture))
                .count();
            sco.push_str(&format!(
                "[matl]\n{}\n{occurrence}\n[matl_alpha]\n{}\n",
                m.texture,
                if m.diffuse[3] < 1.0 { 2 } else { 1 }
            ));
        }
    }
    std::fs::write(objects.join(format!("{stem}.sco")), sco)?;
    text(&dir.join(format!("{stem}.map")), &format!("[version]\n14\n[object]\n0\n{installed_rel}/proton-import/{stem}.sco\n{}\n0\n0\n0\n0\n0\n0\n0\n", i + 1))?;
    Ok(())
}

pub(crate) fn global(
    dir: &Path,
    installed_rel: &str,
    keys: &[(i32, i32)],
    spawns: &[Spawn],
) -> Result<()> {
    let mut global = format!("[name]\n{}\n[friendlyname]\n{} (Proton Bus)\n[version]\n14\n[description]\nBasic Proton Bus import: free driving only. AI, passengers, timetables and traffic lights are not imported.\n[end]\n[years]\n1980\n2100\n", installed_rel.rsplit('/').next().unwrap_or(installed_rel), installed_rel.trim_start_matches("maps/Proton-"));
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
