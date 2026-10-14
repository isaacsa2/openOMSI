//! A mod archive chosen with the phone's file dialog.
//!
//! Android's dialog (the storage access framework) hands over a stream, not a path, and the
//! file may live in a cloud drive or in another app. The archive is copied into the Mods
//! folder - the inbox the launcher already installs from, where a `.zip` used in place is
//! *moved* into the content and so stays - and the copy's path goes on to the installer like
//! one picked on a computer. Copying is done here, not in the Java side, so that the checks
//! (free space, a copy cut short, cancelling) are the same on every system and are tested.

use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Kept free besides the archive itself (the install that follows checks its own needs).
const MARGIN: u64 = 64 << 20;
/// Needed when the dialog does not tell the size (a cloud file): a write that fails for lack
/// of space is still caught and the partial copy removed.
const UNKNOWN_SIZE_NEEDS: u64 = 256 << 20;
const CHUNK: usize = 1 << 20;
const ARCHIVES: [&str; 3] = ["zip", "7z", "rar"];
const MAX_NAME: usize = 120;

/// Why a copy did not give a file; a cancel is not a failure to tell the player about.
#[derive(Debug)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}
impl std::error::Error for Cancelled {}

/// Set by [`cancel_import`] while the copy of [`import_picked`] is under way.
static CANCEL: AtomicBool = AtomicBool::new(false);

/// Stop the copy of [`import_picked`] that is under way (the page's "cancel").
pub fn cancel_import() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// Copy the archive `from` (the dialog's stream) named `name` of `size` bytes (when known) into
/// the content folder's Mods folder and give the new file's path, or `None` when the player
/// cancelled. `progress` is told the bytes copied so far, a few times a second.
pub fn import_picked(name: &str, size: Option<u64>, from: &mut dyn Read, progress: &mut dyn FnMut(u64)) -> Result<Option<PathBuf>> {
    let content = crate::content_dir().ok_or_else(|| anyhow!("no game binary configured, so no content folder"))?;
    picked_into(&content, name, size, from, progress)
}

fn picked_into(content: &Path, name: &str, size: Option<u64>, from: &mut dyn Read, progress: &mut dyn FnMut(u64)) -> Result<Option<PathBuf>> {
    CANCEL.store(false, Ordering::Relaxed);
    // (the page needs a few updates a second, not one per megabyte)
    let mut last: Option<std::time::Instant> = None;
    let mut throttled = |done: u64| {
        if last.is_none_or(|t| t.elapsed() >= Duration::from_millis(250)) {
            last = Some(std::time::Instant::now());
            progress(done);
        }
    };
    match import_into(content, name, size, from, &CANCEL, &mut throttled) {
        Ok(path) => Ok(Some(path)),
        Err(e) if e.downcast_ref::<Cancelled>().is_some() => Ok(None),
        Err(e) => Err(e),
    }
}

fn import_into(content: &Path, name: &str, size: Option<u64>, from: &mut dyn Read, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf> {
    let name = archive_name(name)?;
    let inbox = content.join("Mods");
    std::fs::create_dir_all(&inbox).with_context(|| format!("creating {}", inbox.display()))?;
    let needed = size.map_or(UNKNOWN_SIZE_NEEDS, |s| s.saturating_add(MARGIN));
    if let Some(free) = crate::install::free_space(&inbox) {
        if free < needed {
            return Err(anyhow!("not enough room to copy {name}: it needs {} MB and only {} MB are free where the Mods folder is. Free some space and choose it again.", needed.div_ceil(1 << 20), free >> 20));
        }
    }
    let dest = free_name(&inbox, &name);
    // dot first: the inbox watcher does not look at it until it is whole and renamed
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let part = inbox.join(format!(".import-{}-{}.part", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    let result = copy(&part, from, size, cancel, progress).and_then(|()| std::fs::rename(&part, &dest).with_context(|| format!("putting the copy in place as {}", dest.display())));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result.map(|()| dest)
}

fn copy(part: &Path, from: &mut dyn Read, size: Option<u64>, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<()> {
    let mut out = std::fs::File::create(part).with_context(|| format!("creating {}", part.display()))?;
    let mut buf = vec![0u8; CHUNK];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow!(Cancelled));
        }
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(anyhow!("reading the chosen file stopped after {} MB: {e}", done >> 20)),
        };
        out.write_all(&buf[..n]).map_err(|e| anyhow!("writing the copy stopped after {} MB: {e}", done >> 20))?;
        done += n as u64;
        progress(done);
    }
    out.flush()?;
    out.sync_all().ok();
    // a stream cut short (a cloud file whose download broke) would end like a whole one
    if let Some(size) = size {
        if done != size {
            return Err(anyhow!("the copy is incomplete: {done} of {size} bytes arrived. Nothing was kept; try choosing the file again."));
        }
    }
    if done == 0 {
        return Err(anyhow!("the chosen file is empty"));
    }
    Ok(())
}

