use super::*;

// ---------------------------------------------------------------------------------------
// content lists

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MapInfo {
    pub name: String,
    pub friendly: String,
    pub file: String,
    pub description: String,
    pub entry_points: Vec<EntryInfo>,
    /// The depot file (`.hof` name) the map's own buses use, from ailists.cfg.
    pub hof: String,
    /// Installed as a mod (in the content folder).
    #[serde(default)]
    pub installed: bool,
    /// The years the map's `[years]` (global.cfg) allows, first and last, when it sets any;
    /// a date outside them is moved in ([`fit_date_to_years`]).
    #[serde(default)]
    pub year_span: Option<(i32, i32)>,
    /// The transport modes the map is built for (`[bus_transport]` ... in global.cfg); empty:
    /// every vehicle may run on it.
    #[serde(default)]
    pub transport: Vec<String>,
}

/// The span of years a map's `[years]` allows: none for `0 0`; no first year when the first
/// is 0, no last when the last is 0 or before the first.
pub(super) fn year_span_of(g: &omsi_map::GlobalCfg) -> Option<(i32, i32)> {
    match g.years {
        (first, last) if first <= 0 && last <= 0 => None,
        (first, last) => {
            let from = if first > 0 { first } else { i32::MIN };
            let to = if last > 0 && last >= from {
                last
            } else {
                i32::MAX
            };
            Some((from, to))
        }
    }
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// `date` (YYYY-MM-DD) fitted to a map's year span, as OMSI 2 starts a map on a day it
/// allows so that its AI lists, chrono and timetables fit: the same day in the nearest
/// allowed year (29 February becomes the 28th where that year has none). A date inside the
/// span, or one that is no date, is kept.
pub fn fit_date_to_years(date: &str, span: Option<(i32, i32)>) -> String {
    let mut fields = date.trim().splitn(3, '-').map(|f| f.trim().parse::<i32>());
    let (Some((from, to)), Some(Ok(year)), Some(Ok(month)), Some(Ok(day))) =
        (span, fields.next(), fields.next(), fields.next())
    else {
        return date.to_string();
    };
    if (from..=to).contains(&year) {
        return date.to_string();
    }
    let year = year.clamp(from, to);
    let day = if (month, day) == (2, 29) && !is_leap_year(year) {
        28
    } else {
        day
    };
    format!("{year:04}-{month:02}-{day:02}")
}

/// The `--date` of a duty: the date the player chose as it stands. Only a date taken from
/// the machine's calendar (`use_real_date`) is fitted to the map's `[years]` - an old saved
/// duty or a manually picked year must not be moved behind the player's back.
pub(super) fn date_for_game(date: &str, real_date: bool, span: Option<(i32, i32)>) -> String {
    if real_date {
        fit_date_to_years(date, span)
    } else {
        date.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EntryInfo {
    pub index: i32,
    pub name: String,
}

pub fn list_maps() -> Result<Vec<MapInfo>> {
    root()?;
    let lang = content_language();
    let mut out = Vec::new();
    let mut keys = Vec::new();
    for (folder, dirs) in merged_folders("maps") {
        // a map is one folder: the first copy that has a global.cfg
        let Some(d) = dirs
            .into_iter()
            .find(|d| omsi_cfg::vfs::is_file(&omsi_cfg::oi::global_path(d)))
        else {
            continue;
        };
        let key = format!("map|{lang}|{}", d.display());
        let mut stamped = vec![
            d.clone(),
            omsi_cfg::oi::global_path(&d),
            d.join("ailists.cfg"),
            d.join(omsi_proton::depot::NAME_FILE),
        ];
        stamped.extend(dsc_candidates(&omsi_cfg::oi::global_path(&d), lang));
        let stamp = index::files_stamp(&stamped);
        keys.push(key.clone());
        let info: Option<MapInfo> = index::cached(&key, stamp, || read_map(&d, &folder, lang));
        out.extend(info);
    }
    index::save("map|", Some(&keys));
    if out.is_empty() {
        log_empty("maps", "global.cfg");
    }
    Ok(out)
}

/// Say in launcher.log where a list that came out empty was looked for: each folder of
/// `rel` and how many entries it has (a player whose lists stay empty sends the log).
fn log_empty(rel: &str, what: &str) {
    let places: Vec<String> = bases()
        .iter()
        .map(|b| {
            let d = b.join(rel);
            match omsi_cfg::vfs::list_dir(&d) {
                Some(l) => format!("{} ({} entries)", d.display(), l.len()),
                None => format!(
                    "{} (cannot be read: {})",
                    d.display(),
                    std::fs::read_dir(&d)
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "not a folder".into())
                ),
            }
        })
        .collect();
    log_line(&format!(
        "{rel}: nothing with a {what} found in {}",
        if places.is_empty() {
            "no folder (no OMSI 2 folder and no content folder)".to_string()
        } else {
            places.join(", ")
        }
    ));
}

/// The map in `d` and the files its entry is read from beyond the stamped ones (the tiles,
/// when their entry objects are its entry points).
pub(super) fn read_map(d: &Path, folder: &str, lang: &str) -> (Option<MapInfo>, Vec<PathBuf>) {
    let mut g = match omsi_map::GlobalCfg::load(&omsi_cfg::oi::global_path(d)) {
        Ok(g) => g,
        Err(e) => {
            log_line(&format!(
                "maps: {} cannot be read ({e:#}) - not listed",
                omsi_cfg::oi::global_path(d).display()
            ));
            return (None, Vec::new());
        }
    };
    // A map from OMSI 1 lists no entry points in global.cfg: the game takes them from the
    // entry objects of its tiles (`GlobalCfg::add_object_entry_points`). Only then are the
    // tiles read here - the list of maps stays quick.
    let mut deps = Vec::new();
    if g.entry_points.is_empty() {
        g.add_object_entry_points();
        deps = g
            .tiles
            .iter()
            .map(|t| omsi_cfg::resolve_path(g.dir(), &t.file))
            .collect();
    }
    // `global_ENG.dsc` (the language of the settings) names and describes the map
    let dsc = find_dsc(&omsi_cfg::oi::global_path(d), lang);
    let friendly = dsc
        .as_ref()
        .and_then(|x| x.name.first().cloned())
        .unwrap_or_else(|| g.friendly_name.trim().to_string());
    let description = dsc
        .map(|x| x.description)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| g.description.trim().to_string());
    let mut entries: Vec<EntryInfo> = g
        .entry_points
        .iter()
        .map(|e| EntryInfo {
            index: e.index,
            name: e.name.trim().to_string(),
        })
        .collect();
    // the game's --entry is the position in the list, not the index field; several
    // entries often share a name (one per stop position), so number them
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let total: std::collections::HashMap<String, usize> =
        entries
            .iter()
            .fold(std::collections::HashMap::new(), |mut m, e| {
                *m.entry(e.name.clone()).or_default() += 1;
                m
            });
    for (i, e) in entries.iter_mut().enumerate() {
        e.index = i as i32;
        let n = seen.entry(e.name.clone()).or_default();
        *n += 1;
        if total.get(&e.name).copied().unwrap_or(0) > 1 {
            e.name = format!("{} ({})", e.name, n);
        }
    }
    // (the depot groups' first: a plain car group's name line is no depot)
    let hof = omsi_map::ailists::AiLists::load(&d.join("ailists.cfg"))
        .ok()
        .and_then(|l| {
            l.groups
                .iter()
                .filter(|g| g.is_depot)
                .chain(l.groups.iter())
                .find_map(|g| g.hof.clone())
        })
        .or_else(|| omsi_proton::depot::installed_name(d))
        .unwrap_or_default();
    (
        Some(MapInfo {
            name: if g.name.trim().is_empty() {
                folder.to_string()
            } else {
                g.name.trim().to_string()
            },
            friendly,
            file: format!(
                "maps/{folder}/{}",
                omsi_cfg::oi::global_path(d)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            description,
            entry_points: entries,
            hof,
            installed: in_content(d),
            year_span: year_span_of(&g),
            transport: g.transport.iter().map(|m| m.name().to_string()).collect(),
        }),
        deps,
    )
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VehicleInfo {
    pub name: String,
    pub manufacturer: String,
    pub type_name: String,
    pub file: String,
    pub folder: String,
    pub description: String,
    /// The original third [friendlyname] line, shown for the bus's standard paint.
    #[serde(default)]
    pub default_paint: String,
    pub paints: Vec<String>,
    pub hofs: Vec<String>,
    /// Installed as a mod (the bus file is in the content folder).
    #[serde(default)]
    pub installed: bool,
    /// Vehicle packs this bus borrows parts from that are not installed (the Ahlheim
    /// Citaro's dashboard, steering wheel and ticket machine come from `Urbino_II`): it
    /// drives, but with holes in the cockpit, as it would in OMSI.
    #[serde(default)]
    pub missing_packs: Vec<String>,
    /// The fleet numbers of the bus's `[number]` list with the plate each comes with
    /// (Omsi.exe's number combo in the vehicle dialog; empty: the bus has no list).
    #[serde(default)]
    pub numbers: Vec<(String, String)>,
    /// The transport modes of the vehicle: what its file declares, else what its shape says.
    #[serde(default)]
    pub transport: Vec<String>,
}

/// Why `vehicle` may not run on `map`, or `None` when it may (a map that names no transport
/// mode takes every vehicle). The game checks the same rule when it starts.
pub fn transport_refusal(map: &MapInfo, vehicle: &VehicleInfo) -> Option<String> {
    let modes = |v: &[String]| -> omsi_cfg::transport::ModeSet {
        v.iter()
            .filter_map(|n| omsi_cfg::transport::TransportMode::from_name(n))
            .collect()
    };
    match omsi_cfg::transport::admit(modes(&map.transport), modes(&vehicle.transport), false) {
        omsi_cfg::transport::Admission::Allowed => None,
        omsi_cfg::transport::Admission::Refused(why) => Some(why),
    }
}

/// Names in older vehicle packs use underscores as spaces.
pub fn display_bus_name(name: &str) -> String {
    name.replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The displayed vehicle type, using its file name when [friendlyname] leaves it empty.
pub fn vehicle_type_label(type_name: &str, path: &Path) -> String {
    if type_name.trim().is_empty() {
        display_bus_name(&path.file_stem().unwrap_or_default().to_string_lossy())
    } else {
        display_bus_name(type_name)
    }
}

/// The vehicle packs whose parts a model file names and that are installed nowhere.
pub(super) fn missing_packs_of(model: &Path) -> Vec<String> {
    let Ok(text) = omsi_cfg::vfs::read(model) else {
        return Vec::new();
    };
    let text = omsi_cfg::codepage::decode(&text);
    let dir = model.parent().unwrap_or(Path::new("."));
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if !l.starts_with("..") {
            continue;
        }
        let p = omsi_cfg::resolve_path(dir, l);
        // (the game also finds a part from the model's parent folders - `<vehicle>/model`
        // and the vehicle folder for a cfg in `model/Configuration Files`: mesh_path)
        let found = |d: &Path| omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(d, l));
        if omsi_cfg::vfs::is_file(&p) || dir.ancestors().skip(1).take(2).any(found) {
            continue;
        }
        if let Some(pack) = omsi_cfg::missing_vehicle_pack(&p) {
            if !out.contains(&pack) {
                out.push(pack);
            }
        }
    }
    out
}

/// `[item]` names of every `.cti` in the model's `[CTC]` folders (in every content root):
/// the paint schemes, and the folders they were read from.
pub(super) fn paint_schemes(vehicle: &omsi_vehicle::Vehicle) -> (Vec<String>, Vec<PathBuf>) {
    let mut names: Vec<String> = Vec::new();
    let mut dirs_read: Vec<PathBuf> = Vec::new();
    let Some(model_rel) = vehicle.model.as_ref() else {
        return (names, dirs_read);
    };
    let model_path = omsi_cfg::resolve_path(vehicle.dir(), model_rel);
    let Ok(model) = omsi_model::Model::load(&model_path) else {
        return (names, dirs_read);
    };
    for c in &model.ctc {
        let dir = omsi_cfg::resolve_path(vehicle.dir(), &c.path);
        let mut files: Vec<PathBuf> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut copies = omsi_cfg::mirrored_dirs(&dir);
        if !copies.contains(&dir) {
            copies.push(dir.clone());
        }
        for d in copies {
            dirs_read.push(d.clone());
            let Some(list) = omsi_cfg::vfs::list_dir(&d) else {
                continue;
            };
            let mut here: Vec<PathBuf> = list
                .into_iter()
                .map(|(n, _)| d.join(n))
                .filter(|p| {
                    p.extension()
                        .map(|e| e.eq_ignore_ascii_case("cti"))
                        .unwrap_or(false)
                })
                .collect();
            here.sort();
            for f in here {
                if seen.insert(
                    f.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_ascii_lowercase(),
                ) {
                    files.push(f);
                }
            }
        }
        for f in files {
            let Ok(cfg) = omsi_cfg::CfgFile::read(&f) else {
                continue;
            };
            let mut r = cfg.reader();
            while let Some(k) = r.next_keyword() {
                if k == "item" {
                    let name = r.str().trim().to_string();
                    if !name.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                        names.push(name);
                    }
                }
            }
        }
    }
    dirs_read.sort();
    dirs_read.dedup();
    (names, dirs_read)
}

/// The `[name]`s of the depot files in the top-level `HOFs/` folder of every content root
/// (shared by all vehicles; a higher-priority root's file hides the same file name lower down).
fn shared_depot_names() -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for base in bases() {
        let dir = base.join("HOFs");
        let mut list = omsi_cfg::vfs::list_dir(&dir).unwrap_or_default();
        list.sort();
        for (n, is_dir) in list {
            let file = n.to_string_lossy().to_ascii_lowercase();
            if is_dir || !(file.ends_with(".hof") || file.ends_with(".hofoi")) || !seen.insert(file)
            {
                continue;
            }
            if let Some(name) = omsi_vehicle::Hof::read_name(&dir.join(&n)) {
                names.push(name.trim().to_string());
            }
        }
    }
    names
}

