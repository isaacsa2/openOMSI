//! Optional, lossless on-disk compression for legacy OMSI content.
//!
//! This is deliberately an opt-in storage operation, not a renderer setting. The normal
//! installation remains untouched until the player asks for it. Supported assets are
//! replaced by self-describing `.omc` sidecars that the VFS reads transparently. Restoring
//! writes the original bytes back and removes the sidecars.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MIN_FILE_BYTES: u64 = 64 * 1024;
pub const MIN_SAVING_PERCENT: u8 = 10;
const MANIFEST: &str = ".openomsi-compression.jsonl";
const SAMPLE_FILES_PER_KIND: usize = 48;
const SAMPLE_BYTES_PER_FILE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Analyze,
    Compress,
    Restore,
}

#[derive(Debug, Clone, Default)]
pub struct KindReport {
    pub name: String,
    pub files: u64,
    pub original_bytes: u64,
    pub stored_bytes: u64,
    pub estimated_stored_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct StorageReport {
    pub active: bool,
    pub compressed_files: u64,
    pub current_original_bytes: u64,
    pub current_stored_bytes: u64,
    pub eligible_files: u64,
    pub eligible_bytes: u64,
    pub estimated_stored_bytes: u64,
    pub skipped_small: u64,
    pub skipped_ratio: u64,
    pub restore_required_bytes: u64,
    pub free_bytes: Option<u64>,
    pub kinds: Vec<KindReport>,
}

impl StorageReport {
    pub fn current_saved_bytes(&self) -> u64 {
        self.current_original_bytes.saturating_sub(self.current_stored_bytes)
    }

