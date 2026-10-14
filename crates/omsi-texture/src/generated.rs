//! Pictures the game draws itself and keeps in a folder of its own, outside the content
//! folders: the driver's paper timetable (`~/.openomsi/cache/schedules/schedule-v7-….png`).
//!
//! A vehicle names such a picture by its full path (the `file_schedule` string of a
//! `[matl_freetex]`), and a full path that is not under a content root is refused, so that a
//! mod or another player cannot make the game open a file of the computer or of a stranger's
//! share. Making the cache folder a content root would answer that, and change where every
//! other lookup starts (the main folder, how far a `..` may climb). Instead the game says which
//! folder it writes to and how the files there begin; only a regular `.png` file lying
//! directly in it, with that beginning, is let through. Nothing a mod names can match: the
//! folder holds only what the game wrote.

use parking_lot::Mutex;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// The folders the game writes pictures to, and how the names of those begin.
static FOLDERS: Mutex<Vec<(PathBuf, String)>> = Mutex::new(Vec::new());

/// Let `dir`'s `.png` files whose names begin with `prefix` be found by their full path. The
/// same folder is told again and again (each trip of a duty does), which costs nothing.
pub fn allow(dir: &Path, prefix: &str) {
    if !dir.is_absolute() || prefix.is_empty() {
        return;
    }
    let mut folders = FOLDERS.lock();
    if !folders.iter().any(|(d, p)| same_path(d, dir) && p == prefix) {
        folders.push((dir.to_path_buf(), prefix.to_string()));
    }
}

/// `path` itself when it is a picture the game wrote to a folder it said it writes to.
pub(crate) fn resolve(path: &Path) -> Option<PathBuf> {
    let (dir, name) = (path.parent()?, path.file_name()?.to_str()?);
    // (no `..` to climb out of the folder, no `name:stream` of Windows' file systems)
    if path.components().any(|c| matches!(c, Component::ParentDir)) || name.contains(':') {
        return None;
    }
    let is_png = Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if !is_png || !FOLDERS.lock().iter().any(|(d, prefix)| name.starts_with(prefix.as_str()) && same_path(d, dir)) {
        return None;
    }
    // (a link could lead anywhere; the game writes plain files)
    std::fs::symlink_metadata(path).ok().filter(|m| m.file_type().is_file()).map(|_| path.to_path_buf())
}

/// Whether two absolute paths name the same place, written the same way up to the separators
/// (and the case of the letters, where the file system does not tell them apart).
fn same_path(a: &Path, b: &Path) -> bool {
    let (mut a, mut b) = (a.components(), b.components());
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if same_component(x.as_os_str(), y.as_os_str()) => {}
            _ => return false,
        }
    }
}

#[cfg(windows)]
fn same_component(a: &OsStr, b: &OsStr) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(not(windows))]
fn same_component(a: &OsStr, b: &OsStr) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own below the system's temporary one, removed at its end. Its name
    /// has a space in it, as a Windows user's folder may (`C:\Users\Ana Maria\.openomsi`).
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Dir {
            let d = std::env::temp_dir().join(format!("omsi-generated-{name}-{} x", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn file(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"not a picture, the lookup does not read it").unwrap();
            p
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn only_what_the_game_wrote_in_the_told_folder_is_let_through() {
        let d = Dir::new("told");
        let schedules = d.0.join("cache with space").join("schedules");
        let good = d.file("cache with space/schedules/schedule-v7-00ff.png");
        assert_eq!(resolve(&good), None, "nothing is allowed before the game says so");
        allow(&schedules, "schedule-v7-");
        allow(&schedules, "schedule-v7-");
        assert_eq!(resolve(&good).as_deref(), Some(good.as_path()));
        assert_eq!(FOLDERS.lock().iter().filter(|(f, _)| f == &schedules).count(), 1);

        // another file of the same folder, another kind of file, another folder, a missing file
        for (rel, why) in [
            ("cache with space/schedules/passwords.png", "other beginning"),
            ("cache with space/schedules/schedule-v7-00ff.txt", "other kind of file"),
            ("cache with space/schedules/schedule-v7-00ff.png.exe", "other kind of file"),
            ("cache with space/schedules/sub/schedule-v7-00ff.png", "below the folder"),
            ("cache with space/other/schedule-v7-00ff.png", "other folder"),
            ("cache with space/schedules/schedule-v7-gone.png", "no such file"),
        ] {
            let p = d.0.join(rel);
            if !why.starts_with("no such") {
                d.file(rel);
            }
            assert_eq!(resolve(&p), None, "{why}: {rel}");
        }
        assert_eq!(resolve(&schedules.join("..").join("schedules").join("schedule-v7-00ff.png")), None, "climbs with ..");
        assert_eq!(resolve(&schedules.join("schedule-v7-00ff.png:stream")), None, "stream name");
        assert_eq!(resolve(Path::new("schedule-v7-00ff.png")), None, "no folder");
    }

    #[test]
    fn a_name_that_is_a_folder_is_not_a_picture() {
        let d = Dir::new("folder");
        let schedules = d.0.join("schedules");
        std::fs::create_dir_all(schedules.join("schedule-v7-aa.png")).unwrap();
        allow(&schedules, "schedule-v7-");
        assert_eq!(resolve(&schedules.join("schedule-v7-aa.png")), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_followed() {
        let d = Dir::new("link");
        let private = d.file("private/secret.png");
        let schedules = d.0.join("schedules");
        std::fs::create_dir_all(&schedules).unwrap();
        let link = schedules.join("schedule-v7-bb.png");
        std::os::unix::fs::symlink(&private, &link).unwrap();
        allow(&schedules, "schedule-v7-");
        assert_eq!(resolve(&link), None);
    }

    #[test]
    fn a_relative_or_empty_registration_allows_nothing() {
        let empty_prefix = std::env::temp_dir().join("omsi-generated-empty-prefix");
        allow(Path::new("schedules"), "schedule-v7-");
        allow(&empty_prefix, "");
        assert!(!FOLDERS.lock().iter().any(|(d, _)| d == Path::new("schedules") || d == &empty_prefix));
    }

    #[test]
    fn paths_compare_by_their_parts() {
        assert!(same_path(Path::new("/a/b c/d"), Path::new("/a/b c/d/")));
        assert!(!same_path(Path::new("/a/b"), Path::new("/a/b/c")));
        assert!(!same_path(Path::new("/a/b"), Path::new("/a/bb")));
        #[cfg(windows)]
        assert!(same_path(Path::new(r"C:\Users\Ana Maria\.openomsi"), Path::new("c:/users/ana maria/.openomsi")));
        #[cfg(not(windows))]
        assert!(!same_path(Path::new("/a/B"), Path::new("/a/b")));
    }
}