/// One line into ~/.openomsi/launcher.log.
pub(super) fn log_line(line: &str) {
    use std::io::Write;
    let p = data_dir().join("launcher.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{now} {line}");
    }
}

pub fn list_vehicles() -> Result<Vec<VehicleInfo>> {
    list_vehicles_progress(|_, _, _| {})
}

/// The depot files (by `[name]`) beside bus or map file `bus` (relative to `root`) whose IBIS
/// trips carry line `line`, the fullest first, read once per file and line.
pub fn depot_names_with_line(root: &Path, bus: &str, line: &str) -> Vec<String> {
    type Cache = std::collections::HashMap<(PathBuf, String, String), Vec<String>>;
    static CACHE: std::sync::LazyLock<std::sync::Mutex<Cache>> =
        std::sync::LazyLock::new(Default::default);
    let key = (root.to_path_buf(), bus.to_string(), line.to_string());
    if let Some(v) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return v.clone();
    }
    let file = omsi_cfg::resolve_path(root, bus);
    let found: Vec<String> = file
        .parent()
        .map(|dir| {
            let mut hofs: Vec<omsi_vehicle::Hof> = omsi_vehicle::hof::depot_files(dir)
                .into_iter()
                .filter_map(|p| omsi_vehicle::Hof::load(&p).ok())
                .filter(|h| h.has_line(line))
                .collect();
            hofs.sort_by_key(|h| std::cmp::Reverse(h.info_trips.len()));
            hofs.into_iter()
                .map(|h| h.name.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, found.clone());
    found
}

/// Whether the depot file called `name`, wherever it is installed (the bus's own folder,
/// another bus's, the shared `HOFs/`), carries line `line`; None when there is no such file.
/// Read once per name and line.
pub fn depot_has_line(name: &str, line: &str) -> Option<bool> {
    type Cache = std::collections::HashMap<(String, String), Option<bool>>;
    static CACHE: std::sync::LazyLock<std::sync::Mutex<Cache>> =
        std::sync::LazyLock::new(Default::default);
    let key = (name.trim().to_ascii_lowercase(), line.to_string());
    if let Some(v) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return *v;
    }
    let found = omsi_vehicle::hof::depot_anywhere(name).map(|h| h.has_line(line));
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, found);
    found
}

