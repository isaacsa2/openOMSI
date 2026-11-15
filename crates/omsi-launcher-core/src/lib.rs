//! The launcher's data side: what the window asks for (maps, buses, lines, tours, the
//! roadbook, weather, profiles, settings) and how a duty is turned into a command line for
//! the game. Everything here is plain functions over the OMSI content crates; the window
//! (the game binary's `launcher` module, drawn with wgpu) calls them directly, and
//! `cli()` exposes the same functions to a terminal (`omsi-launcher --cli ...`).
//!
//! `install` runs mod installs as background jobs, `index` caches the content lists and
//! tells the page when they changed, `instances` keeps track of the games started.

pub mod ai_lists;
pub mod crash;
pub mod depot;
mod file_dialog;
mod graphics_profile_transfer;
#[cfg(test)]
mod graphics_profile_transfer_tests;
mod graphics_profiles;
mod hof_sync;
pub mod i18n;
pub mod index;
pub mod install;
pub mod instances;
pub mod map_uses;
pub mod mods;
pub mod optimise;
pub mod storage;
pub use graphics_profile_transfer::{export_graphics_profile, import_graphics_profile};
pub use graphics_profiles::*;
mod situations;
pub use situations::{
    SavedSituation, delete_situation, last_situation, rename_situation, saved_situations,
};

pub use file_dialog::{pick_file, pick_folder, pick_graphics_profile, pick_hofs, pick_mod};
pub use hof_sync::{MapHofOffer, add_shared_hofs, share_map_hofs};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------
// configuration: where the game and the OMSI content are

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// The OMSI 2 folder (maps, Vehicles, ...).
    pub root: String,
    /// The `omsi` game binary.
    pub game: String,
    /// The current profile name.
    pub profile: String,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn data_dir() -> PathBuf {
    // (made once: asked for dozens of times a second - the launcher's frames, its polls - and
    // made each time, two system calls for a folder that was there)
    static MADE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
    let d = home().join(".openomsi");
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if made.as_ref() != Some(&d) && std::fs::create_dir_all(&d).is_ok() {
        *made = Some(d.clone());
    }
    d
}

fn config_path() -> PathBuf {
    data_dir().join("launcher.json")
}

/// A complete installation of the original OMSI 2 (the same test the game makes: it refuses
/// to start on anything else).
fn is_omsi_root(p: &Path) -> bool {
    omsi_cfg::missing_original_essentials(p).is_empty()
}

/// Where the game binary is: the configured path, next to the launcher, or the
/// development build in the source tree.
fn find_game(configured: &str) -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    // the game that came with this launcher first: a path remembered from an older
    // installation (`target/release/omsi` of the days before the rename) kept starting an
    // old build after every update - the new pause menu "was not there" on macOS
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join(if cfg!(windows) {
                "openomsi.exe"
            } else {
                "openomsi"
            }));
        }
    }
    if !configured.trim().is_empty() {
        let c = PathBuf::from(configured.trim());
        // (only a game of today's name: the old `omsi` binary is not taken any more)
        let stem = c
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if stem == "openomsi" {
            cands.push(c);
        }
    }
    if let Some(p) = std::env::var_os("OPENOMSI_BIN") {
        cands.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        for a in exe.ancestors().skip(1).take(7) {
            cands.push(a.join("openomsi"));
            cands.push(a.join("openomsi.exe"));
            cands.push(a.join("target").join("release").join("openomsi"));
            cands.push(a.join("target").join("release").join("openomsi.exe"));
            cands.push(a.join("Resources").join("openomsi"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for a in cwd.ancestors().take(4) {
            cands.push(a.join("target").join("release").join("openomsi"));
        }
    }
    cands.push(data_dir().join("openomsi"));
    cands.into_iter().find(|p| p.is_file())
}

/// The last search for the OMSI folder: (the places given first, what was found).
static FOUND: std::sync::Mutex<Option<(Vec<PathBuf>, Option<PathBuf>)>> =
    std::sync::Mutex::new(None);

/// The OMSI folder: configured, remembered by the game, or found in any usual place
/// (beside the program, Steam libraries, Wine bottles, the user's folders).
fn find_root(configured: &str) -> Option<PathBuf> {
    let mut first: Vec<PathBuf> = Vec::new();
    if !configured.trim().is_empty() {
        first.push(PathBuf::from(configured.trim()));
    }
    if let Some(p) = omsi_cfg::flags::OMSI_ROOT.live_os() {
        first.push(PathBuf::from(p));
    }
    if let Ok(t) = std::fs::read_to_string(home().join(".openomsi-root")) {
        first.push(PathBuf::from(t.trim()));
    }
    // searching the disk costs a moment: once per process is enough (until the settings
    // are saved again, see `save_config`)
    let mut g = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, v)) = g.as_ref() {
        if *k == first && v.as_ref().map(|p| is_omsi_root(p)).unwrap_or(true) {
            return v.clone();
        }
    }
    let r = omsi_cfg::find_original_install(&first);
    if let Some(p) = &r {
        // the game finds it the same way next time
        let _ = std::fs::write(
            home().join(".openomsi-root"),
            p.to_string_lossy().as_bytes(),
        );
    }
    *g = Some((first, r.clone()));
    r
}

pub fn load_config() -> Config {
    let mut c: Config = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if let Some(r) = find_root(&c.root) {
        c.root = r.to_string_lossy().to_string();
    }
    if let Some(g) = find_game(&c.game) {
        c.game = g.to_string_lossy().to_string();
    }
    if c.profile.trim().is_empty() {
        // a first start takes the driver OMSI 2 had last ([last_driver] of its options.cfg)
        c.profile = Path::new(&c.root)
            .is_dir()
            .then(|| omsi_options(Path::new(&c.root)))
            .flatten()
            .and_then(|o| o.last_driver)
            .unwrap_or_else(|| "Driver".into());
    }
    c
}

/// What the player's own OMSI 2 remembers in its `options.cfg`: the settings in the
/// launcher's keys, the map and the driver played last.
pub struct OmsiOptions {
    pub settings: Value,
    /// `maps/<name>/global.cfg`, as the launcher names maps.
    pub last_map: Option<String>,
    /// The personnel file's name without `.odr`.
    pub last_driver: Option<String>,
}

/// Read the original's `options.cfg` (never written: the game keeps its own settings).
pub fn omsi_options(root: &Path) -> Option<OmsiOptions> {
    let o = omsi_content::options::Options::load(&root.join("options.cfg")).ok()?;
    let mut v = json!({});
    let num = |k: &str| {
        o.str(k)
            .and_then(|x| x.trim().replace(',', ".").parse::<f64>().ok())
            .filter(|x| x.is_finite())
    };
    // (not on a phone: the PC's OMSI caps at 30, and a phone played at 30 frames)
    if let Some(x) = num("maxfps").filter(|_| !cfg!(any(target_os = "android", target_os = "ios")))
    {
        v["max_fps"] = json!(x.max(0.0) as i64);
    }
    if let Some(x) = num("performance_minobjsize") {
        v["min_obj_size"] = json!(x.clamp(0.0, 0.2));
    }
    if let Some(x) = num("performance_maxobjdist") {
        v["max_obj_dist"] = json!((x.round() as i64).max(0).to_string());
    }
    if let Some(af) = o
        .values
        .get("texfilter")
        .and_then(|x| x.get(1))
        .and_then(|x| x.trim().parse::<i64>().ok())
    {
        v["anisotropy"] = json!(af.clamp(1, 16));
    }
    // (`texmemlimit`, OMSI's texture budget, is not taken: openOMSI's video memory bounds
    // everything on the card, and the stock 401 would leave it 401 MB)
    if let Some(x) = num("performance_refltexsize") {
        v["mirror_size"] = json!(1i64 << (x as i64).clamp(6, 11));
    }
    if let Some(x) = o.str("performance_realreflexions") {
        v["mirror_refresh"] = json!(mirror_refresh(x));
    }
    if let Some(x) = num("sound_vol_master") {
        v["volume"] = json!(x.clamp(0.0, 1.0));
    }
    if let Some(d) = o.str("sound_doppler") {
        v["doppler"] = json!(!d.trim().eq_ignore_ascii_case("off"));
    }
    if let Some(l) = o.str("language").filter(|l| !l.trim().is_empty()) {
        v["language"] = json!(language_code(l));
    }
    if let Some(x) = num("wear_lifespan") {
        v["maintenance"] = json!((x as i64).clamp(0, 4));
    }
    if let Some(x) = num("aiunschedfactor") {
        v["ai_unsched_factor"] = json!((x as i64).clamp(0, 300));
    }
    if let Some(x) = num("aimaxcountscheduled") {
        v["ai_max_scheduled"] = json!((x as i64).max(0));
    }
    // (the second line of [AIMaxCountRandom]: the people Omsi.exe makes)
    if let Some(x) = o
        .values
        .get("aimaxcountrandom")
        .and_then(|x| x.get(1))
        .and_then(|x| x.trim().parse::<i64>().ok())
    {
        v["ai_max_humans"] = json!(x.max(1));
    }
    if let Some(x) = num("aimaxcountparked") {
        v["ai_max_parked"] = json!((x as i64).max(0));
    }
    if let Some(x) = num("aipassfactor") {
        v["pax_density"] = json!((x / 100.0).clamp(0.0, 3.0));
    }
    // flags: present or not
    v["head_movement"] = json!(o.flag("driverview_moving"));
    v["driverview_smooth"] = json!(o.flag("driverview_smooth"));
    v["collision_vehicles"] = json!(!o.flag("no_collision_vehtoveh"));
    v["collision_objects"] = json!(!o.flag("no_collision"));
    v["driver"] = json!(o.flag("see_own_driver"));
    if let Some(x) = num("ticketselling") {
        v["boarding"] = json!(if x > 0.5 { "pay" } else { "auto" });
    }
    let file_stem = |p: &str| {
        Path::new(&p.replace('\\', "/"))
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
    };
    Some(OmsiOptions {
        settings: v,
        last_map: o
            .str("last_map")
            .map(|m| m.trim().replace('\\', "/"))
            .filter(|m| !m.is_empty()),
        last_driver: o
            .str("last_driver")
            .and_then(file_stem)
            .filter(|d| !d.is_empty()),
    })
}

