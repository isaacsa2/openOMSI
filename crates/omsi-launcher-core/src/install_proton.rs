//! Proton maps become normal OMSI maps before the install's staging area is committed.
use super::*;

const KIND: &str = "Proton Bus map (basic free-driving import)";

/// Read only small configuration files while planning, never meshes or textures.
pub(super) fn read_layouts(source: &mut Source, entries: &mut [Entry], job: &Job) -> Result<()> {
    for entry in entries {
        if !entry.rel.to_ascii_lowercase().ends_with(".map.txt") {
            continue;
        }
        if entry.size > 64 * 1024 {
            entry.proton_authorization = Some(Err("map configuration exceeds 64 KiB; conversion blocked".into()));
            continue;
        }
        if job.cancelled() {
            return Err(anyhow!(Cancelled));
        }
        let mut data = Vec::new();
        match source {
            Source::Folder(paths) => {
                std::fs::File::open(&paths[entry.index])?
                    .take(64 * 1024 + 1)
                    .read_to_end(&mut data)?;
            }
            Source::Zip(zip) => {
                zip.by_index(entry.index)?
                    .take(64 * 1024 + 1)
                    .read_to_end(&mut data)?;
            }
        }
        entry.proton_base = omsi_proton::base_dir(&data).ok();
        entry.proton_authorization = Some(omsi_proton::permission::validate(&data).map_err(|e| e.to_string()));
    }
    Ok(())
}

/// Validate every selected source map before creating a staging/output directory.
pub(super) fn validate_selected(plan: &Plan, entries: &[Entry]) -> Result<()> {
    for map in plan.maps.iter().filter(|m| m.what == KIND) {
        let config = map.files.iter().find(|(_, p)| p.to_ascii_lowercase().ends_with(".map.txt"))
            .and_then(|(i, _)| entries.get(*i)).context("missing source map declaration")?;
        match &config.proton_authorization {
            Some(Ok(())) => {},
            Some(Err(message)) => return Err(anyhow!("{}: {message}", config.rel)),
            None => return Err(anyhow!("{}: conversion blocked: missing validated [openomsi] allowConversion=1 declaration", config.rel)),
        }
    }
    Ok(())
}

fn folder<'a>(root: &'a Node, rel: &str) -> Option<(String, &'a Node)> {
    let rel = safe_rel(rel)?;
    let mut node = root;
    let mut actual = String::new();
    for part in rel.split('/') {
        let (name, child) = node.dirs.get_key_value(part).or_else(|| {
            node.dirs
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(part))
        })?;
        actual = join(&actual, name);
        node = child;
    }
    node.dirs
        .keys()
        .any(|n| n.eq_ignore_ascii_case("tiles"))
        .then_some((actual, node))
}

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
        .any(|(name, _)| name.eq_ignore_ascii_case("global.cfg") || name.eq_ignore_ascii_case(omsi_cfg::oi::GLOBAL))
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
        let declared = entries.get(*index).and_then(|e| e.proton_base.as_deref());
        let base = if let Some(rel) = declared {
            folder(node, rel)
        } else {
            named
                .or_else(|| (candidates.len() == 1).then(|| candidates[0]))
                .map(|(name, base)| (name.clone(), base))
        };
        let inline = node
            .files
            .iter()
            .filter(|(n, _)| n.to_ascii_lowercase().ends_with(".map.txt"))
            .count()
            == 1
            && node.dirs.keys().any(|n| n.eq_ignore_ascii_case("tiles"))
            && declared.is_none_or(|rel| {
                rel.replace('\\', "/")
                    .split('/')
                    .all(|p| p.is_empty() || p == ".")
            });
        if stem.is_empty() || stem.contains(['\r', '\n']) {
            continue;
        }
        let (mut files, mut bytes) = if let Some((base_name, base)) = base {
            let (mut files, bytes) = collect(base, &join(path, &base_name), entries);
            for (_, rel) in &mut files {
                *rel = join(&base_name, rel);
            }
            files.push((*index, file.clone()));
            (files, bytes + entries[*index].size)
        } else if inline {
            collect(node, path, entries)
        } else {
            continue;
        };
        // Package-level attribution is outside baseDir in many original archives.
        for (name, index) in &node.files {
            if attribution(name) && !files.iter().any(|(i, _)| i == index) {
                files.push((*index, name.clone()));
                bytes = bytes.saturating_add(entries[*index].size);
            }
        }
        for (name, folder) in &node.dirs {
            if attribution(name) {
                let (docs, size) = collect(folder, &join(path, name), entries);
                for (index, rel) in docs {
                    if !files.iter().any(|(i, _)| *i == index) { files.push((index, join(name, &rel))); }
                }
                bytes = bytes.saturating_add(size);
            }
        }
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
        plan.warnings.push("Proton Bus preview support: scenery, GPS, bus stops, native passengers, authored vehicle/pedestrian paths and signal cycles. Native humans must be installed. Source static AI models or missing categories from installed Grundorf are used; timetables and drivable Proton buses are not imported.".into());
    }
    found
}

