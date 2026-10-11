//! Proton maps become normal OMSI maps before the install's staging area is committed.
use super::*;

const KIND: &str = "Proton Bus map (basic free-driving import)";

pub(super) fn plan_at(
    node: &Node,
    path: &str,
    entries: &[Entry],
    plan: &mut Plan,
    used: &mut HashSet<usize>,
) -> bool {
    // A normal OMSI map can carry incidental .map.txt files; it stays a normal map.
    if node
        .files
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("global.cfg"))
    {
        return false;
    }
    let mut found = false;
    for (file, index) in &node.files {
        let lower = file.to_ascii_lowercase();
        if !lower.ends_with(".map.txt") {
            continue;
        }
        let stem = &file[..file.len() - ".map.txt".len()];
        let named = node
            .dirs
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(stem));
        let candidates: Vec<_> = node
            .dirs
            .iter()
            .filter(|(_, d)| d.dirs.keys().any(|n| n.eq_ignore_ascii_case("tiles")))
            .collect();
        let base = named.or_else(|| (candidates.len() == 1).then(|| candidates[0]));
        let inline = node
            .files
            .iter()
            .filter(|(n, _)| n.to_ascii_lowercase().ends_with(".map.txt"))
            .count()
            == 1
            && node.dirs.keys().any(|n| n.eq_ignore_ascii_case("tiles"));
        if stem.is_empty() || stem.contains(['\r', '\n']) {
            continue;
        }
        let (files, bytes) = if let Some((base_name, base)) = base {
            let (mut files, bytes) = collect(base, &join(path, base_name), entries);
            for (_, rel) in &mut files {
                *rel = join(base_name, rel);
            }
            files.push((*index, file.clone()));
            (files, bytes + entries[*index].size)
        } else if inline {
            collect(node, path, entries)
        } else {
            continue;
        };
        used.extend(files.iter().map(|f| f.0));
        plan.maps.push(Mapping {
            src: path.to_string(),
            dest: format!("maps/Proton-{stem}"),
            what: KIND.into(),
            files,
            bytes,
            aside: false,
        });
        found = true;
    }
    if found {
        plan.warnings.push("Proton Bus basic support: scenery, textures and entry points for free driving. Passengers, AI traffic, traffic lights, routes and timetables are not imported.".into());
    }
    found
}

pub(super) fn extra_space(plan: &Plan) -> u64 {
    // Reserve for spooled/clipped faces and textures copied into the scenery
    // directory. Reserve a conservative expansion, including a hard-linked folder source.
    plan.maps
        .iter()
        .filter(|m| m.what == KIND)
        .map(|m| m.bytes.saturating_mul(8))
        .sum()
}

pub(super) fn check_mode(plan: &Plan, mode: InstallMode) -> Result<()> {
    if mode == InstallMode::InPlace && plan.maps.iter().any(|m| m.what == KIND) {
        return Err(anyhow!("Proton maps must be extracted and converted; choose Extract instead of using the archive in place"));
    }
    Ok(())
}

