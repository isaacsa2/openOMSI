//! The pipelines compiled at a start kept for the next one: the driver's own compiled
//! shaders (Vulkan's pipeline cache; on OpenGL and ANGLE the linked programs' binaries),
//! in `~/.openomsi/cache`, one file per graphics chip and interface. On ANGLE the scene's
//! pipelines took about 6 s each to compile, some 70 s of every start on a Radeon R7 200;
//! a phone compiles them at every start as well. wgpu refuses data of another driver,
//! chip or version itself (and starts empty), so an update or another card only costs
//! one compiling start. OMSI_NO_PIPELINE_CACHE=1 compiles everything at every start.

use std::path::PathBuf;
use std::sync::Mutex;

/// The cache of the device the pipelines are made on now, and its file.
static CACHE: Mutex<Option<(wgpu::Device, wgpu::PipelineCache, PathBuf)>> = Mutex::new(None);

/// The cache files are no bigger than this (a whole set is a few MB).
const MAX_BYTES: u64 = 256 << 20;

/// The feature to ask the device for: the cache where the adapter keeps one.
pub(crate) fn wanted(adapter: &wgpu::Adapter, bare: bool) -> wgpu::Features {
    if bare || omsi_cfg::flags::OMSI_NO_PIPELINE_CACHE.is_set() {
        return wgpu::Features::empty();
    }
    adapter.features() & wgpu::Features::PIPELINE_CACHE
}

/// The file of this chip, driver and interface.
fn path(info: &wgpu::AdapterInfo) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let home = PathBuf::from(home);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (&info.name, info.vendor, info.device, &info.driver, &info.driver_info).hash(&mut h);
    // (an unusable home: no cache rather than a file in the game's folder)
    home.is_absolute().then(|| home.join(format!(".openomsi/cache/pipelines-{:?}-{:016x}.bin", info.backend, h.finish()).to_lowercase()))
}

/// The cache for `device`, from its file where there is one.
pub(crate) fn open(device: &wgpu::Device, info: &wgpu::AdapterInfo) {
    let mut slot = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *slot = None;
    let Some(path) = path(info).filter(|_| device.features().contains(wgpu::Features::PIPELINE_CACHE)) else { return };
    let data = std::fs::metadata(&path)
        .ok()
        .filter(|m| m.len() <= MAX_BYTES)
        .and_then(|_| std::fs::read(&path).ok());
    // SAFETY: the data is only ever what `get_data` of a cache of this chip, driver and
    // interface returned (see `path`), and wgpu checks its header against the device
    let cache = unsafe {
        device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor { label: Some("omsi pipelines"), data: data.as_deref(), fallback: true })
    };
    log::info!("pipeline cache: {} ({})", path.display(), match &data { Some(d) => format!("{} KB from the last start", d.len() >> 10), None => "new".into() });
    *slot = Some((device.clone(), cache, path));
}

/// The cache to make pipelines with on `device`.
pub(crate) fn current(device: &wgpu::Device) -> Option<wgpu::PipelineCache> {
    let slot = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    slot.as_ref().filter(|(d, _, _)| d == device).map(|(_, c, _)| c.clone())
}

/// Writes the cache of `device` to its file (a copy put in place whole: a crash or another
/// game writing at the same time leaves a complete file).
pub(crate) fn save(device: &wgpu::Device) {
    let Some((cache, path)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|(d, _, _)| d == device).map(|(_, c, p)| (c.clone(), p.clone())) else {
        return;
    };
    let Some(data) = cache.get_data().filter(|d| !d.is_empty() && d.len() as u64 <= MAX_BYTES) else { return };
    let Some(parent) = path.parent() else { return };
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let written = std::fs::create_dir_all(parent).and_then(|_| std::fs::write(&temp, &data)).and_then(|_| std::fs::rename(&temp, &path));
    match written {
        Ok(()) => log::info!("pipeline cache: {} KB written", data.len() >> 10),
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            log::warn!("pipeline cache: {} could not be written: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    /// The file names one chip, driver and interface: another of any of them, another file.
    #[test]
    fn each_chip_driver_and_interface_has_a_file_of_its_own() {
        let info = |backend, driver: &str| wgpu::AdapterInfo {
            name: "AMD Radeon R7 200 Series".into(),
            vendor: 0x1002,
            device: 0x6613,
            device_type: wgpu::DeviceType::DiscreteGpu,
            device_pci_bus_id: String::new(),
            driver: driver.into(),
            driver_info: String::new(),
            backend,
            subgroup_min_size: 64,
            subgroup_max_size: 64,
            transient_saves_memory: false,
        };
        let a = super::path(&info(wgpu::Backend::Gl, "27.20"));
        let Some(a) = a else { return };
        assert!(a.to_string_lossy().contains("pipelines-gl-"), "{}", a.display());
        assert_eq!(Some(a.clone()), super::path(&info(wgpu::Backend::Gl, "27.20")));
        assert_ne!(Some(a.clone()), super::path(&info(wgpu::Backend::Gl, "27.21")));
        assert_ne!(Some(a), super::path(&info(wgpu::Backend::Vulkan, "27.20")));
    }
}
