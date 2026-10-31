//! A strictly bounded CPU cache for already-compressed, GPU-ready vehicle textures.
//! No raw pixel buffers are retained; file edits call TextureCache::release to invalidate.
use crate::gpu::TextureData;
use hashbrown::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Do not spend substantial RAM keeping a duplicate of what is in VRAM.
const RAM_LIMIT: usize = if cfg!(any(target_os = "android", target_os = "ios")) {
    8 << 20
} else {
    64 << 20
};

#[derive(Default)]
pub(super) struct WarmCache {
    entries: HashMap<PathBuf, Arc<TextureData>>,
    order: VecDeque<PathBuf>,
    bytes: usize,
}

impl WarmCache {
    pub(super) fn bytes(&self) -> usize { self.bytes }

    fn touch(&mut self, path: &Path) {
        self.order.retain(|p| p != path);
        self.order.push_back(path.to_path_buf());
    }

    pub(super) fn get(&mut self, path: &Path) -> Option<Arc<TextureData>> {
        let value = self.entries.get(path)?.clone();
        self.touch(path);
        Some(value)
    }

    pub(super) fn remove(&mut self, path: &Path) {
        if let Some(value) = self.entries.remove(path) {
            self.bytes = self.bytes.saturating_sub(value.cpu_bytes());
            self.order.retain(|p| p != path);
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }

    pub(super) fn insert(&mut self, path: PathBuf, data: Arc<TextureData>) {
        self.insert_with_limit(path, data, RAM_LIMIT);
    }

    fn insert_with_limit(&mut self, path: PathBuf, data: Arc<TextureData>, limit: usize) {
        let bytes = data.cpu_bytes();
        // Skip oversized textures and RGBA staging, which would amplify RAM pressure.
        self.remove(&path);
        if !data.format.is_compressed() || bytes == 0 || bytes > limit {
            return;
        }
        while self.bytes.saturating_add(bytes) > limit {
            let Some(old) = self.order.pop_front() else { break };
            if let Some(value) = self.entries.remove(&old) {
                self.bytes = self.bytes.saturating_sub(value.cpu_bytes());
            }
        }
        self.bytes += bytes;
        self.order.push_back(path.clone());
        self.entries.insert(path, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::PixelFormat;

    fn compressed(bytes: usize) -> Arc<TextureData> {
        Arc::new(TextureData {
            width: 4, height: 4, format: PixelFormat::Bc1,
            levels: vec![vec![0; bytes]], has_alpha: false, gpu_mips: false,
        })
    }

    #[test]
    fn bounded_lru_evicts_the_least_recently_used_entry() {
        let mut c = WarmCache::default();
        for p in ["a.dds", "b.dds"] {
            c.insert_with_limit(p.into(), compressed(8), 16);
        }
        assert!(c.get(Path::new("a.dds")).is_some());
        c.insert_with_limit("c.dds".into(), compressed(8), 16);
        assert!(c.get(Path::new("b.dds")).is_none());
        assert!(c.get(Path::new("a.dds")).is_some());
        assert_eq!(c.bytes(), 16);
    }

    #[test]
    fn invalidation_and_oversize_entries_do_not_keep_cpu_data() {
        let mut c = WarmCache::default();
        c.insert_with_limit("a.dds".into(), compressed(16), 8);
        assert_eq!(c.bytes(), 0);
        c.insert_with_limit("a.dds".into(), compressed(8), 8);
        assert_eq!(c.bytes(), 8);
        c.remove(Path::new("a.dds"));
        assert_eq!(c.bytes(), 0);
        c.insert_with_limit("again.dds".into(), compressed(8), 8);
        c.clear();
        assert_eq!(c.bytes(), 0);
    }
}
