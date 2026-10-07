//! Experimental in-place compression of the large legacy assets that dominate OMSI installs.
//!
//! This deliberately starts narrow: PCM WAV sounds and O3D meshes under Vehicles and
//! Sceneryobjects. Small files and files that do not save at least 10% stay untouched.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const MIN_FILE_BYTES: u64 = 64 * 1024;
const MIN_SAVING_PERCENT: u8 = 10;
const ROOTS: [&str; 2] = ["Vehicles", "Sceneryobjects"];

#[derive(Default)]
struct Stats {
    candidates: u64,
    changed: u64,
    skipped_small: u64,
    skipped_ratio: u64,
    original_bytes: u64,
    stored_bytes: u64,
}

fn candidate(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wav") || e.eq_ignore_ascii_case("o3d"))
        .unwrap_or(false)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            continue;
        }
        if t.is_dir() {
            walk(&p, out);
        } else if t.is_file() {
            out.push(p);
        }
    }
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for name in ROOTS {
        walk(&root.join(name), &mut out);
    }
    out
}

fn mib(n: u64) -> f64 {
    n as f64 / (1024.0 * 1024.0)
}

/// Compress WAV/O3D assets in place, or restore those already compressed by this experiment.
pub(crate) fn run(root: &Path, restore: bool) -> Result<()> {
    if restore {
        restore_all(root)
    } else {
        compress_all(root)
    }
}

fn compress_all(root: &Path) -> Result<()> {
    let mut stats = Stats::default();
    let all = files(root);
    println!(
        "openOMSI asset compression experiment: scanning {} files under Vehicles/Sceneryobjects",
        all.len()
    );

    for path in all {
        if !candidate(&path) {
            continue;
        }
        stats.candidates += 1;
        let len = match std::fs::metadata(&path) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        if len < MIN_FILE_BYTES {
            stats.skipped_small += 1;
            continue;
        }

        let r = omsi_cfg::vfs::compress_file(&path, MIN_SAVING_PERCENT)
            .with_context(|| format!("compressing {}", path.display()))?;
        stats.original_bytes += r.original_bytes;
        stats.stored_bytes += r.stored_bytes;
        if r.compressed {
            stats.changed += 1;
            println!(
                "compressed: {} ({:.1} MiB -> {:.1} MiB)",
                path.display(),
                mib(r.original_bytes),
                mib(r.stored_bytes)
            );
        } else {
            stats.skipped_ratio += 1;
        }
    }

    let saved = stats.original_bytes.saturating_sub(stats.stored_bytes);
    let pct = if stats.original_bytes == 0 {
        0.0
    } else {
        saved as f64 * 100.0 / stats.original_bytes as f64
    };
    println!(
        "done: {} compressed / {} candidates; saved {:.1} MiB ({:.1}%). {} small and {} low-gain files left raw.",
        stats.changed,
        stats.candidates,
        mib(saved),
        pct,
        stats.skipped_small,
        stats.skipped_ratio
    );
    println!("Use --restore-compressed-assets with the same --root to restore the original files.");
    Ok(())
}

fn restore_all(root: &Path) -> Result<()> {
    let mut restored = 0u64;
    let mut all = files(root);

    // `files()` sees logical names through the VFS only when called through the VFS; this
    // scan is physical, so collect .omc sidecars explicitly as well.
    all.clear();
    for name in ROOTS {
        walk(&root.join(name), &mut all);
    }

    for sidecar in all {
        let Some(logical) = omsi_cfg::vfs::logical_path_of_compressed(&sidecar) else { continue };
        if !candidate(&logical) {
            continue;
        }
        if omsi_cfg::vfs::restore_compressed_file(&logical)
            .with_context(|| format!("restoring {}", logical.display()))?
        {
            restored += 1;
            println!("restored: {}", logical.display());
        }
    }
    println!("done: restored {restored} compressed assets.");
    Ok(())
}
