//! Imported traffic models and missing categories from the installed Grundorf fleet.
use super::*;

pub(super) fn fallback(root: &Path, world: &crate::scene::World) -> RandomTypes {
    let mut types = Vec::new();
    if let Some(map) = world.global.path.parent().filter(|dir| {
        omsi_cfg::oi::kind(&world.global.path).is_some()
            && omsi_cfg::vfs::is_file(&dir.join("proton-import/traffic.cfgoi"))
            && !omsi_cfg::vfs::is_file(&dir.join("ailists.cfg"))
    }) {
        match omsi_proton::traffic::Manifest::load(map) {
            Ok(manifest) => {
                for vehicle in &manifest.vehicles {
                    let file = omsi_cfg::resolve_path(root, &vehicle.file);
                    match VehicleType::load_ai(root, &file) {
                        Ok(mut ty) => {
                            ty.def.ai_veh_type = if vehicle.buses { 2 } else { 0 };
                            ty.def.imported_ai_speed_kmh = Some(vehicle.speed_kmh);
                            types.push((Arc::new(ty), vehicle.weight, LaneKind::Street, 0));
                        }
                        Err(error) => log::warn!("imported AI {}: {error}", file.display()),
                    }
                }
                grundorf(
                    root,
                    manifest.native_cars,
                    manifest.native_buses,
                    &mut types,
                );
            }
            Err(error) => log::warn!("imported AI manifest: {error}"),
        }
    }
    let populated = !types.is_empty();
    RandomTypes {
        types,
        groups: if populated {
            vec![omsi_map::ailists::UnschedGroup {
                name: "Imported traffic".into(),
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

type Types = Vec<(Arc<VehicleType>, f32, LaneKind, usize)>;

fn grundorf(root: &Path, cars: bool, buses: bool, types: &mut Types) {
    if !cars && !buses {
        return;
    }
    let mut roots = omsi_cfg::content_dirs("maps");
    if roots.is_empty() {
        roots.push(root.join("maps"));
    }
    let list = roots.into_iter().find_map(|base| {
        omsi_map::ailists::AiLists::load(&omsi_cfg::resolve_path(&base, "Grundorf/ailists.cfg"))
            .ok()
    });
    let Some(list) = list else {
        log::warn!(
            "imported traffic fallback needs Grundorf/ailists.cfg and its installed vehicles"
        );
        return;
    };
    let mut seen = std::collections::HashSet::new();
    let mut loaded = 0;
    for (name, weight) in references(&list).into_iter().take(256) {
        if !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let file = omsi_cfg::resolve_path(root, &name);
        let Ok(def) = omsi_vehicle::Vehicle::load(&file) else {
            continue;
        };
        let is_bus = matches!(def.kind, omsi_vehicle::vehicle::VehicleKind::Bus);
        let is_car = matches!(def.kind, omsi_vehicle::vehicle::VehicleKind::Other(0))
            && matches!(def.ai_veh_type, 0 | 1);
        if !(is_bus && buses || is_car && cars) {
            continue;
        }
        match VehicleType::load_ai(root, &file) {
            Ok(mut ty) => {
                if is_bus {
                    ty.def.ai_veh_type = 2;
                }
                types.push((Arc::new(ty), weight, LaneKind::Street, 0));
                loaded += 1;
            }
            Err(error) => log::warn!("Grundorf AI {}: {error}", file.display()),
        }
        if loaded >= 8 {
            break;
        }
    }
    log::info!("imported traffic: {loaded} types referenced from installed Grundorf; no stock assets copied");
}

fn references(list: &omsi_map::ailists::AiLists) -> Vec<(String, f32)> {
    list.groups
        .iter()
        .flat_map(|group| {
            group
                .vehicles
                .iter()
                .map(|v| (v.file.clone(), v.weight))
                .chain(group.typgroups.iter().map(|v| (v.file.clone(), 1.0)))
        })
        .filter(|(file, weight)| {
            let normal = file.replace('\\', "/");
            normal.to_ascii_lowercase().starts_with("vehicles/")
                && !normal.split('/').any(|p| p == "..")
                && !normal.contains([':', '\n', '\r'])
                && weight.is_finite()
                && *weight > 0.0
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn grundorf_references_include_depot_buses_and_reject_external_paths() {
        let cfg = omsi_cfg::CfgFile::from_str("ailists.cfg", "[aigroup_2]\nCars\n\nVehicles/Car/car.ovh\t2\n../external.ovh\t1\n[end]\n[aigroup_depot]\nBuses\nGrundorf\n[aigroup_depot_typgroup_2]\nVehicles/Bus/bus.bus\n1\tA\tPaint\n[end]\n");
        let list = omsi_map::ailists::AiLists::parse(&cfg);
        assert_eq!(
            super::references(&list),
            vec![
                ("Vehicles/Car/car.ovh".into(), 2.0),
                ("Vehicles/Bus/bus.bus".into(), 1.0)
            ]
        );
    }
}