pub fn save_config(c: &Config) -> Result<()> {
    std::fs::write(config_path(), serde_json::to_vec_pretty(c)?)?;
    // the folder is looked for again: a folder that was not a complete OMSI 2 when it was
    // first chosen (still being copied, a part missing) and is now stayed "not found" until
    // the launcher was restarted, however often it was chosen and saved again
    *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(())
}

/// The OMSI 2 folder the launcher works with (configured or found).
pub fn root_dir() -> Result<PathBuf> {
    root()
}

/// The game program a duty is started with (see `find_game`).
pub fn game_binary() -> Option<PathBuf> {
    find_game(&load_config().game)
}

/// Where a content path (`maps/Grundorf/picture.jpg`) is: the content folder and the
/// archives first, then the OMSI 2 folder.
pub fn content_path(rel: &str) -> Result<PathBuf> {
    resolve_content(rel)
}

/// The buses the map's ailists.cfg gives a tour's AI group on `date` (#1886: to drive a
/// model the AI drives on it): of a depot group each vehicle type with a bus that runs
/// that day - the file, the first such bus's repaint and fleet number -, else the group's
/// vehicles, the most likely first. (file, paint, number)
pub fn tour_buses(map: &str, group: &str, date: &str) -> Vec<(String, String, String)> {
    let want = group.trim();
    let (Ok(dir), Some(day)) = (resolve_content(map), omsi_map::ailists::date_code(date)) else {
        return Vec::new();
    };
    let Some(dir) = dir.parent().filter(|_| !want.is_empty()) else {
        return Vec::new();
    };
    let lists = omsi_map::ailists::ailists_with_chrono(
        dir,
        &omsi_map::ailists::active_chrono_dirs(dir, day),
    );
    let Some(g) = lists
        .groups
        .iter()
        .find(|g| g.name.trim().eq_ignore_ascii_case(want))
    else {
        return Vec::new();
    };
    let file = |f: &str| f.trim().replace('\\', "/");
    let mut depot: Vec<(String, String, String)> = g
        .typgroups
        .iter()
        .filter_map(|t| {
            t.entries
                .iter()
                .find(|e| e.from.is_none_or(|f| f <= day) && e.to.is_none_or(|to| day <= to))
                .map(|e| {
                    (
                        file(&t.file),
                        e.paint.trim().to_string(),
                        e.number.trim().to_string(),
                    )
                })
        })
        .collect();
    // (a type listed in several groups of the depot: once)
    let mut seen = std::collections::HashSet::new();
    depot.retain(|(f, _, _)| seen.insert(f.to_ascii_lowercase()));
    if !depot.is_empty() {
        return depot;
    }
    let mut v: Vec<_> = g.vehicles.iter().collect();
    v.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    v.into_iter()
        .map(|v| {
            (
                file(&v.file),
                String::new(),
                v.number.clone().unwrap_or_default().trim().to_string(),
            )
        })
        .collect()
}

/// The depot file a bus uses on `map` on `date`: the map's own buses' of that day (chrono
/// scenarios move it - Berlin's 1994 depot has 137, 1986's 92), one of the bus's carrying
/// the line driven, its own of that name or named like the map, else the first (#896).
pub fn default_hof(
    map: Option<&MapInfo>,
    bus: Option<&VehicleInfo>,
    date: &str,
    line: Option<&str>,
) -> String {
    let on_date = map.and_then(|m| {
        let dir = resolve_content(&m.file).ok()?;
        omsi_map::ailists::depot_hof_on(dir.parent()?, omsi_map::ailists::date_code(date)?)
    });
    let want = on_date
        .or_else(|| map.map(|m| m.hof.clone()))
        .unwrap_or_default();
    let Some(v) = bus else { return want };
    let names: Vec<&str> = v.hofs.iter().map(|h| h.as_str()).collect();
    let like =
        |hints: &[&str]| omsi_vehicle::hof::closest_name(&names, hints).map(|i| v.hofs[i].clone());
    let map_hints: Vec<String> = map
        .map(|m| {
            vec![
                m.name.clone(),
                m.friendly.clone(),
                m.file
                    .trim_end_matches("/global.cfg")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string(),
            ]
        })
        .unwrap_or_default();
    let map_hints: Vec<&str> = map_hints.iter().map(|h| h.as_str()).collect();
    let with_line = match (line, root()) {
        (Some(line), Ok(r)) => depot_names_with_line(&r, &v.file, line),
        _ => Vec::new(),
    };
    // (not over the map's own depot borrowed from elsewhere when it carries the line: a bus
    // without it took a depot of its folder that had a line of the same number - the map's was
    // no longer given to a bus that lacks it, #1837)
    let map_depot_has_line = match (line, want.is_empty()) {
        (Some(line), false) => depot_has_line(&want, line) == Some(true),
        _ => false,
    };
    let with_line = match (line, map, root()) {
        (Some(line), Some(m), Ok(r)) if with_line.is_empty() && !map_depot_has_line => {
            depot_names_with_line(&r, &m.file, line)
        }
        _ => with_line,
    };
    if !with_line.is_empty()
        && !map_depot_has_line
        && !with_line.iter().any(|h| h.eq_ignore_ascii_case(&want))
    {
        let names: Vec<&str> = with_line.iter().map(|h| h.as_str()).collect();
        let hints: Vec<&str> = std::iter::once(want.as_str())
            .chain(map_hints.iter().copied())
            .collect();
        let pick = omsi_vehicle::hof::closest_name(&names, &hints).unwrap_or(0);
        return with_line[pick].clone();
    }
    v.hofs
        .iter()
        .find(|h| h.eq_ignore_ascii_case(&want))
        .cloned()
        .or_else(|| like(&[want.as_str()]))
        .or(Some(want.clone()).filter(|w| !w.is_empty()))
        .or_else(|| like(&map_hints))
        .or_else(|| v.hofs.first().cloned())
        .unwrap_or_default()
}

pub(crate) fn root() -> Result<PathBuf> {
    let c = load_config();
    let r = PathBuf::from(&c.root);
    if c.root.trim().is_empty() {
        return Err(anyhow!(i18n::tr(
            "no OMSI 2 folder configured (set it under Setup)"
        )));
    }
    let missing = omsi_cfg::missing_original_essentials(&r);
    if !missing.is_empty() {
        return Err(anyhow!(i18n::trf(
            "{folder} is not a complete OMSI 2 installation (missing: {missing}); choose the original game's folder under Setup",
            &[("folder", &r.display()), ("missing", &missing.join(", "))]
        )));
    }
    Ok(r)
}

