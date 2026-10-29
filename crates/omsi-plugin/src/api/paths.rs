//! The paths a plugin names: always relative to one of its own folders, never out of it.

use std::path::{Component, Path, PathBuf};

/// Longest path a plugin may give.
const MAX_PATH: usize = 260;

/// `rel` inside `base`: `/` or `\` separated, no `..`, no root, no drive, no device names, no
/// empty or dotted-only parts, and not out of `base` through a link (a symlink or junction a
/// downloaded vehicle or plugin brings, `secret -> ~/.ssh/id_rsa`). The error says what is
/// wrong.
pub fn inside(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let clean = clean(rel)?;
    let path = if clean.is_empty() { base.to_path_buf() } else { base.join(clean) };
    if !stays_in(base, &path) {
        return Err(format!("\"{rel}\": the path leads out of the plugin's folder"));
    }
    Ok(path)
}

/// Whether `path` (under `base` by its text) is still under `base` once its links are
/// followed: the deepest part of it that exists, resolved, against `base` resolved. (A folder
/// not made yet has no links in it.)
fn stays_in(base: &Path, path: &Path) -> bool {
    let Ok(root) = base.canonicalize() else { return true };
    let Some(have) = path.ancestors().find(|a| a.symlink_metadata().is_ok()) else { return true };
    match have.canonicalize() {
        Ok(real) => real.starts_with(&root),
        // (a link that leads nowhere: what it would make or read is not known)
        Err(_) => false,
    }
}

/// `rel` checked and written with `/` (empty: the folder itself).
pub fn clean(rel: &str) -> Result<String, String> {
    if rel.len() > MAX_PATH {
        return Err(format!("a path is at most {MAX_PATH} characters"));
    }
    if rel.contains('\0') {
        return Err("the path holds a zero character".into());
    }
    let mut parts = Vec::new();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return Err(format!("\"{rel}\": a path cannot go up (..)")),
            _ => {}
        }
        if part.contains(':') {
            return Err(format!("\"{rel}\": no drive or stream names (:)"));
        }
        if part.trim_end_matches(['.', ' ']).is_empty() || part.ends_with(['.', ' ']) {
            return Err(format!("\"{rel}\": a name cannot end in a dot or a space"));
        }
        let stem = part.split('.').next().unwrap_or("").trim_end().to_uppercase();
        if is_device(&stem) {
            return Err(format!("\"{rel}\": {stem} is a device name"));
        }
        // (a component the system reads as a root - only reachable through the checks above
        // on odd systems, but checked as the last word)
        if Path::new(part).components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(format!("\"{rel}\": not a plain name"));
        }
        parts.push(part);
    }
    if rel.starts_with(['/', '\\']) {
        return Err(format!("\"{rel}\": the path is relative to the plugin's folder"));
    }
    Ok(parts.join("/"))
}

/// A name Windows takes for a device whatever folder it is in (`COM1`-`COM9`, `LPT1`-`LPT9`
/// with the superscript digits too, `CONIN$`, `CONOUT$`).
fn is_device(stem: &str) -> bool {
    if matches!(stem, "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    let (head, tail) = stem.split_at(stem.char_indices().nth(3).map_or(stem.len(), |(i, _)| i));
    matches!(head, "COM" | "LPT") && matches!(tail, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside() {
        let base = Path::new("/data/p");
        assert_eq!(inside(base, "a/b.txt").unwrap(), base.join("a/b.txt"));
        assert_eq!(inside(base, "a\\b.txt").unwrap(), base.join("a/b.txt"));
        assert_eq!(inside(base, "./x").unwrap(), base.join("x"));
        assert_eq!(inside(base, "").unwrap(), base);
        for bad in ["../x", "a/../../x", "/etc/passwd", "\\x", "C:/x", "c:x", "a/NUL", "con.txt", "x.", "x ", "...", "a\0b", "COM5", "lpt9.log", "conin$", "CONOUT$.txt", "COM¹"] {
            assert!(inside(base, bad).is_err(), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_refused() {
        let d = std::env::temp_dir().join(format!("omsi-paths-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (base, outside) = (d.join("plugin"), d.join("secret"));
        std::fs::create_dir_all(base.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("key"), "x").unwrap();
        std::fs::write(base.join("sub/ok.txt"), "x").unwrap();
        std::os::unix::fs::symlink(outside.join("key"), base.join("key")).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("dir")).unwrap();
        std::os::unix::fs::symlink(base.join("sub/ok.txt"), base.join("near")).unwrap();
        std::os::unix::fs::symlink(d.join("nowhere"), base.join("dangling")).unwrap();
        for bad in ["key", "dir/key", "dir/new.txt", "dangling"] {
            assert!(inside(&base, bad).is_err(), "{bad}");
        }
        for good in ["sub/ok.txt", "near", "sub/new.txt", "new/dir/file", ""] {
            assert!(inside(&base, good).is_ok(), "{good}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
