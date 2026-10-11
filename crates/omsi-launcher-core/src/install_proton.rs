//! Proton maps become normal OMSI maps before the install's staging area is committed.
use super::*;

const KIND: &str = "Proton Bus map (basic free-driving import)";

/// Read only small configuration files while planning, never meshes or textures.
pub(super) fn read_layouts(source: &mut Source, entries: &mut [Entry], job: &Job) -> Result<()> {
    for entry in entries {
        if !entry.rel.to_ascii_lowercase().ends_with(".map.txt") || entry.size > 64 * 1024 {
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
        let (files, bytes) = if let Some((base_name, base)) = base {
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
        plan.warnings.push("Proton Bus preview support: scenery, GPS, bus stops, native passengers, authored vehicle/pedestrian paths and signal cycles. Native vehicles and humans must be installed. Timetables and Proton vehicle mods are not imported.".into());
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
                "{}: imported {} tiles and {} triangles; {} GPS strips, {} navigation roads and {} streetlight markers; {} helper meshes removed and {} unmatched GPS helpers left out; {} duplicate vertices removed",
                m.dest, report.tiles, report.triangles, report.gps_imported, report.roads_imported, report.lights_imported, report.markers_skipped, report.gps_skipped, report.vertices_removed
            ));
            p.report.push(format!("{}: {} bus stops, {} traffic segments, {} pedestrian segments, {} signal programs and {} controlled paths", m.dest, report.stops_imported, report.traffic_segments_imported, report.pedestrian_segments_imported, report.signals_imported, report.signal_paths_imported));
            p.warnings.extend(report.warnings);
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "install_proton_tests.rs"]
mod tests;