/// The `upscaler=` setting as the game reads it: unknown words are the standard upscaler.
pub fn upscaler_word(s: &str) -> &'static str {
    match s.trim().to_ascii_lowercase().as_str() {
        "fsr1" => "fsr1",
        "dlss" => "dlss",
        "fsr3" => "fsr3",
        "metalfx" => "metalfx",
        _ => "default",
    }
}

/// The game's own content folder: the folder of the game binary, laid out like OMSI 2
/// (Vehicles, maps, Sceneryobjects ...). Installed mods live here; the game searches it
/// before the original installation.
pub fn content_dir() -> Option<PathBuf> {
    // the same rules as the game: $OMSI_CONTENT, else the folder of the game binary (beside
    // the bundle when the binary sits inside a macOS .app)
    let dir = match omsi_cfg::flags::OMSI_CONTENT.live_os() {
        Some(d) => PathBuf::from(d),
        None => {
            let c = load_config_raw();
            let game = find_game(&c.game)?;
            let dir = game.parent()?.to_path_buf();
            let beside = if dir.ends_with("Contents/MacOS") {
                dir.parent()?.parent()?.parent()?.to_path_buf()
            } else {
                dir
            };
            let cand = omsi_cfg::content_folder_of(&beside);
            if !omsi_cfg::is_programs_folder(&cand)
                && (cand.exists() || std::fs::create_dir_all(&cand).is_ok())
                && omsi_cfg::is_writable(&cand)
            {
                cand
            } else {
                data_dir().join("content")
            }
        }
    };
    let _ = omsi_cfg::ensure_content_layout(&dir);
    register_roots(&dir);
    Some(dir)
}

/// Tell the OMSI readers about the content folder, the archives used in place in its
/// `Archives` folder and the OMSI folder (in that order, as the game has them), so a path
/// inside one is also looked for in the others (a repaint's `.cti`, a map in an archive).
fn register_roots(content: &Path) {
    static DONE: std::sync::Once = std::sync::Once::new();
    let content = content.to_path_buf();
    DONE.call_once(|| {
        omsi_cfg::add_content_root(content.clone());
        mount_archives(&content);
    });
    // the OMSI folder - also one chosen under Setup while the launcher runs (it was only
    // looked for once, at the start, and a folder set later was never a root)
    if let Some(r) = find_root(&load_config_raw().root) {
        omsi_cfg::add_content_root(r);
    }
}

/// Mount the archives in `<content>/Archives` that are not mounted yet (an install may
/// have put one there, or the user did), each as a content root in front of the OMSI
/// folder. Archives that went away stay mounted until the launcher restarts.
fn mount_archives(content: &Path) {
    let dir = content.join(install::ARCHIVES);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut zips: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e.eq_ignore_ascii_case("zip"))
                    .unwrap_or(false)
        })
        .collect();
    zips.sort();
    let mounted: Vec<PathBuf> = omsi_cfg::vfs::mounts()
        .iter()
        .map(|m| m.path().to_path_buf())
        .collect();
    for z in zips.into_iter().filter(|z| !mounted.contains(z)) {
        mount_archive(&z);
    }
}

/// Mount one archive of the content folder for the lists.
pub(crate) fn mount_archive(zip: &Path) {
    match omsi_cfg::vfs::mount_zip(zip) {
        Ok(m) => match find_root(&load_config_raw().root) {
            Some(r) => omsi_cfg::add_content_root_before(m, &r),
            None => omsi_cfg::add_content_root(m),
        },
        Err(e) => {
            log_to_file(&format!("archive {}: {e}", zip.display()));
        }
    }
}

/// The archives of the content folder that are mounted, in name order.
fn archive_roots(content: &Path) -> Vec<PathBuf> {
    let dir = content.join(install::ARCHIVES);
    let mut v: Vec<PathBuf> = omsi_cfg::vfs::mounts()
        .iter()
        .map(|m| m.path().to_path_buf())
        .filter(|p| p.starts_with(&dir) && p.exists())
        .collect();
    v.sort();
    v
}

fn load_config_raw() -> Config {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// `rel` (a path as the game takes it, `Vehicles/Foo/foo.bus`) in the content folder if it
/// is there, else in the OMSI folder.
fn resolve_content(rel: &str) -> Result<PathBuf> {
    let root = root()?;
    for b in bases() {
        if b == root {
            continue;
        }
        let p = omsi_cfg::resolve_path(&b, rel);
        if omsi_cfg::vfs::exists(&p) {
            return Ok(p);
        }
    }
    Ok(omsi_cfg::resolve_path(&root, rel))
}

/// The folders the lists are made of: the content folder first, then the archives used in
/// place (mounted as folders), then the OMSI folder.
fn bases() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(c) = content_dir() {
        mount_archives(&c);
        v.push(c.clone());
        v.extend(archive_roots(&c));
    }
    if let Ok(r) = root() {
        v.push(r);
    }
    v
}

/// Entries of `rel` (e.g. "Vehicles") across the content folder and the OMSI 2 folder;
/// a name in the content folder hides the same name in the installation.
fn merged_entries(rel: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let dirs: Vec<PathBuf> = bases().iter().map(|b| b.join(rel)).collect();
    for d in dirs {
        for (name, _) in omsi_cfg::vfs::list_dir(&d).unwrap_or_default() {
            if seen.insert(name.to_string_lossy().to_ascii_lowercase()) {
                out.push(d.join(name));
            }
        }
    }
    out.sort();
    out
}

/// Folders of `rel` by name across the content folder and the OMSI folder: each name with
/// all its copies, the content folder's first (a mod may add files to a stock folder).
fn merged_folders(rel: &str) -> Vec<(String, Vec<PathBuf>)> {
    let mut out: Vec<(String, Vec<PathBuf>)> = Vec::new();
    let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for base in bases() {
        let dir = base.join(rel);
        let Some(list) = omsi_cfg::vfs::list_dir(&dir) else {
            continue;
        };
        let mut names: Vec<PathBuf> = list
            .into_iter()
            .filter(|(_, is_dir)| *is_dir)
            .map(|(n, _)| dir.join(n))
            .collect();
        names.sort();
        for p in names {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.starts_with('.') {
                continue;
            }
            // (by an index: thousands of folders compared each with all before took long)
            match at.get(&name.to_ascii_lowercase()) {
                Some(&i) => out[i].1.push(p),
                None => {
                    at.insert(name.to_ascii_lowercase(), out.len());
                    out.push((name, vec![p]));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    out
}

/// Is `p` in the content folder (a mod) rather than the OMSI folder?
/// (An archive used in place lies in the content folder's `Archives` too.)
fn in_content(p: &Path) -> bool {
    content_dir().map(|c| p.starts_with(c)).unwrap_or(false)
}

// ---------------------------------------------------------------------------------------
// mods: sorting a mod's folders into the content folder, the way it would be copied into
// an OMSI 2 installation by hand

#[derive(Serialize, Clone, Debug)]
pub struct ModsStatus {
    pub content_dir: String,
    /// (folder, number of entries)
    pub folders: Vec<(String, usize)>,
    pub inbox: String,
    /// What lies in the inbox now.
    pub inbox_items: Vec<String>,
    /// Packs waiting in Mods/waiting for the bus they belong to.
    pub waiting: Vec<String>,
    /// Archives used in place (in `<content>/Archives`): (name, size in bytes).
    pub archives: Vec<(String, u64)>,
    /// Free space on the content folder's disk (bytes).
    pub free_bytes: u64,
    /// What an earlier, interrupted install left and was removed now.
    pub cleaned: Vec<String>,
    pub jobs: Vec<install::Progress>,
    pub hof_warnings: Vec<String>,
    pub hof_offers: Vec<MapHofOffer>,
    /// Every mod, one by one (see `mods`).
    pub installed: Vec<mods::Mod>,
}

/// Start installing the mod at `src` (a folder, .zip, .7z or .rar) into the content folder in the
/// background; `mode` is "auto" (unpack, or use a .zip in place when it does not fit; .7z
/// and .rar are always unpacked),
/// "extract" or "inplace".
pub fn start_install(src: &Path, mode: &str) -> Result<install::Progress> {
    let content = content_dir()
        .ok_or_else(|| anyhow!(i18n::tr("no game binary configured, so no content folder")))?;
    if !src.exists() {
        return Err(anyhow!("{} does not exist", src.display()));
    }
    let from_inbox = src.starts_with(content.join("Mods"));
    let job = install::start(
        content,
        root().ok(),
        src.to_path_buf(),
        install::InstallMode::parse(mode),
        from_inbox,
    );
    Ok(job.snapshot())
}

/// How big the mod at `src` is unpacked, what is free, and whether it can be used in place.
pub fn inspect_mod(src: &Path) -> Result<install::SourceInfo> {
    let content = content_dir()
        .ok_or_else(|| anyhow!(i18n::tr("no game binary configured, so no content folder")))?;
    install::inspect(&content, root().ok().as_deref(), src)
}

/// Install the mod at `src` and wait for it (the terminal), printing the progress.
pub fn install_mod_blocking(
    src: &Path,
    mode: &str,
    cancel_after_ms: Option<u64>,
) -> Result<install::Progress> {
    let content = content_dir()
        .ok_or_else(|| anyhow!(i18n::tr("no game binary configured, so no content folder")))?;
    Ok(install::run_blocking(
        content,
        root().ok(),
        src.to_path_buf(),
        install::InstallMode::parse(mode),
        cancel_after_ms.map(std::time::Duration::from_millis),
        true,
    ))
}

fn inbox_entries(content: &Path) -> Vec<PathBuf> {
    let inbox = content.join("Mods");
    let Ok(rd) = std::fs::read_dir(&inbox) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            !(name.starts_with('.')
                || name.eq_ignore_ascii_case("installed")
                || name.eq_ignore_ascii_case(install::WAITING)
                || name.eq_ignore_ascii_case(install::PLUGINS_HELD)
                || name.eq_ignore_ascii_case(install::UNINSTALLED)
                || name.eq_ignore_ascii_case(mods::DISABLED)
                || name.eq_ignore_ascii_case("README.txt"))
                && (p.is_dir()
                    || p.extension()
                        .map(|x| {
                            ["zip", "7z", "rar"]
                                .iter()
                                .any(|ext| x.eq_ignore_ascii_case(ext))
                        })
                        .unwrap_or(false))
        })
        .collect();
    v.sort();
    v
}

/// (size, newest time, files) of a file or a folder tree.
fn tree_signature(p: &Path) -> (u64, u64, u64) {
    let Ok(md) = std::fs::symlink_metadata(p) else {
        return (0, 0, 0);
    };
    let t = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    if !md.is_dir() {
        return (md.len(), t, 1);
    }
    let mut acc = (0, t, 0);
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let (s, m, n) = tree_signature(&e.path());
            acc = (acc.0 + s, acc.1.max(m), acc.2 + n);
        }
    }
    acc
}