    pub fn estimated_saved_bytes(&self) -> u64 {
        self.eligible_bytes.saturating_sub(self.estimated_stored_bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestEntry {
    version: u32,
    path: String,
    kind: String,
    original_bytes: u64,
    stored_bytes: u64,
    crc32: u32,
}

#[derive(Default)]
struct KindWork {
    files: u64,
    original: u64,
    stored: u64,
    raw_bytes: u64,
    sample_in: u64,
    sample_out: u64,
    samples: usize,
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join(MANIFEST)
}

fn load_manifest(root: &Path) -> BTreeMap<String, ManifestEntry> {
    let Ok(text) = std::fs::read_to_string(manifest_path(root)) else { return BTreeMap::new() };
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let Ok(e) = serde_json::from_str::<ManifestEntry>(line) else { continue };
        if e.version == 1 && !e.path.is_empty() {
            out.insert(e.path.clone(), e);
        }
    }
    out
}

fn append_manifest(root: &Path, entry: &ManifestEntry) -> Result<()> {
    let p = manifest_path(root);
    let mut f = OpenOptions::new().create(true).append(true).open(&p)
        .with_context(|| format!("opening {}", p.display()))?;
    serde_json::to_writer(&mut f, entry)?;
    f.write_all(b"\n")?;
    f.flush()?;
    Ok(())
}

fn rewrite_manifest(root: &Path, entries: &BTreeMap<String, ManifestEntry>) -> Result<()> {
    let path = manifest_path(root);
    if entries.is_empty() {
        let _ = std::fs::remove_file(path);
        return Ok(());
    }
    let tmp = root.join(".openomsi-compression.jsonl.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    for e in entries.values() {
        serde_json::to_writer(&mut f, e)?;
        f.write_all(b"\n")?;
    }
    f.flush()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
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

/// Only content areas whose audio/models/textures are loaded through the VFS are scanned.
/// In particular Plugins is excluded: a native plugin may open its own resources directly.
fn files(root: &Path) -> Vec<PathBuf> {
    const AREAS: [&str; 9] = [
        "Vehicles",
        "Sceneryobjects",
        "Splines",
        "Texture",
        "Humans",
        "Announcements",
        "Sounds",
        "maps",
        "Trains",
    ];
    let mut out = Vec::new();
    for folder in AREAS {
        walk(&root.join(folder), &mut out);
    }
    out
}

fn dds_is_raw(path: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut h = [0u8; 128];
    if f.read_exact(&mut h).is_err() || &h[..4] != b"DDS " {
        return false;
    }
    // DDS_PIXELFORMAT.dwFlags is at byte 80. DDPF_FOURCC means DXT/BC or another encoded
    // payload; leave every such DDS alone. Plain RGB/RGBA/luminance DDS has no FOURCC and
    // is worth testing with the lossless compressor.
    let flags = u32::from_le_bytes([h[80], h[81], h[82], h[83]]);
    flags & 0x4 == 0
}

fn kind_of(path: &Path, sidecar: bool) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "wav" => Some("WAV audio"),
        "o3d" => Some("O3D models"),
        "x" => Some("X models"),
        "bmp" => Some("BMP textures"),
        "tga" => Some("TGA textures"),
        // A DDS sidecar could only have been created after this check succeeded. Do not
        // decompress it merely to classify it during a later scan.
        "dds" if sidecar || dds_is_raw(path) => Some("DDS textures (raw)"),
        _ => None,
    }
}

fn sample_compressed(path: &Path) -> Option<(u64, u64)> {
    let f = std::fs::File::open(path).ok()?;
    let mut data = Vec::new();
    f.take(SAMPLE_BYTES_PER_FILE).read_to_end(&mut data).ok()?;
    if data.is_empty() {
        return None;
    }
    let stored = omsi_cfg::vfs::trial_compressed_size(&data).ok()?;
    Some((data.len() as u64, stored))
}

fn estimate_stored(w: &KindWork) -> u64 {
    if w.sample_in == 0 {
        return w.stored + w.raw_bytes;
    }
    let ratio = (w.sample_out as f64 / w.sample_in as f64).clamp(0.0, 1.0);
    // Files that are unlikely to cross the real 10% threshold stay raw.
    let ratio = if ratio >= (100 - MIN_SAVING_PERCENT as u64) as f64 / 100.0 { 1.0 } else { ratio };
    w.stored.saturating_add((w.raw_bytes as f64 * ratio).round() as u64)
}

fn report_from_work(work: &BTreeMap<String, KindWork>, skipped_small: u64, skipped_ratio: u64) -> StorageReport {
    let mut r = StorageReport { skipped_small, skipped_ratio, ..Default::default() };
    for (name, w) in work {
        let estimated = estimate_stored(w);
        r.eligible_files += w.files;
        r.eligible_bytes += w.original;
        r.estimated_stored_bytes += estimated;
        r.current_original_bytes += w.original.saturating_sub(w.raw_bytes);
        r.current_stored_bytes += w.stored;
        let compressed = if w.stored > 0 { w.files.saturating_sub(if w.raw_bytes > 0 { 1 } else { 0 }) } else { 0 };
        let _ = compressed; // exact compressed count is filled by callers.
        r.kinds.push(KindReport {
            name: name.clone(),
            files: w.files,
            original_bytes: w.original,
            stored_bytes: w.stored,
            estimated_stored_bytes: estimated,
        });
    }
    r.active = r.current_stored_bytes > 0;
    r
}

pub fn quick_status(root: &Path) -> StorageReport {
    let entries = load_manifest(root);
    let mut r = StorageReport::default();
    let mut kinds: BTreeMap<String, KindReport> = BTreeMap::new();
    let mut max_stored = 0u64;
    for e in entries.values() {
        r.compressed_files += 1;
        r.current_original_bytes += e.original_bytes;
        r.current_stored_bytes += e.stored_bytes;
        max_stored = max_stored.max(e.stored_bytes);
        let k = kinds.entry(e.kind.clone()).or_insert_with(|| KindReport { name: e.kind.clone(), ..Default::default() });
        k.files += 1;
        k.original_bytes += e.original_bytes;
        k.stored_bytes += e.stored_bytes;
        k.estimated_stored_bytes += e.stored_bytes;
    }
    r.active = r.compressed_files > 0;
    r.eligible_files = r.compressed_files;
    r.eligible_bytes = r.current_original_bytes;
    r.estimated_stored_bytes = r.current_stored_bytes;
    r.restore_required_bytes = r.current_original_bytes.saturating_sub(r.current_stored_bytes).saturating_add(max_stored);
    r.free_bytes = free_space(root);
    r.kinds = kinds.into_values().collect();
    r
}

pub fn execute<F>(root: &Path, mode: Mode, mut progress: F) -> Result<StorageReport>
where
    F: FnMut(String),
{
    match mode {
        Mode::Analyze => analyze(root, &mut progress),
        Mode::Compress => compress(root, &mut progress),
        Mode::Restore => restore(root, &mut progress),
    }
}

fn analyze(root: &Path, progress: &mut dyn FnMut(String)) -> Result<StorageReport> {
    let all = files(root);
    let mut work: BTreeMap<String, KindWork> = BTreeMap::new();
    let mut compressed_files = 0u64;
    let mut skipped_small = 0u64;

    progress(format!("Scanning {} content files…", all.len()));
    for (i, path) in all.iter().enumerate() {
        if i > 0 && i % 5000 == 0 {
            progress(format!("Analyzing… {i} / {}", all.len()));
        }
        if let Some(logical_path) = omsi_cfg::vfs::logical_path_of_compressed(path) {
            let Some(kind) = kind_of(&logical_path, true) else { continue };
            let Ok(info) = omsi_cfg::vfs::compressed_file_info(&logical_path) else { continue };
            let w = work.entry(kind.into()).or_default();
            w.files += 1;
            w.original += info.original_bytes;
            w.stored += info.stored_bytes;
            compressed_files += 1;
            continue;
        }

        let Some(kind) = kind_of(path, false) else { continue };
        let Ok(meta) = std::fs::metadata(path) else { continue };
        if meta.len() < MIN_FILE_BYTES {
            skipped_small += 1;
            continue;
        }
        let w = work.entry(kind.into()).or_default();
        w.files += 1;
        w.original += meta.len();
        w.raw_bytes += meta.len();
        if w.samples < SAMPLE_FILES_PER_KIND {
            if let Some((input, output)) = sample_compressed(path) {
                w.sample_in += input;
                w.sample_out += output;
                w.samples += 1;
            }
        }
    }

    let mut r = report_from_work(&work, skipped_small, 0);
    r.compressed_files = compressed_files;
    r.active = compressed_files > 0;
    r.free_bytes = free_space(root);
    let mut max_stored = 0u64;
    for path in all {
        if let Some(logical_path) = omsi_cfg::vfs::logical_path_of_compressed(&path) {
            if let Ok(info) = omsi_cfg::vfs::compressed_file_info(&logical_path) {
                max_stored = max_stored.max(info.stored_bytes);
            }
        }
    }
    r.restore_required_bytes = r.current_original_bytes.saturating_sub(r.current_stored_bytes).saturating_add(max_stored);
    Ok(r)
}

fn compress(root: &Path, progress: &mut dyn FnMut(String)) -> Result<StorageReport> {
    let all = files(root);
    let mut manifest: BTreeMap<String, ManifestEntry> = BTreeMap::new();
    let mut work: BTreeMap<String, KindWork> = BTreeMap::new();
    let mut skipped_small = 0u64;
    let mut skipped_ratio = 0u64;
    let mut changed = 0u64;

    progress(format!("Compressing supported assets… {} files to scan", all.len()));
    for (i, path) in all.iter().enumerate() {
        if i > 0 && i % 1000 == 0 {
            progress(format!("Compressing… {i} / {} · {changed} files changed", all.len()));
        }

        if let Some(logical_path) = omsi_cfg::vfs::logical_path_of_compressed(path) {
            let Some(kind) = kind_of(&logical_path, true) else { continue };
            let Ok(info) = omsi_cfg::vfs::compressed_file_info(&logical_path) else { continue };
            let key = rel(root, &logical_path);
            manifest.insert(key.clone(), ManifestEntry {
                version: 1,
                path: key,
                kind: kind.into(),
                original_bytes: info.original_bytes,
                stored_bytes: info.stored_bytes,
                crc32: info.crc32,
            });
            let w = work.entry(kind.into()).or_default();
            w.files += 1;
            w.original += info.original_bytes;
            w.stored += info.stored_bytes;
            continue;
        }

        let Some(kind) = kind_of(path, false) else { continue };
        let Ok(meta) = std::fs::metadata(path) else { continue };
        if meta.len() < MIN_FILE_BYTES {
            skipped_small += 1;
            continue;
        }

        let w = work.entry(kind.into()).or_default();
        w.files += 1;
        w.original += meta.len();

        let result = omsi_cfg::vfs::compress_file(path, MIN_SAVING_PERCENT)
            .with_context(|| format!("compressing {}", path.display()))?;
        if !result.compressed {
            skipped_ratio += 1;
            w.raw_bytes += result.original_bytes;
            continue;
        }

        changed += 1;
        let info = omsi_cfg::vfs::compressed_file_info(path)?;
        w.stored += info.stored_bytes;
        let key = rel(root, path);
        let entry = ManifestEntry {
            version: 1,
            path: key.clone(),
            kind: kind.into(),
            original_bytes: info.original_bytes,
            stored_bytes: info.stored_bytes,
            crc32: info.crc32,
        };
        manifest.insert(key, entry.clone());
        // Append immediately. If the application is closed mid-operation, the already
        // compressed files are still recorded and can be restored.
        append_manifest(root, &entry)?;
    }

    rewrite_manifest(root, &manifest)?;
    let mut r = report_from_work(&work, skipped_small, skipped_ratio);
    r.compressed_files = manifest.len() as u64;
    r.active = !manifest.is_empty();
    r.current_original_bytes = manifest.values().map(|e| e.original_bytes).sum();
    r.current_stored_bytes = manifest.values().map(|e| e.stored_bytes).sum();
    r.eligible_files = work.values().map(|w| w.files).sum();
    r.eligible_bytes = work.values().map(|w| w.original).sum();
    // After applying, "estimated" is the actual stored size for compressed assets plus
    // raw files that failed the threshold.
    r.estimated_stored_bytes = work.values().map(|w| w.stored + w.raw_bytes).sum();
    let max_stored = manifest.values().map(|e| e.stored_bytes).max().unwrap_or(0);
    r.restore_required_bytes = r.current_original_bytes.saturating_sub(r.current_stored_bytes).saturating_add(max_stored);
    r.free_bytes = free_space(root);
    progress(format!("Compression complete: {changed} files changed."));
    Ok(r)
}

fn restore(root: &Path, progress: &mut dyn FnMut(String)) -> Result<StorageReport> {
    let all = files(root);
    let mut sidecars: Vec<(PathBuf, omsi_cfg::vfs::CompressedFileInfo)> = Vec::new();
    for path in &all {
        let Some(logical_path) = omsi_cfg::vfs::logical_path_of_compressed(path) else { continue };
        if let Ok(info) = omsi_cfg::vfs::compressed_file_info(&logical_path) {
            sidecars.push((logical_path, info));
        }
    }
    if sidecars.is_empty() {
        let _ = std::fs::remove_file(manifest_path(root));
        return Ok(StorageReport { free_bytes: free_space(root), ..Default::default() });
    }

    let original: u64 = sidecars.iter().map(|(_, i)| i.original_bytes).sum();
    let stored: u64 = sidecars.iter().map(|(_, i)| i.stored_bytes).sum();
    let max_stored = sidecars.iter().map(|(_, i)| i.stored_bytes).max().unwrap_or(0);
    let required = original.saturating_sub(stored).saturating_add(max_stored);
    if let Some(free) = free_space(root) {
        if free < required {
            return Err(anyhow!(
                "not enough free space to restore: need about {:.1} GiB, only {:.1} GiB is available",
                required as f64 / 1073741824.0,
                free as f64 / 1073741824.0
            ));
        }
    }

    progress(format!("Restoring {} compressed assets…", sidecars.len()));
    let mut restored = 0u64;
    for (i, (logical_path, _)) in sidecars.iter().enumerate() {
        if i > 0 && i % 1000 == 0 {
            progress(format!("Restoring… {i} / {} · {restored} restored", sidecars.len()));
        }
        if omsi_cfg::vfs::restore_compressed_file(logical_path)
            .with_context(|| format!("restoring {}", logical_path.display()))?
        {
            restored += 1;
        }
    }
    let _ = std::fs::remove_file(manifest_path(root));
    progress(format!("Restore complete: {restored} files."));
    Ok(StorageReport { free_bytes: free_space(root), ..Default::default() })
}

#[cfg(unix)]
fn free_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some((s.f_bavail as u64).saturating_mul(s.f_frsize as u64))
}

#[cfg(windows)]
fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut free), None, None).ok()?;
    }
    Some(free)
}

#[cfg(not(any(unix, windows)))]
fn free_space(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_dds_is_selected_but_fourcc_dds_is_not() {
        let dir = std::env::temp_dir().join(format!("openomsi-dds-compress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("raw.dds");
        let bc = dir.join("bc.dds");
        let mut h = vec![0u8; 128];
        h[..4].copy_from_slice(b"DDS ");
        h[80..84].copy_from_slice(&0x41u32.to_le_bytes()); // RGB + alpha
        std::fs::write(&raw, &h).unwrap();
        h[80..84].copy_from_slice(&0x4u32.to_le_bytes()); // FOURCC
        h[84..88].copy_from_slice(b"DXT5");
        std::fs::write(&bc, &h).unwrap();
        assert_eq!(kind_of(&raw, false), Some("DDS textures (raw)"));
        assert_eq!(kind_of(&bc, false), None);
        std::fs::remove_dir_all(dir).ok();
    }
}