/// The file name as the dialog gave it, made safe to be a name in the Mods folder: no folders,
/// nothing a file system refuses, and one of the archive kinds the installer reads.
fn archive_name(name: &str) -> Result<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let mut clean: String = base.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
    clean = clean.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).to_string();
    let ext = Path::new(&clean).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !ARCHIVES.contains(&ext.as_str()) {
        return Err(anyhow!("{} is not a .zip, .7z or .rar archive", if base.is_empty() { "the chosen file" } else { base }));
    }
    if clean.chars().count() > MAX_NAME {
        // (cut in the stem, the extension stays)
        let stem: String = Path::new(&clean).file_stem().and_then(|s| s.to_str()).unwrap_or("mod").chars().take(MAX_NAME - ext.len() - 1).collect();
        clean = format!("{stem}.{ext}");
    }
    Ok(clean)
}

/// `name`, or `name (2).zip`, `name (3).zip` ... when the Mods folder has that already (an
/// earlier choice still waiting): nothing there is overwritten.
fn free_name(inbox: &Path, name: &str) -> PathBuf {
    let first = inbox.join(name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("mod");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("zip");
    (2..).map(|n| inbox.join(format!("{stem} ({n}).{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omsi-import-{name}-{} x", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn import(content: &Path, name: &str, data: &[u8], size: Option<u64>) -> Result<PathBuf> {
        import_into(content, name, size, &mut Cursor::new(data.to_vec()), &AtomicBool::new(false), &mut |_| {})
    }

    fn leftovers(content: &Path) -> Vec<String> {
        std::fs::read_dir(content.join("Mods")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with('.')).collect()
    }

    #[test]
    fn an_archive_is_copied_into_the_mods_folder_whole() {
        let c = dir("whole");
        let data: Vec<u8> = (0..3_500_000u32).map(|i| (i % 251) as u8).collect();
        let mut seen = Vec::new();
        let path = import_into(&c, "My Bus.zip", Some(data.len() as u64), &mut Cursor::new(data.clone()), &AtomicBool::new(false), &mut |n| seen.push(n)).unwrap();
        assert_eq!(path, c.join("Mods").join("My Bus.zip"));
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(*seen.last().unwrap(), data.len() as u64);
        assert!(seen.windows(2).all(|w| w[0] < w[1]) && seen.len() > 1, "progress after each piece: {seen:?}");
        assert!(leftovers(&c).is_empty());
        let _ = std::fs::remove_dir_all(&c);
    }

    #[test]
    fn a_copy_cut_short_is_refused_and_leaves_nothing() {
        let c = dir("short");
        let e = import(&c, "map.zip", &[1u8; 1000], Some(5000)).unwrap_err().to_string();
        assert!(e.contains("incomplete") && e.contains("1000 of 5000"), "{e}");
        assert!(!c.join("Mods").join("map.zip").exists());
        assert!(leftovers(&c).is_empty());
        // a stream that fails half way
        struct Breaks(usize);
        impl Read for Breaks {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                if self.0 == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "drive went away"));
                }
                self.0 -= 1;
                b[..10].fill(7);
                Ok(10)
            }
        }
        let e = import_into(&c, "map.zip", None, &mut Breaks(3), &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string();
        assert!(e.contains("drive went away"), "{e}");
        assert!(!c.join("Mods").join("map.zip").exists() && leftovers(&c).is_empty());
        let _ = std::fs::remove_dir_all(&c);
    }

    #[test]
    fn cancelling_stops_the_copy_and_removes_the_part() {
        let c = dir("cancel");
        let cancel = AtomicBool::new(false);
        let data = vec![9u8; 5 << 20];
        let e = import_into(&c, "big.7z", Some(data.len() as u64), &mut Cursor::new(data), &cancel, &mut |n| {
            if n >= 2 << 20 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
        assert!(e.downcast_ref::<Cancelled>().is_some(), "{e}");
        assert!(!c.join("Mods").join("big.7z").exists() && leftovers(&c).is_empty());
        let _ = std::fs::remove_dir_all(&c);
    }

    #[test]
    fn the_page_cancels_a_copy_and_hears_a_few_updates_not_one_per_piece() {
        let c = dir("picked");
        let data = vec![3u8; 6 << 20];
        let mut told = Vec::new();
        let done = picked_into(&c, "a.zip", Some(data.len() as u64), &mut Cursor::new(data.clone()), &mut |n| told.push(n)).unwrap();
        assert_eq!(done, Some(c.join("Mods").join("a.zip")));
        assert!(!told.is_empty() && told.len() < 6, "{told:?}");
        // cancelled from the callback, as the page does: nothing is kept, and it is no error
        let out = picked_into(&c, "b.zip", Some(data.len() as u64), &mut Cursor::new(data), &mut |_| cancel_import()).unwrap();
        assert_eq!(out, None);
        assert!(!c.join("Mods").join("b.zip").exists() && leftovers(&c).is_empty());
        let _ = std::fs::remove_dir_all(&c);
    }

    #[test]
    fn what_is_not_an_archive_or_empty_is_refused() {
        let c = dir("refuse");
        for name in ["notes.txt", "", "zip", "bus.zip.exe", ".zip"] {
            assert!(import(&c, name, b"x", Some(1)).is_err(), "{name:?}");
        }
        assert!(import(&c, "empty.zip", b"", Some(0)).unwrap_err().to_string().contains("empty"));
        assert!(leftovers(&c).is_empty());
        let _ = std::fs::remove_dir_all(&c);
    }

    #[test]
    fn the_name_cannot_leave_the_mods_folder_and_nothing_is_overwritten() {
        let c = dir("names");
        let p = import(&c, "../../etc/evil..\\x:y.ZIP", b"x", Some(1)).unwrap();
        assert_eq!(p.parent().unwrap(), c.join("Mods"));
        assert_eq!(p.file_name().unwrap(), "x_y.ZIP");
        let a = import(&c, "bus.zip", b"1", Some(1)).unwrap();
        let b = import(&c, "bus.zip", b"2", Some(1)).unwrap();
        let d = import(&c, "bus.zip", b"3", Some(1)).unwrap();
        assert_eq!([a.file_name().unwrap(), b.file_name().unwrap(), d.file_name().unwrap()], ["bus.zip", "bus (2).zip", "bus (3).zip"]);
        assert_eq!(std::fs::read(&a).unwrap(), b"1");
        let long = format!("{}.rar", "a".repeat(300));
        let p = import(&c, &long, b"x", None).unwrap();
        assert!(p.file_name().unwrap().len() <= MAX_NAME && p.extension().unwrap() == "rar");
        let _ = std::fs::remove_dir_all(&c);
    }

    #[test]
    fn too_little_free_space_is_said_before_copying() {
        let c = dir("space");
        let free = crate::install::free_space(&c).expect("a free-space reading");
        let e = import(&c, "huge.zip", b"x", Some(free.saturating_add(1 << 30))).unwrap_err().to_string();
        assert!(e.contains("not enough room"), "{e}");
        assert!(leftovers(&c).is_empty() && !c.join("Mods").join("huge.zip").exists());
        let _ = std::fs::remove_dir_all(&c);
    }
}