/// The inbox watcher: something dropped into Mods/ is installed once it has stopped
/// growing (the same size and time on two looks at least two seconds apart); packs in
/// Mods/waiting are installed once their bus is. Returns the sources of the jobs started.
fn watch_inbox(content: &Path) -> Vec<String> {
    struct Seen {
        sig: (u64, u64, u64),
        at: std::time::Instant,
        started: bool,
        /// When the signature was last read (a big folder whose install failed is looked
        /// at again only now and then).
        checked: std::time::Instant,
    }
    static SEEN: std::sync::Mutex<Option<std::collections::HashMap<PathBuf, Seen>>> =
        std::sync::Mutex::new(None);
    // a mod deleted from Mods/installed is taken out of the lists (#819)
    let gone = install::uninstall_removed(content);
    for g in &gone {
        log_line(&format!(
            "mods: {g} was deleted from Mods/installed - uninstalled, its folders are in Mods/{}/{g}",
            install::UNINSTALLED
        ));
    }
    if !gone.is_empty() {
        omsi_cfg::content_changed();
    }
    let mut started = Vec::new();
    let mut guard = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let seen = guard.get_or_insert_with(Default::default);
    let waiting_dir = content.join("Mods").join(install::WAITING);
    let items = inbox_entries(content);
    seen.retain(|p, _| items.contains(p) || (p.starts_with(&waiting_dir) && p.exists()));
    for p in items {
        if install::is_busy(&p) {
            continue;
        }
        let now = std::time::Instant::now();
        if seen
            .get(&p)
            .map(|s| {
                s.started && now.duration_since(s.checked) < std::time::Duration::from_secs(30)
            })
            .unwrap_or(false)
        {
            continue;
        }
        let sig = tree_signature(&p);
        if let Some(s) = seen.get_mut(&p) {
            s.checked = now;
        }
        match seen.get_mut(&p) {
            Some(s) if s.sig == sig => {
                if !s.started && now.duration_since(s.at) >= std::time::Duration::from_secs(2) {
                    s.started = true;
                    install::start(
                        content.to_path_buf(),
                        root().ok(),
                        p.clone(),
                        install::InstallMode::Auto,
                        true,
                    );
                    started.push(p.to_string_lossy().to_string());
                }
            }
            // new, still growing (a copy in progress), or changed after a failed try
            _ => {
                seen.insert(
                    p.clone(),
                    Seen {
                        sig,
                        at: now,
                        started: false,
                        checked: now,
                    },
                );
            }
        }
    }
    let busy = install::jobs().iter().any(|j| j.finished.is_none());
    if !busy {
        for p in install::waiting_ready(content, root().ok().as_deref()) {
            let sig = tree_signature(&p);
            if seen
                .get(&p)
                .map(|s| s.started && s.sig == sig)
                .unwrap_or(false)
            {
                continue;
            }
            seen.insert(
                p.clone(),
                Seen {
                    sig,
                    at: std::time::Instant::now(),
                    started: true,
                    checked: std::time::Instant::now(),
                },
            );
            install::start(
                content.to_path_buf(),
                root().ok(),
                p.clone(),
                install::InstallMode::Extract,
                true,
            );
            started.push(p.to_string_lossy().to_string());
        }
    }
    started
}

/// Install everything in the inbox (and the waiting packs whose bus is there) now, and
/// wait (the terminal's `mods`).
pub fn install_inbox_blocking() -> Vec<install::Progress> {
    let Some(content) = content_dir() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in inbox_entries(&content)
        .into_iter()
        .chain(install::waiting_ready(&content, root().ok().as_deref()))
    {
        let job = install::start(
            content.clone(),
            root().ok(),
            p,
            install::InstallMode::Auto,
            true,
        );
        while job.snapshot().finished.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        out.push(job.snapshot());
    }
    out
}

pub fn mods_status() -> Result<ModsStatus> {
    let content = content_dir()
        .ok_or_else(|| anyhow!(i18n::tr("no game binary configured, so no content folder")))?;
    let cleaned = install::cleanup_stale(&data_dir(), Some(&content));
    let (hof_offers, hof_warnings) = hof_sync::offers();
    let folders = omsi_cfg::CONTENT_FOLDERS
        .iter()
        .map(|f| {
            let n = std::fs::read_dir(content.join(f))
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                        .count()
                })
                .unwrap_or(0);
            (f.to_string(), n)
        })
        .collect();
    let names = |v: Vec<PathBuf>| {
        v.iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .collect::<Vec<_>>()
    };
    let waiting: Vec<PathBuf> = std::fs::read_dir(content.join("Mods").join(install::WAITING))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    let mut archives: Vec<(String, u64)> = std::fs::read_dir(content.join(install::ARCHIVES))
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|x| x.eq_ignore_ascii_case("zip"))
                        .unwrap_or(false)
                })
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().to_string(),
                        e.metadata().map(|m| m.len()).unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    archives.sort();
    Ok(ModsStatus {
        content_dir: content.to_string_lossy().to_string(),
        folders,
        inbox: content.join("Mods").to_string_lossy().to_string(),
        inbox_items: names(inbox_entries(&content)),
        waiting: names(waiting),
        archives,
        free_bytes: install::free_space(&content).unwrap_or(0),
        cleaned,
        jobs: install::jobs(),
        hof_warnings,
        hof_offers,
        installed: mods::list(&content),
    })
}

/// Switch the mod `id` of the content folder off or on (see `mods::set_enabled`); its name.
pub fn mod_set_enabled(id: &str, on: bool) -> Result<String> {
    let content = content_dir()
        .ok_or_else(|| anyhow!(i18n::tr("no game binary configured, so no content folder")))?;
    mods::set_enabled(&content, id, on)
}

