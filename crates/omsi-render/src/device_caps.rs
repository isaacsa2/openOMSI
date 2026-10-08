//! What the graphics device can do and how the renderer was built for it: the backend
//! (OpenGL or not), the way the per-draw arrays reach the shaders (`ArrayPath`), the
//! adapter's memory, the reduced renderer (basic pipelines) and Enhanced+'s targets, and
//! the file that remembers, per adapter, the reduced renderer that worked there. These are
//! process-wide: `Renderer::new_on` sets them once, and everything after reads them.

/// Texture memory (MB) the adapter is taken to have room for (0 = no adapter yet), see
/// `Renderer::new`.
pub static ADAPTER_TEXTURE_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The discrete card's own memory (MB) where the system tells it (0 = not known, or not a
/// discrete card), see `Renderer::new`.
pub static ADAPTER_VRAM_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The device runs on OpenGL (set in `Renderer::new`).
pub(super) static GL_BACKEND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the device draws on OpenGL (known once a renderer is made).
pub fn gl_backend() -> bool {
    GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed)
}

/// How the per-draw arrays (the model matrices, the instance parameters, the draw list) and
/// the point lights reach the scene shader on this device (set in `Renderer::new_on`).
/// Older OpenGL chips cannot read a storage buffer in a vertex shader - an Intel HD 2500, a
/// Mali on GLES ("Downlevel flags VERTEX_STORAGE are required but not supported", the
/// renderer was never made, #770, #316) - or have no storage buffers at all (OpenGL below
/// 4.3, GLES 3.0): for them the arrays are textures the vertex shader reads texel by texel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum ArrayPath {
    /// Storage buffers everywhere (every Metal, Vulkan and DirectX 12 device).
    Storage,
    /// The vertex shader's arrays are textures; the lights stay storage buffers.
    VertexTextures,
    /// No storage buffers at all: the arrays are textures, and the point lights (the street
    /// lamps, headlights and interior lamps lit per pixel) are left out - the sixteen
    /// textures a fragment shader may read here are all taken.
    NoStorage,
}

pub(super) static ARRAY_PATH: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub(super) fn array_path() -> ArrayPath {
    match ARRAY_PATH.load(std::sync::atomic::Ordering::Relaxed) {
        1 => ArrayPath::VertexTextures,
        2 => ArrayPath::NoStorage,
        _ => ArrayPath::Storage,
    }
}

/// The card's own memory in MB where the system tells it: Windows, through DXGI, for
/// whichever backend draws; Linux, through the DRM driver's sysfs (amdgpu; not
/// NVIDIA's own driver, whose memory [`vulkan_vram_mb`] reads from Vulkan instead).
pub(super) fn dedicated_vram_mb(info: &wgpu::AdapterInfo) -> Option<u64> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
        let f: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut i = 0;
        while let Ok(a) = f.EnumAdapters1(i) {
            i += 1;
            let Ok(d) = a.GetDesc1() else { continue };
            if d.VendorId == info.vendor && d.DeviceId == info.device {
                return Some(d.DedicatedVideoMemory as u64 >> 20);
            }
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        let hex = |p: std::path::PathBuf| {
            let t = std::fs::read_to_string(p).ok()?;
            u32::from_str_radix(t.trim().trim_start_matches("0x"), 16).ok()
        };
        for e in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
            // (card0, card1, ...; not their connectors, card1-DP-1)
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let dev = e.path().join("device");
            if hex(dev.join("vendor")) != Some(info.vendor) || hex(dev.join("device")) != Some(info.device) {
                continue;
            }
            let bytes = std::fs::read_to_string(dev.join("mem_info_vram_total"))
                .ok()
                .and_then(|t| t.trim().parse::<u64>().ok());
            if let Some(b) = bytes.filter(|b| *b > 0) {
                return Some(b >> 20);
            }
        }
        None
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = info;
        None
    }
}