/// The buses, as `list_vehicles`, with `progress` told after every few folders what they
/// held, how many folders are done and how many there are: a big installation's first
/// reading (thousands of vehicle folders, nothing in the cache yet) takes minutes, and the
/// page showed nothing at all until the last folder was read.
pub fn list_vehicles_progress(
    progress: impl Fn(&[VehicleInfo], usize, usize),
) -> Result<Vec<VehicleInfo>> {
    use rayon::prelude::*;
    root()?;
    let lang = content_language();
    let folders = merged_folders("Vehicles");
    let shared_hofs = shared_depot_names();
    let keys: Vec<String> = folders
        .iter()
        .map(|(_, dirs)| {
            format!(
                "bus4|{lang}|{}",
                dirs.iter()
                    .map(|d| d.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("|")
            )
        })
        .collect();
    let read = |(folder, dirs): &(String, Vec<PathBuf>), key: &String| -> Vec<VehicleInfo> {
        // the stamp covers every copy of the folder and their direct entries (Model/,
        // Texture/ ...); the paint folders the entry read are its dependencies
        let mut stamped: Vec<PathBuf> = dirs.clone();
        for d in dirs {
            if let Some(list) = omsi_cfg::vfs::list_dir(d) {
                let mut subs: Vec<PathBuf> = list
                    .into_iter()
                    .filter(|(_, is_dir)| *is_dir)
                    .map(|(n, _)| d.join(n))
                    .collect();
                subs.sort();
                stamped.extend(subs);
            }
        }
        index::cached(key, index::folder_stamp(&stamped), || {
            read_vehicle_folder(folder, dirs, lang)
        })
    };
    // folders side by side (the files of one wait for the disk while another's are parsed),
    // a handful of threads so that a hard disk is not sent seeking all over
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .clamp(2, 8),
        )
        .build()
        .ok();
    let mut out = Vec::new();
    let total = folders.len();
    let mut done = 0;
    let mut saved = std::time::Instant::now();
    for (chunk, chunk_keys) in folders.chunks(32).zip(keys.chunks(32)) {
        let lists: Vec<Vec<VehicleInfo>> = match &pool {
            Some(pool) => pool.install(|| {
                chunk
                    .par_iter()
                    .zip(chunk_keys.par_iter())
                    .map(|(f, k)| read(f, k))
                    .collect()
            }),
            None => chunk
                .iter()
                .zip(chunk_keys.iter())
                .map(|(f, k)| read(f, k))
                .collect(),
        };
        let mut batch: Vec<VehicleInfo> = lists.into_iter().flatten().collect();
        // the shared depot files after each bus's own (as the game offers them,
        // `omsi_vehicle::hof::depot_files`; a bus's own of the same name wins) - after the
        // cache, whose stamps cover only the vehicle folders
        for v in batch.iter_mut() {
            for name in &shared_hofs {
                if !v.hofs.iter().any(|h| h.eq_ignore_ascii_case(name)) {
                    v.hofs.push(name.clone());
                }
            }
        }
        done += chunk.len();
        // what was read is kept every few seconds: a first reading left half-way (the
        // launcher closed) starts from there the next time
        if saved.elapsed().as_secs() >= 10 {
            index::save("bus4|", None);
            saved = std::time::Instant::now();
        }
        progress(&batch, done, total);
        out.extend(batch);
    }
    index::save("bus4|", Some(&keys));
    if out.is_empty() {
        log_empty("Vehicles", ".bus file");
    }
    Ok(out)
}