/// Delete the mod `id` of the content folder (see `mods::remove`); its name.
pub fn mod_remove(id: &str) -> Result<String> {
    let content = content_dir()
        .ok_or_else(|| anyhow!(i18n::tr("no game binary configured, so no content folder")))?;
    mods::remove(&content, id)
}

pub fn mod_remove_preview(id: &str) -> Result<map_uses::Plan> {
    let content = content_dir()
        .ok_or_else(|| anyhow!(i18n::tr("no game binary configured, so no content folder")))?;
    mods::remove_preview(&content, id)
}

/// What the page asks every few seconds: whether the content changed (then it asks for the
/// lists again), the install jobs and the running games. It also starts the inbox installs.
#[derive(Serialize, Clone, Debug)]
pub struct Poll {
    pub stamp: String,
    pub jobs: Vec<install::Progress>,
    pub instances: Vec<instances::Instance>,
    /// The games that ended within the last half hour (as long as the registry keeps them):
    /// how each ended (its exit code, or killed by Stop) and its log, apart from the running ones.
    pub ended: Vec<instances::Instance>,
    /// Inbox items whose install started with this poll.
    pub started: Vec<String>,
}

pub fn poll() -> Result<Poll> {
    let content = content_dir();
    let started = content.as_deref().map(watch_inbox).unwrap_or_default();
    let stamp = index::content_stamp(
        &bases(),
        content.as_ref().map(|c| c.join("Mods")).as_deref(),
    );
    // (only the games still running in `instances`: an ended one stays listed a while for its
    // log, in `ended`, not as running)
    let (running, ended) = instances::list().into_iter().partition(|i| i.running);
    Ok(Poll {
        stamp,
        jobs: install::jobs(),
        instances: running,
        ended,
        started,
    })
}

mod airports;
pub use airports::nearest_airport;
mod custom_weather;
pub use custom_weather::{CustomWeather, custom_weather};
pub use omsi_content::weather_choice::metar_airports;
mod date_codes;
use date_codes::{add_days, day_before, days_of, weekday};
mod content_lists;
pub use content_lists::*;
use content_lists::{read_map, missing_packs_of, paint_schemes, dsc_candidates, parse_dsc};
use content_lists::{content_language, date_for_game, log_line, year_span_of};

mod timetable_lists;
pub use timetable_lists::*;
use timetable_lists::lines_on;

// ---------------------------------------------------------------------------------------
// profiles: the driver's personnel file plus the sessions the game writes

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Session {
    pub time: u64,
    pub driver: String,
    pub map: String,
    pub bus: String,
    pub line: Option<String>,
    pub tour: Option<String>,
    pub seconds: f64,
    pub metres: f64,
    pub stops: i32,
    pub early: i32,
    pub late: i32,
    pub tickets: i32,
    pub cash: f64,
    pub crashes: i32,
    pub hurt: i32,
    pub jolts: i32,
    pub driving: f64,
    pub comfort: f64,
    pub ticketing: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub file: String,
    pub hours: f64,
    pub km: f64,
    pub xp: i64,
    pub level: i64,
    pub next_level_xp: i64,
    pub stops: i32,
    pub early: i32,
    pub late: i32,
    pub tickets: f64,
    pub cash: f64,
    pub crashes: i32,
    pub hurt: i32,
    pub rating_driving: f64,
    pub rating_comfort: f64,
    pub rating_tickets: f64,
    pub sessions: Vec<Session>,
    pub exists: bool,
}

fn sessions() -> Vec<Session> {
    let dir = data_dir().join("sessions");
    let mut out: Vec<Session> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .filter_map(|t| serde_json::from_str::<Session>(&t).ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.time.cmp(&a.time));
    out
}

/// Experience: a point per hundred metres, five per stop served on time, two per ticket,
/// minus twenty per crash and fifty per pedestrian; the level grows with the square root.
fn xp_of(s: &Session) -> i64 {
    let on_time = (s.stops - s.early - s.late).max(0) as i64;
    let xp =
        (s.metres / 100.0) as i64 + on_time * 5 + s.tickets as i64 * 2 + (s.seconds / 60.0) as i64
            - s.crashes as i64 * 20
            - s.hurt as i64 * 50
            - s.jolts as i64;
    xp.max(0)
}

fn level_of(xp: i64) -> (i64, i64) {
    let level = ((xp as f64 / 250.0).sqrt().floor() as i64) + 1;
    let next = (level * level) as i64 * 250;
    (level, next)
}

/// Where a driver's personnel file is written: the content folder's `Drivers` (the
/// original installation is only ever read).
fn driver_write_path(root: &Path, name: &str) -> PathBuf {
    content_dir()
        .unwrap_or_else(|| root.to_path_buf())
        .join("Drivers")
        .join(format!("{name}.odr"))
}

/// Where a driver's personnel file is read: the content folder's copy once the game has
/// written one, else the original installation's (the stock `OMSI-Fan.odr`).
fn driver_read_path(root: &Path, name: &str) -> PathBuf {
    let own = driver_write_path(root, name);
    if own.exists() {
        own
    } else {
        root.join("Drivers").join(format!("{name}.odr"))
    }
}

pub fn list_profiles() -> Result<Vec<String>> {
    let root = root()?;
    let odr_names = |dir: PathBuf| -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .map(|e| e.eq_ignore_ascii_case("odr"))
                            .unwrap_or(false)
                    })
                    .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut names = odr_names(root.join("Drivers"));
    if let Some(c) = content_dir() {
        names.extend(odr_names(c.join("Drivers")));
    }
    for s in sessions() {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&s.driver)) {
            names.push(s.driver.clone());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn get_profile(name: &str) -> Result<Profile> {
    let root = root()?;
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!(i18n::tr("a profile needs a name")));
    }
    let file = driver_read_path(&root, name);
    let driver = omsi_content::driver::Driver::load(&file).ok();
    let mine: Vec<Session> = sessions()
        .into_iter()
        .filter(|s| s.driver.eq_ignore_ascii_case(name))
        .collect();
    let xp: i64 = mine.iter().map(xp_of).sum();
    let (level, next) = level_of(xp);
    let hours = mine.iter().map(|s| s.seconds).sum::<f64>() / 3600.0;
    let (km, stops, early, late, tickets, cash, crashes, hurt, rating) = match &driver {
        Some(d) => (
            d.hektom / 10.0,
            d.bus_stops[0],
            d.bus_stops[1],
            d.bus_stops[2],
            d.tickets[0],
            d.tickets[1],
            d.crashes[0],
            d.crashes[1],
            [
                d.driving_percent(),
                d.comfort_percent().unwrap_or(100.0),
                d.ticket_percent().unwrap_or(100.0),
            ],
        ),
        None => {
            let km = mine.iter().map(|s| s.metres).sum::<f64>() / 1000.0;
            let n = mine.len().max(1) as f64;
            (
                km,
                mine.iter().map(|s| s.stops).sum(),
                mine.iter().map(|s| s.early).sum(),
                mine.iter().map(|s| s.late).sum(),
                mine.iter().map(|s| s.tickets as f64).sum(),
                mine.iter().map(|s| s.cash).sum(),
                mine.iter().map(|s| s.crashes).sum(),
                mine.iter().map(|s| s.hurt).sum(),
                [
                    mine.iter().map(|s| s.driving).sum::<f64>() / n,
                    mine.iter().map(|s| s.comfort).sum::<f64>() / n,
                    mine.iter().map(|s| s.ticketing).sum::<f64>() / n,
                ],
            )
        }
    };
    Ok(Profile {
        name: name.to_string(),
        file: format!("Drivers/{name}.odr"),
        hours,
        km,
        xp,
        level,
        next_level_xp: next,
        stops,
        early,
        late,
        tickets,
        cash,
        crashes,
        hurt,
        rating_driving: rating[0],
        rating_comfort: rating[1],
        rating_tickets: rating[2],
        sessions: mine.into_iter().take(40).collect(),
        exists: driver.is_some(),
    })
}