/// The largest device-local memory heap of a Vulkan adapter (MB).
#[cfg(target_os = "linux")]
pub(super) fn vulkan_vram_mb(adapter: &wgpu::Adapter) -> Option<u64> {
    // SAFETY: the adapter outlives the borrow, and only its memory properties are read
    let hal = unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }?;
    // SAFETY: the physical device belongs to this instance
    let props = unsafe { hal.shared_instance().raw_instance().get_physical_device_memory_properties(hal.raw_physical_device()) };
    props.memory_heaps[..props.memory_heap_count as usize]
        .iter()
        .filter(|h| h.flags.contains(ash::vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|h| h.size >> 20)
        .max()
}

#[cfg(not(target_os = "linux"))]
pub(super) fn vulkan_vram_mb(_adapter: &wgpu::Adapter) -> Option<u64> {
    None
}

/// The renderer is built for Enhanced+: its HDR pipelines and targets have the two above.
pub(super) static RT_GBUF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The renderer is built without the pipelines that are not needed to draw a picture (the
/// snowfall, the lamps in the fog, the street lamps' shadow maps): a driver whose shader
/// compiler fails on one of them (it answers "out of memory" or an unknown error - phones'
/// drivers do) takes the whole device with it, and every frame after that came out black
/// (see `Renderer::new_on`). Once set, it stays for the rest of the run. OMSI_BASIC_PIPELINES=1
/// asks for it.
pub(super) static BASIC_PIPELINES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn basic_pipelines() -> bool {
    BASIC_PIPELINES.load(std::sync::atomic::Ordering::Relaxed) || omsi_cfg::env::var_os("OMSI_BASIC_PIPELINES").is_some()
}
/// The file that remembers, per graphics adapter, the reduced renderer that worked there
/// (`~/.openomsi/gpu-fallback.cfg`, lines `adapter|msaa|basic`).
fn fallback_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::PathBuf::from(home).join(".openomsi").join("gpu-fallback.cfg"))
}

fn parse_fallbacks(text: &str) -> Vec<(String, u32, bool)> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.rsplitn(3, '|');
            let basic = f.next()?.trim() == "1";
            let msaa = f.next()?.trim().parse().ok()?;
            Some((f.next()?.to_string(), msaa, basic))
        })
        .collect()
}

/// The reduced renderer (MSAA, basic pipelines) that worked on adapter `name` before.
pub(super) fn fallback_load(name: &str) -> Option<(u32, bool)> {
    let text = std::fs::read_to_string(fallback_path()?).ok()?;
    parse_fallbacks(&text).into_iter().find(|e| e.0 == name).map(|e| (e.1, e.2))
}

/// Remember (`Some`) or forget (`None`) the reduced renderer for adapter `name`.
pub(super) fn fallback_store(name: &str, what: Option<(u32, bool)>) {
    let Some(path) = fallback_path() else { return };
    let mut all = std::fs::read_to_string(&path).map(|t| parse_fallbacks(&t)).unwrap_or_default();
    all.retain(|e| e.0 != name);
    if let Some((m, b)) = what {
        all.push((name.to_string(), m, b));
    }
    let text: String = all.iter().map(|(n, m, b)| format!("{n}|{m}|{}\n", *b as u8)).collect();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(path, text);
}

#[cfg(test)]
mod fallback_tests {
    #[test]
    fn a_remembered_fallback_is_read_back() {
        let e = super::parse_fallbacks("Adreno (TM) 830 (Gl)|1|1\nNVIDIA | odd (Vulkan)|4|0\nbroken\n");
        assert_eq!(e, vec![("Adreno (TM) 830 (Gl)".to_string(), 1, true), ("NVIDIA | odd (Vulkan)".to_string(), 4, false)]);
    }
}

pub(super) fn rt_gbuf() -> bool {
    RT_GBUF.load(std::sync::atomic::Ordering::Relaxed)
}
