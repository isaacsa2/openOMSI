//! Installed native cars for generated maps that do not bring an OMSI AI list.
use super::*;

pub(super) fn fallback(root: &Path, world: &crate::scene::World) -> RandomTypes {
    let mut types = Vec::new();
    // An explicit ailists.cfg remains the user's choice, including an intentionally
    // empty list. A generated navigator alone in an ordinary map does not opt it in.
    let enabled = world.global.path.parent().is_some_and(|dir| {
        dir.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("Proton-"))
            && dir.join("proton-import/traffic.enabled").is_file()
            && !omsi_cfg::vfs::is_file(&dir.join("ailists.cfg"))
    });
    if enabled {
        let mut roots = omsi_cfg::content_dirs("Vehicles");
        if roots.is_empty() {
            roots.push(root.join("Vehicles"));
        }
        let mut files = std::collections::BTreeMap::new();
        for base in roots {
            for (folder, is_dir) in omsi_cfg::vfs::list_dir(&base)
                .unwrap_or_default()
                .into_iter()
                .take(4096)
            {
                if !is_dir {
                    continue;
                }
                for (name, is_dir) in omsi_cfg::vfs::list_dir(&base.join(&folder))
                    .unwrap_or_default()
                    .into_iter()
                    .take(4096)
                {
                    if is_dir
                        || !name
                            .to_string_lossy()
                            .to_ascii_lowercase()
                            .ends_with(".ovh")
                    {
                        continue;
                    }
                    let identity =
                        format!("{}/{}", folder.to_string_lossy(), name.to_string_lossy())
                            .to_ascii_lowercase();
                    files
                        .entry(identity)
                        .or_insert_with(|| base.join(&folder).join(&name));
                    if files.len() >= 256 {
                        break;
                    }
                }
                if files.len() >= 256 {
                    break;
                }
            }
            if files.len() >= 256 {
                break;
            }
        }
        for (_, file) in files.into_iter().take(32) {
            // Inspect the definition before loading geometry: boats, aircraft and trains
            // can also use .ovh. Never put them onto imported street paths.
            let Ok(def) = omsi_vehicle::Vehicle::load(&file) else {
                continue;
            };
            if !matches!(def.kind, omsi_vehicle::vehicle::VehicleKind::Other(0))
                || !matches!(def.ai_veh_type, 0 | 1)
            {
                continue;
            }
            match VehicleType::load_ai(root, &file) {
                Ok(ty) => types.push((Arc::new(ty), 1.0, LaneKind::Street, 0)),
                Err(error) => log::warn!("installed preview AI {}: {error}", file.display()),
            }
            if types.len() >= 8 {
                break;
            }
        }
        if types.is_empty() {
            log::warn!("imported traffic needs installed native OMSI AI cars (.ovh), or a custom ailists.cfg");
        } else {
            log::info!(
                "imported traffic: {} installed native car types",
                types.len()
            );
        }
    }
    let populated = !types.is_empty();
    RandomTypes {
        types,
        groups: if populated {
            vec![omsi_map::ailists::UnschedGroup {
                name: "Local preview traffic".into(),
                factor: 1.0,
                densities: Vec::new(),
            }]
        } else {
            Vec::new()
        },
        group_curves: false,
        group_uvg: if populated { vec![None] } else { Vec::new() },
        uvg_defaults: Vec::new(),
    }
}