/// Create the personnel file for a new driver (OMSI's own `.odr` format), so that the game
/// can add every run to it.
pub fn create_profile(name: &str, sex: &str) -> Result<Profile> {
    let root = root()?;
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) {
        return Err(anyhow!(i18n::tr("the name must be a plain file name")));
    }
    let file = driver_write_path(&root, name);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if !file.exists() && !driver_read_path(&root, name).exists() {
        let d = omsi_content::driver::Driver {
            path: file.clone(),
            name: name.to_string(),
            sex: if sex.trim().is_empty() {
                "M".into()
            } else {
                sex.trim().to_string()
            },
            ..Default::default()
        };
        d.save(&file)?;
    }
    let mut c = load_config();
    c.profile = name.to_string();
    save_config(&c)?;
    get_profile(name)
}

/// Remove only the history belonging to this driver. Unreadable or malformed records
/// have no trustworthy owner and are kept, as are records belonging to other drivers.
fn delete_profile_sessions(dir: &Path, name: &str) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(session) = std::fs::read_to_string(entry.path())
            .ok()
            .and_then(|text| serde_json::from_str::<Session>(&text).ok())
        else {
            continue;
        };
        if session.driver.eq_ignore_ascii_case(name) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Delete the personnel file and its run history. A driver who exists only in the
/// original installation is left there: the original is never written.
pub fn delete_profile(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) {
        return Err(anyhow!(i18n::tr("the name must be a plain file name")));
    }
    let root = root()?;
    let file = driver_write_path(&root, name);
    if file.exists() {
        std::fs::remove_file(&file)?;
    } else if root
        .join("Drivers")
        .join(format!("{}.odr", name.trim()))
        .exists()
    {
        return Err(anyhow!(i18n::trf(
            "'{name}' belongs to the original OMSI 2 installation, which is not changed",
            &[("name", &name.trim())]
        )));
    }
    delete_profile_sessions(&data_dir().join("sessions"), name)
}

#[cfg(test)]
mod profile_cleanup_tests;

// ---------------------------------------------------------------------------------------
// key bindings: the newer of ~/.openomsi/Inputs/keyboard.cfg and the content folder's once
// the page has saved one (the game reads that first), else the original installation's,
// which is never written

pub fn adopt_own_inputs() {
    let game = load_config_raw().game;
    omsi_cfg::adopt_own_inputs(Some(Path::new(game.trim())).filter(|p| !p.as_os_str().is_empty()));
}

fn keyboard_cfg_write_path() -> Result<PathBuf> {
    omsi_cfg::own_input("keyboard.cfg")
        .ok_or_else(|| anyhow!("no home folder to keep the key bindings in"))
}

fn keyboard_cfg_read_path() -> Result<PathBuf> {
    match omsi_cfg::input_in_use("keyboard.cfg") {
        Some(p) => Ok(p),
        None => Ok(omsi_cfg::original_keyboard_cfg(&root()?)),
    }
}

fn binding_to_json(b: &omsi_content::input::KeyBinding) -> Value {
    json!({ "action": b.action, "scan_code": b.scan_code, "modifier": b.modifier })
}

fn binding_from_json(v: &Value) -> Option<omsi_content::input::KeyBinding> {
    Some(omsi_content::input::KeyBinding {
        action: v.get("action")?.as_str()?.to_string(),
        scan_code: v.get("scan_code")?.as_i64()? as i32,
        modifier: v.get("modifier").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
    })
}

pub fn get_keybindings() -> Result<Value> {
    let path = keyboard_cfg_read_path()?;
    let k = omsi_content::input::KeyboardCfg::load(&path)?
        .with_game_defaults()
        .with_vr_defaults();
    Ok(
        json!({ "game": k.game.iter().map(binding_to_json).collect::<Vec<_>>(), "vehicles": k.vehicles.iter().map(binding_to_json).collect::<Vec<_>>() }),
    )
}

/// Replace the bindings with the page's list. Written to a temp file and read back through
/// the same loader the game uses before it replaces the real file, so a page bug never
/// leaves the player with a `keyboard.cfg` the game itself cannot parse.
pub fn save_keybindings(v: &Value) -> Result<()> {
    let list = |k: &str| -> Vec<omsi_content::input::KeyBinding> {
        v.get(k)
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(binding_from_json).collect())
            .unwrap_or_default()
    };
    let k = omsi_content::input::KeyboardCfg {
        game: list("game"),
        vehicles: list("vehicles"),
    };
    let path = keyboard_cfg_write_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("cfg.tmp");
    k.save(&tmp)?;
    omsi_content::input::KeyboardCfg::load(&tmp)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

mod game_settings;
use game_settings::mirror_refresh;
pub use game_settings::*;

// ---------------------------------------------------------------------------------------
// the 3D preview and launching the game