/// The buses of one vehicle folder (all its copies; a bus file in the content folder hides
/// the one of the same name in the OMSI folder), and the folders read besides its own.
fn read_vehicle_folder(
    folder: &str,
    dirs: &[PathBuf],
    lang: &str,
) -> (Vec<VehicleInfo>, Vec<PathBuf>) {
    // Every file of the folder and of the folders in it, however deep: OMSI's bus list looks
    // for `*.bus` and `*.ovh` under Vehicles to any depth (Omsi.exe 0x67bdf9, the search
    // with depth 255) - `Vehicles\Pack\Variant\x.bus` was not listed here (#137). A file of
    // the content folder hides the one at the same place in the installation.
    fn walk(d: &Path, rel: &str, depth: u32, out: &mut Vec<(String, PathBuf)>) {
        let mut list = omsi_cfg::vfs::list_dir(d).unwrap_or_default();
        list.sort();
        for (n, is_dir) in list {
            let name = n.to_string_lossy().to_string();
            let r = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if is_dir {
                if depth > 0 && !name.starts_with('.') {
                    walk(&d.join(&n), &r, depth - 1, out);
                }
            } else {
                out.push((r, d.join(&n)));
            }
        }
    }
    let mut files: Vec<PathBuf> = Vec::new();
    let mut rel_of: std::collections::HashMap<PathBuf, String> = std::collections::HashMap::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for d in dirs {
        let mut here = Vec::new();
        walk(d, "", 8, &mut here);
        for (r, f) in here {
            if seen.insert(r.to_ascii_lowercase()) {
                rel_of.insert(f.clone(), r);
                files.push(f);
            }
        }
    }
    let mut out = Vec::new();
    let mut deps = Vec::new();
    if !files.iter().any(|f| {
        f.extension()
            .map(|e| {
                e.eq_ignore_ascii_case("bus")
                    || e.eq_ignore_ascii_case("busoi")
                    || e.eq_ignore_ascii_case("ovh")
                    || e.eq_ignore_ascii_case("ovhoi")
            })
            .unwrap_or(false)
    }) {
        // textures, or a repaint for a bus that is not installed: nothing to drive
        log_line(&format!(
            "vehicles: Vehicles/{folder} has no .bus file (a repaint or textures for a bus that is not installed?) - not listed"
        ));
        return (out, deps);
    }
    // the depot files beside each bus (a bus in a folder of the pack: those of its folder,
    // else those of the pack's own)
    let hof_dir = |f: &PathBuf| {
        rel_of
            .get(f)
            .map(|r| {
                r.rsplit_once('/')
                    .map(|(d, _)| d.to_ascii_lowercase())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    };
    let mut hofs_in: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for f in files.iter().filter(|f| {
        f.extension()
            .map(|e| e.eq_ignore_ascii_case("hof"))
            .unwrap_or(false)
    }) {
        // (the name alone: UK depot files carry megabytes of trips)
        if let Some(name) = omsi_vehicle::Hof::read_name(f) {
            hofs_in
                .entry(hof_dir(f))
                .or_default()
                .push(name.trim().to_string());
        }
    }
    // OMSI offers what has a [friendlyname]: never the rear section of an articulated bus
    // (its front brings it along), an AI-only variant or a car
    for (f, v) in omsi_vehicle::vehicle::offered_vehicles(&files) {
        let f = &f;
        let stem = f
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        // a vehicle file whose model is not there would load as nothing
        let model = v.model.as_ref().map(|m| omsi_cfg::resolve_path(v.dir(), m));
        if !model
            .as_ref()
            .map(|m| omsi_cfg::vfs::is_file(m))
            .unwrap_or(false)
        {
            log_line(&format!(
                "vehicles: {} - its model {} is missing, not listed",
                f.display(),
                model
                    .map(|m| m.display().to_string())
                    .unwrap_or_else(|| "(none)".into())
            ));
            continue;
        }
        let rel = format!(
            "Vehicles/{}/{}",
            folder,
            rel_of.get(f).cloned().unwrap_or_else(|| f
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string())
        );
        let hofs = hofs_in
            .get(&hof_dir(f))
            .or_else(|| hofs_in.get(""))
            .cloned()
            .unwrap_or_default();
        let name = format!("{} {}", v.manufacturer.trim(), v.type_name.trim())
            .trim()
            .to_string();
        let (paints, paint_dirs) = paint_schemes(&v);
        deps.extend(paint_dirs);
        // `<bus>_ENG.dsc` beside the bus file (the language of the settings) describes it
        let description = find_dsc(f, lang)
            .map(|x| x.description)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| v.description.trim().to_string());
        let missing_packs = model.as_deref().map(missing_packs_of).unwrap_or_default();
        if !missing_packs.is_empty() {
            log_line(&format!(
                "vehicles: {} borrows parts from packs that are not installed: {}",
                f.display(),
                missing_packs.join(", ")
            ));
        }
        out.push(VehicleInfo {
            name: if name.is_empty() { stem.clone() } else { name },
            manufacturer: v.manufacturer.trim().to_string(),
            type_name: v.type_name.trim().to_string(),
            file: rel,
            folder: folder.to_string(),
            description,
            default_paint: v.default_paint.trim().to_string(),
            paints,
            hofs,
            installed: in_content(f),
            missing_packs,
            numbers: v.numbers_with_plates().to_vec(),
            transport: v
                .transport_modes()
                .0
                .iter()
                .map(|m| m.name().to_string())
                .collect(),
        });
    }
    deps.sort();
    deps.dedup();
    // the folders themselves are stamped by the caller
    deps.retain(|d| !dirs.contains(d));
    (out, deps)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WeatherInfo {
    pub name: String,
    pub file: String,
    pub description: String,
    pub fog_m: f32,
    pub temp: f32,
    pub clouds: String,
    pub precip: String,
    pub snow: bool,
    #[serde(default)]
    pub installed: bool,
}

pub fn list_weather() -> Result<Vec<WeatherInfo>> {
    root()?;
    let mut out = Vec::new();
    let mut keys = Vec::new();
    let lang = content_language();
    let mut files: Vec<PathBuf> = merged_entries("Weather")
        .into_iter()
        .filter(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("owt"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    for f in files {
        // the description in the settings' language lives in `<name>_ENG.dsc`
        let mut stamped = vec![f.clone()];
        stamped.extend(dsc_candidates(&f, lang));
        let key = format!("wx2|{lang}|{}", f.display());
        keys.push(key.clone());
        let info: Option<WeatherInfo> = index::cached(&key, index::files_stamp(&stamped), || {
            (read_weather(&f, lang), Vec::new())
        });
        out.extend(info);
    }
    index::save("wx2|", Some(&keys));
    Ok(out)
}

/// A weather's name as the player reads it: the `[name]` of its description in the
/// interface's language - the English one where OMSI has none in that language (the
/// interface's tables translate it further) - else its file's name. OMSI's own angle brackets
/// ("<Best Weather>", which sort it first in its list) are left out.
pub fn weather_name(f: &Path) -> String {
    weather_name_in(f, content_language())
}

/// OMSI 2's own weathers by their English names (its `_ENG.dsc`), which the interface's
/// tables translate for the languages OMSI has no descriptions in; listed here so the
/// translation check knows them as texts the program shows.
pub const STOCK_WEATHER_NAMES: [&str; 9] = [
    "Best Weather",
    "Ground Fog",
    "Twilight Fog",
    "Iciness",
    "Rainy",
    "Summerlike",
    "Heavy Snowfall",
    "Storm",
    "Freezing Wetness",
];

fn weather_name_in(f: &Path, lang: &str) -> String {
    let stem = f
        .file_stem()
        .map(|s| s.to_string_lossy().trim_start_matches('#').to_string())
        .unwrap_or_default();
    find_dsc(f, lang)
        .and_then(|d| {
            d.name.first().map(|n| {
                n.trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .trim()
                    .to_string()
            })
        })
        .filter(|n| !n.is_empty())
        .unwrap_or(stem)
}

fn read_weather(f: &Path, lang: &str) -> Option<WeatherInfo> {
    let w = omsi_content::weather::Weather::load(f).ok()?;
    // (the `[name]` of the file is not part of the description: "Ground Fog Heavy ground fog ...")
    let description = find_dsc(f, lang)
        .map(|x| x.description.replace('\n', " "))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| w.description.replace('\n', " "));
    let precip = match w.precip.first().copied().unwrap_or(0.0) as i32 {
        1 => format!(
            "rain {:.0}%",
            w.precip.get(1).copied().unwrap_or(0.0) / 255.0 * 100.0
        ),
        2 => format!(
            "snow {:.0}%",
            w.precip.get(1).copied().unwrap_or(0.0) / 255.0 * 100.0
        ),
        _ => "dry".into(),
    };
    Some(WeatherInfo {
        name: weather_name_in(f, lang),
        file: format!("Weather/{}", f.file_name().unwrap().to_string_lossy()),
        description,
        fog_m: w.fog.0,
        temp: w.temp.0,
        clouds: if w.clouds.0.trim().starts_with("-1") {
            "clear".into()
        } else {
            w.clouds.0.trim().to_string()
        },
        precip,
        snow: w.snow,
        installed: in_content(f),
    })
}

/// The content language of the settings (`language=`, English by default): which
/// `<file>_<LANG>.dsc` names and describes maps, buses and weathers.
pub(super) fn content_language() -> &'static str {
    let text = std::fs::read_to_string(data_dir().join("settings.cfg")).ok();
    language_code(
        settings_from_text(text.as_deref())["language"]
            .as_str()
            .unwrap_or("ENG"),
    )
}

/// The description files OMSI reads for `file` (`global.cfg` -> `global_ENG.dsc`), in the
/// order they are tried: the language itself, then English for a language that has none
/// (German is the files' own language, so German falls back to the file itself).
pub(super) fn dsc_candidates(file: &Path, lang: &str) -> Vec<PathBuf> {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut langs = vec![lang];
    if lang != "DEU" && lang != "ENG" {
        langs.push("ENG");
    }
    langs
        .into_iter()
        .map(|l| file.with_file_name(format!("{stem}_{l}.dsc")))
        .collect()
}

/// A `.dsc` file: the `[name]` / `[friendlyname]` lines and the `[description]` text.
pub(super) struct Dsc {
    pub(super) name: Vec<String>,
    pub(super) description: String,
}

pub(super) fn parse_dsc(text: &str) -> Dsc {
    let mut name = Vec::new();
    let mut desc: Vec<&str> = Vec::new();
    let mut section = "";
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section =
                if t.eq_ignore_ascii_case("[name]") || t.eq_ignore_ascii_case("[friendlyname]") {
                    "name"
                } else if t.eq_ignore_ascii_case("[description]") {
                    "description"
                } else {
                    ""
                };
            continue;
        }
        match section {
            "name" if !t.is_empty() => name.push(t.to_string()),
            "name" => section = "",
            "description" => desc.push(line.trim_end()),
            _ => {}
        }
    }
    Dsc {
        name,
        description: desc.join("\n").trim().to_string(),
    }
}

fn find_dsc(file: &Path, lang: &str) -> Option<Dsc> {
    dsc_candidates(file, lang)
        .iter()
        .find_map(|p| omsi_cfg::vfs::read(p).ok())
        .map(|b| parse_dsc(&encoding_latin1(&b)))
}

/// OMSI's text files are Latin-1, except that some description files were saved as
/// UTF-16 with a byte-order mark.
fn encoding_latin1(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let u: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    if b.len() >= 2 && b[0] == 0xFE && b[1] == 0xFF {
        let u: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    b.iter().map(|&c| c as char).collect()
}