pub(super) fn convert(staging: &Path, plan: &Plan, job: &Job) -> Result<()> {
    for (i, m) in plan.maps.iter().enumerate().filter(|(_, m)| m.what == KIND) {
        if job.cancelled() {
            return Err(anyhow!(Cancelled));
        }
        job.state(
            "converting",
            "importing Proton Bus scenery for free driving",
        );
        check_space(&staging.join(format!("{i}")))?;
        let report =
            omsi_proton::import(&staging.join(format!("{i}")), &m.dest, || job.cancelled());
        if job.cancelled() {
            return Err(anyhow!(Cancelled));
        }
        let report = report.with_context(|| format!("converting {}", m.dest))?;
        check_space(staging)?;
        job.set(|p| {
            p.report.push(format!(
                "{}: imported {} tiles and {} triangles; {} simulation markers and {} GPS helpers left out; {} duplicate vertices removed",
                m.dest, report.tiles, report.triangles, report.markers_skipped, report.gps_skipped, report.vertices_removed
            ));
            p.warnings.extend(report.warnings);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_maps_and_multiple_siblings_are_planned_separately() {
        let entries: Vec<_> = [
            "Pack/A.map.txt",
            "Pack/A/tiles/x/map.3ds",
            "Pack/A/textures/a.png",
            "Pack/B.map.txt",
            "Pack/B/tiles/x/map.3ds",
        ]
        .iter()
        .enumerate()
        .map(|(index, rel)| Entry {
            rel: rel.to_string(),
            size: 1,
            index,
        })
        .collect();
        let mut tree = Node::default();
        for (i, e) in entries.iter().enumerate() {
            tree.insert(&e.rel, i);
        }
        let mut plan = Plan::default();
        let mut used = HashSet::new();
        assert!(plan_at(
            tree.get("Pack").unwrap(),
            "Pack",
            &entries,
            &mut plan,
            &mut used
        ));
        assert_eq!(plan.maps.len(), 2);
        assert_eq!(used.len(), entries.len());
        assert_eq!(plan.maps[0].dest, "maps/Proton-A");
        assert!(plan.maps[0]
            .files
            .iter()
            .any(|(_, p)| p == "A/tiles/x/map.3ds"));
        assert!(check_mode(&plan, InstallMode::InPlace).is_err());
        assert!(check_mode(&plan, InstallMode::Extract).is_ok());
    }

    #[test]
    fn unique_base_folder_and_inline_maps_are_detected_without_guessing_ambiguous_layouts() {
        for paths in [
            vec!["Pack/Display.map.txt", "Pack/Actual/tiles/x/road.3ds"],
            vec!["Pack/Display.map.txt", "Pack/tiles/x/road.3ds"],
        ] {
            let entries: Vec<_> = paths
                .iter()
                .enumerate()
                .map(|(index, rel)| Entry {
                    rel: rel.to_string(),
                    size: 1,
                    index,
                })
                .collect();
            let mut tree = Node::default();
            for (i, e) in entries.iter().enumerate() {
                tree.insert(&e.rel, i);
            }
            let mut plan = Plan::default();
            let mut used = HashSet::new();
            assert!(plan_at(
                tree.get("Pack").unwrap(),
                "Pack",
                &entries,
                &mut plan,
                &mut used
            ));
            assert_eq!(plan.maps[0].dest, "maps/Proton-Display");
            assert_eq!(used.len(), entries.len());
            assert!(plan.maps[0]
                .files
                .iter()
                .any(|(_, rel)| rel == "Display.map.txt"));
        }
        let mut node = Node::default();
        node.insert("Display.map.txt", 0);
        node.insert("A/tiles/x/road.3ds", 1);
        node.insert("B/tiles/x/road.3ds", 2);
        assert!(!plan_at(
            &node,
            "",
            &[],
            &mut Plan::default(),
            &mut HashSet::new()
        ));
    }

    #[test]
    fn existing_omsi_map_with_incidental_proton_text_is_not_converted() {
        let mut node = Node::default();
        node.insert("global.cfg", 0);
        node.insert("Other.map.txt", 1);
        node.insert("Other/tiles/x/map.3ds", 2);
        assert!(!plan_at(
            &node,
            "",
            &[],
            &mut Plan::default(),
            &mut HashSet::new()
        ));
    }

    #[test]
    fn install_supplied_proton_zip_when_available() {
        let Some(src) = std::env::var_os("PROTON_INSTALL_FIXTURE") else {
            return;
        };
        let content = std::env::temp_dir().join(format!("proton-install-{}", std::process::id()));
        std::fs::create_dir_all(&content).unwrap();
        let report = run_blocking(
            content.clone(),
            None,
            PathBuf::from(src),
            InstallMode::Extract,
            None,
            false,
        );
        assert_eq!(
            report.state, "done",
            "{}: {:?}",
            report.message, report.warnings
        );
        assert_eq!(report.installed.len(), 1);
        let map = content.join(&report.installed[0]);
        let global = omsi_map::GlobalCfg::load(&map.join("global.cfg")).unwrap();
        assert!(!global.tiles.is_empty() && !global.entry_points.is_empty());
        for t in global.tiles {
            let tile = omsi_map::Tile::load(&map.join(t.file)).unwrap();
            for o in tile.objects {
                assert!(content.join(o.file).is_file());
            }
        }
        std::fs::remove_dir_all(content).unwrap();
    }
}