/// Ask the game to write the bus as glTF (cached by bus and paint) and return the path.
pub fn bus_preview(bus: &str, paint: &str) -> Result<String> {
    let c = load_config();
    let game = find_game(&c.game).context("the game binary was not found (set it under Setup)")?;
    let root = root()?;
    let key: String = format!("{bus}|{paint}")
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    let out = data_dir().join("cache").join(format!("{key}.glb"));
    let bus_path = resolve_content(bus)?;
    // a bus inside an archive used in place changes with the archive
    let bus_path = omsi_cfg::vfs::archive_of(&bus_path).unwrap_or(bus_path);
    let newest_input = [&bus_path, &game]
        .iter()
        .filter_map(|p| p.metadata().and_then(|m| m.modified()).ok())
        .max();
    let fresh = out
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .zip(newest_input)
        .map(|(o, b)| o >= b)
        .unwrap_or(false);
    if !fresh {
        let mut cmd = std::process::Command::new(&game);
        cmd.arg("--root")
            .arg(&root)
            .arg("--bus")
            .arg(bus)
            .arg("--export-glb")
            .arg(&out);
        if !paint.trim().is_empty() {
            cmd.arg("--paint").arg(paint.trim());
        }
        let status = cmd
            .env("RUST_LOG", "warn")
            .status()
            .context("running the game for the preview")?;
        if !status.success() || !out.exists() {
            return Err(anyhow!("the game could not export {bus}"));
        }
    }
    Ok(out.to_string_lossy().to_string())
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Duty {
    pub map: String,
    pub bus: String,
    pub paint: Option<String>,
    /// The number plate (registration) the player typed: it wins over the plate the bus's
    /// `[number]` list or the map's `registrations.txt` gives it (empty: as the content says).
    #[serde(default)]
    pub plate: Option<String>,
    /// The fleet number picked from the bus's `[number]` list (none: its first).
    #[serde(default)]
    pub number: Option<String>,
    pub hof: Option<String>,
    pub entry: Option<i32>,
    pub line: Option<String>,
    pub tour: Option<String>,
    /// The trip of the tour to start with: its departure (HH:MM) or its place in the tour.
    #[serde(default)]
    pub trip: Option<String>,
    #[serde(default)]
    pub whole_tour: bool,
    /// HH:MM
    pub time: String,
    /// YYYY-MM-DD
    pub date: Option<String>,
    pub weather: Option<String>,
    pub traffic: Option<u32>,
    pub passengers: Option<bool>,
    pub schedule: Option<bool>,
    pub autostart: Option<bool>,
    pub comfortable_cabin_on_start: Option<bool>,
    /// Start as a pedestrian beside the bus (which is then left out): a bus is placed or
    /// taken over from the game menu later.
    #[serde(default)]
    pub on_foot: Option<bool>,
    pub profile: Option<String>,
    /// LAN play: "host", or "join:<session code | ip[:port] | port | empty = search>".
    pub lan: Option<String>,
    /// The name the other LAN players see (default: the profile).
    pub lan_name: Option<String>,
    /// Discovered web gateway to try if the initial UDP join gets no answer (Auto only).
    pub lan_ws_fallback: Option<String>,
    /// Explicit per-launch authorization for server-defined vehicle content.
    #[serde(default)]
    pub allow_server_fleet: bool,
    /// Season override: spring / summer / autumn / winter, with a phase `-early` / `-late`
    /// (`autumn-late`; none = the middle; empty = by date).
    pub season: Option<String>,
    /// One of OMSI's tutorials (1..4): its own situation, nothing else of the duty.
    #[serde(default)]
    pub tutorial: Option<usize>,
    /// A situation file to continue (the map's `laststn.osn`): nothing else of the duty.
    #[serde(default)]
    pub situation: Option<String>,
    /// The depot bus driven: it starts with the wear it was left with (see [`depot`]).
    #[serde(default)]
    pub depot: Option<String>,
}

/// The command line a duty becomes.
pub fn duty_args(d: &Duty) -> Result<Vec<String>> {
    let root = root()?;
    duty_args_for_root(d, &root)
}

// The installation is validated by duty_args; argument tests supply their own path.
fn duty_args_for_root(d: &Duty, root: &Path) -> Result<Vec<String>> {
    duty_args_with_settings(d, root, &get_settings().unwrap_or_default())
}

fn duty_args_with_settings(d: &Duty, root: &Path, st: &Value) -> Result<Vec<String>> {
    let mut start = vec![
        "--root".into(),
        root.to_string_lossy().to_string(),
        "--no-menu".into(),
    ];
    if st
        .get("disable_plugins")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        start.push("--no-plugins".into());
    }
    if let Some(t) = d.tutorial {
        start.extend(["--tutorial".into(), t.to_string()]);
        return Ok(start);
    }
    if let Some(sit) = d.situation.as_deref().filter(|s| !s.trim().is_empty()) {
        let mut a = start;
        a.extend(["--situation".into(), sit.to_string()]);
        if let Some(p) = d.profile.as_deref().filter(|p| !p.trim().is_empty()) {
            a.extend(["--driver".into(), format!("Drivers/{}.odr", p.trim())]);
        }
        // the traffic and the people as for any other start: a situation file keeps the
        // vehicles the player placed, not how busy the streets are (OMSI takes that from its
        // options), and without these a continued session had the timetable buses alone -
        // no cars, nobody at the stops (#136)
        a.extend(["--traffic".into(), d.traffic.unwrap_or(30).to_string()]);
        if d.passengers.unwrap_or(true) {
            a.push("--passengers".into());
        }
        return Ok(a);
    }
    let mut a = start;
    a.extend([
        "--map".into(),
        d.map.clone(),
        "--bus".into(),
        d.bus.clone(),
        "--time".into(),
        if d.time.trim().is_empty() {
            "09:00".into()
        } else {
            d.time.trim().to_string()
        },
    ]);
    if let Some(p) = d.paint.as_deref().filter(|p| !p.trim().is_empty()) {
        a.extend(["--paint".into(), p.trim().to_string()]);
    }
    if let Some(pl) = d.plate.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        a.extend(["--plate".into(), pl.to_string()]);
    }
    if let Some(n) = d.number.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        a.extend(["--number".into(), n.to_string()]);
    }
    if let Some(id) = d.depot.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        a.extend(["--depot".into(), id.to_string()]);
    }
    // (a vehicle file taken for a depot from a broken ailists.cfg by older launchers is none)
    if let Some(h) = d.hof.as_deref().filter(|h| {
        !h.trim().is_empty()
            && !h.to_ascii_lowercase().contains(".bus")
            && !h.to_ascii_lowercase().contains(".ovh")
    }) {
        a.extend(["--hof".into(), h.trim().to_string()]);
    }
    // the entry point's place in the map's list; -1: the one nearest to the duty's first
    // stop (a free drive takes the list's first then)
    match d.entry {
        Some(e) if e < 0 => {
            if d.line
                .as_deref()
                .map(|l| !l.trim().is_empty())
                .unwrap_or(false)
            {
                a.push("--auto-entry".into());
            }
        }
        Some(e) => a.extend(["--entry".into(), e.to_string()]),
        None => {}
    }
    // OMSI's [useActTime] / [useActDate] / [useActYear]: the machine's clock and calendar
    // instead of the duty's (the year only on its own switch - a map's timetable is for
    // its years)
    let on = |k: &str| st.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let now = local_now();
    if let (true, Some((_, _, _, h, m))) = (on("use_real_time"), now) {
        if let Some(i) = a.iter().position(|x| x == "--time") {
            a[i + 1] = format!("{h:02}:{m:02}");
        }
        // (a duty picked for later starts now, not ten minutes before its first trip)
        a.push("--keep-time".into());
    }
    let mut date = d
        .date
        .as_deref()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty());
    if let (true, Some((y, mo, dd, _, _))) = (on("use_real_date"), now) {
        let year = if on("use_real_year") {
            y
        } else {
            date.as_deref()
                .and_then(|x| x.split('-').next()?.parse::<i32>().ok())
                .unwrap_or(y)
        };
        date = Some(format!("{year:04}-{mo:02}-{dd:02}"));
    }
    if let Some(dt) = date {
        // (only the machine's calendar is fitted to the map's [years], so that its AI lists,
        // chrono and timetables fit; the date the player chose in the launcher stays)
        let real_date = on("use_real_date") && !on("use_real_year");
        let span = if real_date {
            let file = root.join(&d.map);
            let file = if file.is_file() {
                file
            } else {
                resolve_content(&d.map).unwrap_or(file)
            };
            omsi_map::GlobalCfg::load(&file)
                .ok()
                .and_then(|g| year_span_of(&g))
        } else {
            None
        };
        a.extend(["--date".into(), date_for_game(&dt, real_date, span)]);
    }
    if let Some(w) = d.weather.as_deref().filter(|x| !x.trim().is_empty()) {
        a.extend(["--weather".into(), w.trim().to_string()]);
    }
    a.extend(["--traffic".into(), d.traffic.unwrap_or(30).to_string()]);
    if d.passengers.unwrap_or(true) {
        a.push("--passengers".into());
    }
    // (a duty needs the timetable even with the timetable buses switched off: the game
    // then runs it for the player alone, #1762)
    let schedule = d.schedule.unwrap_or(true) || d.line.is_some();
    if schedule {
        a.push("--schedule".into());
        if d.schedule == Some(false) {
            a.push("--no-timetable-buses".into());
        }
    }
    if let (Some(l), true) = (d.line.as_deref().filter(|x| !x.trim().is_empty()), schedule) {
        a.extend(["--line".into(), l.trim().to_string()]);
        if let Some(t) = d.tour.as_deref().filter(|x| !x.trim().is_empty()) {
            a.extend(["--tour".into(), t.trim().to_string()]);
            if let Some(tr) = d.trip.as_deref().filter(|x| !x.trim().is_empty()) {
                a.extend(["--trip".into(), tr.trim().to_string()]);
                if d.whole_tour {
                    a.push("--whole-tour".into());
                }
            }
        }
    }
    if d.autostart.unwrap_or(false) {
        a.push("--autostart".into());
    }
    if d.comfortable_cabin_on_start.unwrap_or(false) {
        a.push("--comfortable-cabin-on-start".into());
    }
    if d.on_foot.unwrap_or(false) {
        a.push("--on-foot".into());
    }
    let profile = d
        .profile
        .clone()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| load_config().profile);
    if let Some(season) = d
        .season
        .as_deref()
        .map(str::trim)
        .filter(|x| !x.is_empty() && !x.eq_ignore_ascii_case("auto"))
    {
        a.extend(["--season".into(), season.to_ascii_lowercase()]);
    }
    if let Some(lan) = d
        .lan
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("off"))
    {
        if lan.eq_ignore_ascii_case("host") {
            // 0: the default port, or the next free one when a session runs here already
            a.extend(["--lan-host".into(), "0".into()]);
        } else if let Some(target) = lan.strip_prefix("join:") {
            let target = target.trim();
            omsi_net::describe_join(target).map_err(|e| anyhow!("LAN join: {e}"))?;
            a.extend([
                "--lan-join".into(),
                if target.is_empty() {
                    "auto".into()
                } else {
                    target.to_string()
                },
            ]);
            if let Some(url) = d.lan_ws_fallback.as_deref().filter(|url| !url.is_empty()) {
                a.extend(["--lan-ws-fallback".into(), url.to_string()]);
            }
            if d.allow_server_fleet {
                a.push("--allow-server-fleet".into());
            }
        } else {
            return Err(anyhow!(
                "LAN play: '{lan}' is neither host nor join:<code or address>"
            ));
        }
        let name = d
            .lan_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| profile.clone());
        if !name.trim().is_empty() {
            a.extend(["--lan-name".into(), name.trim().to_string()]);
        }
    }
    if !profile.trim().is_empty() {
        a.extend(["--driver".into(), format!("Drivers/{}.odr", profile.trim())]);
    }
    Ok(a)
}

#[derive(Serialize, Clone, Debug)]
pub struct Launched {
    pub pid: u32,
    pub log: String,
    pub command: String,
    /// Games that were running already (and keep running).
    pub others: usize,
}