fn attribution(name: &str) -> bool {
    let name = name.to_lowercase();
    ["license", "licence", "licença", "licenca", "credit", "crédito", "copyright", "readme", "author", "autor"].iter().any(|tag| name.contains(tag))
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

pub(super) fn convert(staging: &Path, plan: &mut Plan, job: &Job) -> Result<()> {
    // Folder sources can change after planning. Recheck every copied declaration
    // before converting the first map, not only when each map is reached.
    for (i, m) in plan.maps.iter().enumerate().filter(|(_, m)| m.what == KIND) {
        let config = m.files.iter().find(|(_, p)| p.to_ascii_lowercase().ends_with(".map.txt"))
            .context("missing original map configuration")?;
        omsi_proton::permission::validate_file(&staging.join(i.to_string()).join(&config.1))?;
    }
    let shared = staging.join(plan.maps.len().to_string());
    let mut hofs = Vec::new();
    let mut hof_bytes = 0;
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
        let filename = format!("{}.hofoi", omsi_proton::depot::name(&m.dest));
        std::fs::create_dir_all(&shared)?;
        hof_bytes += std::fs::copy(
            staging.join(i.to_string()).join(omsi_proton::depot::FILE),
            shared.join(&filename),
        )?;
        hofs.push((usize::MAX, filename));
        check_space(staging)?;
        job.set(|p| {
            p.report.push(format!(
                "{}: imported {} tiles and {} triangles; {} GPS strips, {} navigation roads and {} streetlight markers; {} helper meshes removed and {} unmatched GPS helpers left out; {} duplicate vertices removed",
                m.dest, report.tiles, report.triangles, report.gps_imported, report.roads_imported, report.lights_imported, report.markers_skipped, report.gps_skipped, report.vertices_removed
            ));
            p.report.push(format!("{}: {} bus stops, {} traffic segments, {} pedestrian segments, {} signal programs and {} controlled paths", m.dest, report.stops_imported, report.traffic_segments_imported, report.pedestrian_segments_imported, report.signals_imported, report.signal_paths_imported));
            p.report.push(format!("{}: shared HOF with {} textual destinations (generated codes); no timetable or ordered line stops inferred", m.dest, report.destinations_imported));
            p.report.push(format!("{}: {} prefab instances and {} source AI types imported; OI package for OpenOMSI", m.dest, report.prefabs_imported, report.ai_vehicles_imported));
            p.report.push(format!("{}: {} 3D road faces recognized for the navigator; manual paths remain overview-only",m.dest,report.road_surface_triangles));
            p.warnings.extend(report.warnings);
        });
    }
    if !hofs.is_empty() {
        // Generated files take the same move/cleanup and mod-ownership path as
        // source files. No shared depot is installed before all maps convert.
        plan.maps.push(Mapping {
            src: String::new(), dest: "HOFs".into(), what: "generated map depots".into(),
            files: hofs, bytes: hof_bytes, aside: false,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "install_proton_tests.rs"]
mod tests;
