use super::*;

pub(super) fn plan(entries: &[Entry], source_name: &str, installed: &Installed) -> Plan {
    let mut tree = Node::default();
    for (i, e) in entries.iter().enumerate() {
        tree.insert(&e.rel, i);
    }
    let mut plan = Plan::default();
    let mut used: HashSet<usize> = HashSet::new();
    // level by level: a folder that *is* a thing (a bus, a map ...) is taken whole; the
    // OMSI folders among a folder's children are merged; stop below the first level that
    // gave anything
    let mut level: Vec<String> = vec![String::new()];
    for _ in 0..5 {
        let mut next = Vec::new();
        let mut found = false;
        for path in &level {
            let Some(node) = tree.get(path) else { continue };
            if proton::plan_at(node, path, entries, &mut plan, &mut used) {
                found = true;
                continue;
            }
            // a folder holding bus files is a bus even with `Sound` / `Texture` in it; one
            // holding `Vehicles` or `Sceneryobjects` is a pack whatever lies beside them
            let pack = node
                .dirs
                .keys()
                .any(|d| PACK_ONLY.iter().any(|f| f.eq_ignore_ascii_case(d)));
            if let Some(kind) = guess_kind(node).filter(|k| *k == "maps" || !pack) {
                let name = if path.is_empty() {
                    source_name.to_string()
                } else {
                    path.rsplit('/').next().unwrap_or(path).to_string()
                };
                let (files, bytes) = collect(node, path, entries);
                used.extend(files.iter().map(|f| f.0));
                let dest = format!("{kind}/{name}");
                plan.maps.push(Mapping {
                    src: path.clone(),
                    dest,
                    what: kind_label(kind).into(),
                    files,
                    bytes,
                    aside: false,
                });
                found = true;
                continue;
            }
            for (child, sub) in &node.dirs {
                let cpath = join(path, child);
                match content_folder_of(child) {
                    Some("Vehicles") => {
                        found = true;
                        // each bus folder separately: a folder without a vehicle file only
                        // adds to a bus (paints, textures), which must be installed
                        for (bus, bn) in &sub.dirs {
                            let bpath = join(&cpath, bus);
                            let (files, bytes) = collect(bn, &bpath, entries);
                            used.extend(files.iter().map(|f| f.0));
                            if bn.has_ext_deep(&["bus", "busoi", "ovh", "ovhoi", "zug", "sco"]) {
                                plan.maps.push(Mapping {
                                    src: bpath,
                                    dest: format!("Vehicles/{bus}"),
                                    what: "vehicle".into(),
                                    files,
                                    bytes,
                                    aside: false,
                                });
                            } else {
                                match installed.vehicle(bus) {
                                    Some(have) => plan.maps.push(Mapping {
                                        src: bpath,
                                        dest: format!("Vehicles/{have}"),
                                        what: format!("add-on for {have} (paints / textures)"),
                                        files,
                                        bytes,
                                        aside: false,
                                    }),
                                    None => {
                                        plan.warnings.push(trf("{bus}: only paints or textures for a bus that is not installed (Vehicles/{bus}) - kept aside in Mods/{folder} and installed by itself once that bus is installed", &[("bus", &bus), ("folder", &WAITING)]));
                                        plan.maps.push(Mapping {
                                            src: bpath,
                                            dest: format!("Vehicles/{bus}"),
                                            what: format!(
                                                "add-on for {bus}, which is not installed"
                                            ),
                                            files,
                                            bytes,
                                            aside: true,
                                        });
                                    }
                                }
                            }
                        }
                        let loose: Vec<(usize, String)> =
                            sub.files.iter().map(|(f, i)| (*i, f.clone())).collect();
                        if !loose.is_empty() {
                            plan.notes.push(format!(
                                "{} loose file(s) directly in {cpath} left out",
                                loose.len()
                            ));
                        }
                    }
                    Some(canon) => {
                        found = true;
                        let (files, bytes) = collect(sub, &cpath, entries);
                        used.extend(files.iter().map(|f| f.0));
                        plan.maps.push(Mapping {
                            src: cpath,
                            dest: canon.to_string(),
                            what: format!("{canon} folder"),
                            files,
                            bytes,
                            aside: false,
                        });
                    }
                    None => next.push(cpath),
                }
            }
        }
        if found || next.is_empty() {
            break;
        }
        level = next;
    }
    // plugins are native code that the game loads by itself: one from a download is put
    // aside, and only the player moving it into `Plugins` makes it run
    for m in plan.maps.iter_mut() {
        let lower = m.dest.to_ascii_lowercase();
        if lower == "plugins" || lower.starts_with("plugins/") {
            m.dest = format!("Mods/{PLUGINS_HELD}{}", &m.dest["plugins".len()..]);
            plan.warnings.push(trf(
                "{name}: a plugin runs its own program code inside the game, so it was not enabled - it is in Mods/{folder}; move it into Plugins yourself if you trust where it came from",
                &[("name", &if m.src.is_empty() { source_name } else { &m.src }), ("folder", &PLUGINS_HELD)],
            ));
        }
    }
    let unused = entries.len() - used.len();
    if unused > 0 && !plan.maps.is_empty() {
        let mut sample: Vec<&str> = entries
            .iter()
            .enumerate()
            .filter(|(i, _)| !used.contains(i))
            .map(|(_, e)| e.rel.as_str())
            .take(4)
            .collect();
        sample.sort();
        plan.notes.push(format!(
            "{unused} file(s) that belong to no OMSI folder were left out (e.g. {})",
            sample.join(", ")
        ));
    }
    plan
}