/// Start a game for the duty. Any number may run at once; each writes its own log.
pub fn launch(d: &Duty) -> Result<Launched> {
    if IN_PROCESS_GAMES {
        let args = duty_args(d)?;
        let command = args.join(" ");
        log_to_file(&format!("game in this process: {command}"));
        *IN_PROCESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(args);
        return Ok(Launched {
            pid: std::process::id(),
            log: data_dir().join("game.log").to_string_lossy().to_string(),
            command,
            others: 0,
        });
    }
    let c = load_config();
    let game = find_game(&c.game).context("the game binary was not found (set it under Setup)")?;
    let args = duty_args(d)?;
    let profile = d
        .profile
        .clone()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or(c.profile);
    let s = instances::start(&game, &args, d, &profile)?;
    Ok(Launched {
        pid: s.pid,
        log: s.log.to_string_lossy().to_string(),
        command: s.command,
        others: s.others,
    })
}

/// What a LAN join field means (or what is wrong with it), and the sessions hosted here.
pub fn check_join(text: &str) -> Value {
    match omsi_net::describe_join(text) {
        Ok(d) => json!({ "ok": true, "text": i18n::tr(&d), "local": instances::local_hosts() }),
        Err(e) => json!({ "ok": false, "text": i18n::tr(&e), "local": instances::local_hosts() }),
    }
}

// ---------------------------------------------------------------------------------------
// small services for the window

/// A line into ~/.openomsi/launcher.log.
pub fn log_to_file(line: &str) {
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

/// What interrupted installs left behind (and the old unzip folder): cleaned up once when
/// the launcher opens.
pub fn cleanup() {
    for line in install::cleanup_stale(&data_dir(), content_dir().as_deref()) {
        log_to_file(&format!("cleanup: {line}"));
    }
}

/// A phone runs one program: the launcher hands the game's command line over here and the
/// same process plays it in the same window (see the app's `android.rs`) instead of starting
/// another process.
static IN_PROCESS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

/// The command line of a game the launcher asked for (taken once).
pub fn take_in_process_launch() -> Option<Vec<String>> {
    IN_PROCESS.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Whether games run inside the launcher's own process (a phone, an iPhone, an iPad: an app
/// there may not start programs).
pub const IN_PROCESS_GAMES: bool = cfg!(any(target_os = "android", target_os = "ios"));

pub use instances::{Instance, list as list_instances, log_tail, stop as stop_instance};

/// Terminal access to the same functions: `--cli lines '{"map":"maps/Grundorf/global.cfg"}'`.
pub fn cli(cmd: &str, arg: &str) -> Result<Value> {
    let a: Value = serde_json::from_str(arg).unwrap_or(json!({}));
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Ok(match cmd {
        "config" => serde_json::to_value(load_config())?,
        "maps" => serde_json::to_value(list_maps()?)?,
        "vehicles" => serde_json::to_value(list_vehicles()?)?,
        "weather" => serde_json::to_value(list_weather()?)?,
        "lines" => serde_json::to_value(list_lines(&s("map"), &s("date"))?)?,
        "ai_vehicles" => serde_json::to_value(ai_lists::vehicles(false)?)?,
        "ai_lists" => ai_lists::view(&s("map"), &s("date"))?,
        "ai_lists_save" => ai_lists::save(
            &s("map"),
            &s("date"),
            &serde_json::from_value::<Vec<ai_lists::GroupJson>>(
                a.get("groups").cloned().unwrap_or_default(),
            )?,
            &serde_json::from_value::<Vec<ai_lists::RuleJson>>(
                a.get("rules").cloned().unwrap_or_default(),
            )?,
        )?,
        "ai_lists_reset" => {
            ai_lists::reset(&s("map"))?;
            json!({})
        }
        "ibis" => serde_json::to_value(ibis_info(
            &s("bus"),
            &s("hof"),
            &s("line"),
            &s("terminus"),
            &serde_json::from_value::<Vec<String>>(a.get("stops").cloned().unwrap_or_default())
                .unwrap_or_default(),
        )?)?,
        "profiles" => serde_json::to_value(list_profiles()?)?,
        "profile" => serde_json::to_value(get_profile(&s("name"))?)?,
        "mods" => {
            // the inbox is installed right away here (there is no page to follow it)
            let done = install_inbox_blocking();
            let mut v = serde_json::to_value(mods_status()?)?;
            v["installed_now"] = serde_json::to_value(done)?;
            v
        }
        "install" => {
            let cancel = a.get("cancel_after_ms").and_then(|v| v.as_u64());
            let mode = s("mode");
            let p = install_mod_blocking(
                Path::new(&s("path")),
                if mode.is_empty() {
                    "auto"
                } else {
                    mode.as_str()
                },
                cancel,
            )?;
            if p.state == "failed" {
                return Err(anyhow!("{}", p.message));
            }
            serde_json::to_value(p)?
        }
        "modinfo" => serde_json::to_value(inspect_mod(Path::new(&s("path")))?)?,
        "stamp" => json!(poll()?.stamp),
        "poll" => {
            // {"watch": seconds}: keep polling like the page does (the inbox watcher needs
            // two looks), then wait for the installs it started
            let watch = a.get("watch").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let t0 = std::time::Instant::now();
            let mut stamps = vec![poll()?.stamp];
            let mut started = Vec::new();
            while t0.elapsed().as_secs_f64() < watch
                || install::jobs().iter().any(|j| j.finished.is_none())
            {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                let p = poll()?;
                started.extend(p.started);
                if stamps.last() != Some(&p.stamp) {
                    stamps.push(p.stamp);
                }
            }
            let mut v = serde_json::to_value(poll()?)?;
            v["started"] = json!(started);
            v["stamps"] = json!(stamps);
            v
        }
        "instances" => serde_json::to_value(instances::list())?,
        "stop" => {
            let by_itself =
                instances::stop(a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32)?;
            json!({ "stopped": true, "ended_by_itself": by_itself })
        }
        "log" => json!(instances::log_tail(
            a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            a.get("lines").and_then(|v| v.as_u64()).unwrap_or(40) as usize
        )?),
        "join" => check_join(&s("text")),
        "settings" => get_settings()?,
        // what the page's Save does: `--cli save_settings '{"view_distance":"1500"}'` (the
        // other values from the file as it is)
        "save_settings" => {
            let mut v = get_settings()?;
            if let (Some(v), Some(changes)) = (v.as_object_mut(), a.as_object()) {
                v.extend(changes.clone());
            }
            save_settings(&v)?;
            get_settings()?
        }
        "keybindings" => get_keybindings()?,
        // `--cli save_keybindings '{"game":[...],"vehicles":[...]}'`: the whole list, as
        // `keybindings` returns it - a partial update reads the current file first
        "save_keybindings" => {
            save_keybindings(&a)?;
            get_keybindings()?
        }
        "preview" => json!(bus_preview(&s("bus"), &s("paint"))?),
        "args" => json!(duty_args(&serde_json::from_value(a.clone())?)?),
        "launch" => serde_json::to_value(launch(&serde_json::from_value(a.clone())?)?)?,
        _ => return Err(anyhow!("unknown command {cmd}")),
    })
}

#[cfg(test)]
mod tests;

/// The machine's local date and time: (year, month, day, hour, minute).
#[cfg(unix)]
pub fn local_now() -> Option<(i32, i32, i32, i32, i32)> {
    // SAFETY: time and localtime_r only write the struct handed to them
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some((
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
        ))
    }
}

#[cfg(windows)]
pub fn local_now() -> Option<(i32, i32, i32, i32, i32)> {
    // SAFETY: GetLocalTime only fills the struct handed to it
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    Some((
        t.wYear as i32,
        t.wMonth as i32,
        t.wDay as i32,
        t.wHour as i32,
        t.wMinute as i32,
    ))
}

#[cfg(not(any(unix, windows)))]
pub fn local_now() -> Option<(i32, i32, i32, i32, i32)> {
    None
}

#[cfg(test)]
mod omsi_options_tests;

#[cfg(test)]
mod head_tracking_tests;
#[cfg(test)]
mod right_stick_look_tests;

#[cfg(test)]
#[path = "passenger_connections_tests.rs"]
mod passenger_connections_tests;

#[cfg(test)]
#[path = "passenger_settings_tests.rs"]
mod passenger_settings_tests;
