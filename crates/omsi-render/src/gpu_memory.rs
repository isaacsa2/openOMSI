//! GPU dedicated-memory detection for the rendering backends.
//! The OpenGL and ANGLE paths may omit PCI IDs even on a discrete GPU.

use super::*;

/// The card's own memory in MB where the system tells it: Windows, through DXGI, for
/// whichever backend draws; Linux, through the DRM driver's sysfs (amdgpu; not
/// NVIDIA's own driver, whose memory [`vulkan_vram_mb`] reads from Vulkan instead).
fn dedicated_vram_mb(info: &wgpu::AdapterInfo) -> Option<u64> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
        let f: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut i = 0;
        let mut name_match = None;
        let mut ambiguous_name = false;
        while let Ok(a) = f.EnumAdapters1(i) {
            i += 1;
            let Ok(d) = a.GetDesc1() else { continue };
            let mb = d.DedicatedVideoMemory as u64 >> 20;
            if mb == 0 {
                continue;
            }
            // OpenGL/ANGLE often report vendor/device as 0 even on a discrete GPU.
            // Prefer matching PCI IDs when they are available.
            if info.vendor != 0 && info.device != 0
                && d.VendorId == info.vendor && d.DeviceId == info.device
            {
                return Some(mb);
            }
            // DXGI still knows the card's name. Use it only if it identifies exactly
            // one adapter; never borrow another card's budget on a multi-GPU PC.
            let dxgi_name = String::from_utf16_lossy(&d.Description);
            let dxgi_name = dxgi_name.trim_end_matches('\0').trim();
            // ANGLE wraps the GPU name and embeds the PCI device ID as
            // "(0x00006613)". Check both the name and that ID, when present.
            let angle_device = info.name
                .split("(0x")
                .nth(1)
                .and_then(|s| s.split(')').next())
                .and_then(|s| u32::from_str_radix(s, 16).ok());
            let angle_matches = info.name.starts_with("ANGLE (")
                && info.name.contains(dxgi_name)
                && angle_device == Some(d.DeviceId)
                && (info.vendor == 0 || info.vendor == d.VendorId);
            if (info.vendor == 0 || info.device == 0)
                && (dxgi_name.eq_ignore_ascii_case(info.name.trim()) || angle_matches)
            {
                if name_match.replace(mb).is_some() {
                    ambiguous_name = true;
                }
            }
        }
        if ambiguous_name { None } else { name_match }
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
fn vulkan_vram_mb(adapter: &wgpu::Adapter) -> Option<u64> {
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
fn vulkan_vram_mb(_adapter: &wgpu::Adapter) -> Option<u64> {
    None
}

/// The card's own memory in MB: wgpu's report on DirectX 12 (the adapter's DXGI
/// `DedicatedVideoMemory`, the number [`dedicated_vram_mb`] reads), else what the system
/// tells. (wgpu's Vulkan report sums every device-local heap where [`vulkan_vram_mb`] takes
/// the largest, and Metal's is the working set the system recommends, not the card's own
/// memory: those keep their own paths, so the texture budgets stay as they were.)
pub(super) fn adapter_vram_mb(adapter: &wgpu::Adapter, info: &wgpu::AdapterInfo, mem: Option<&wgpu::AdapterMemoryInfo>) -> Option<u64> {
    match mem {
        Some(m) if info.backend == wgpu::Backend::Dx12 => Some(m.dedicated_bytes >> 20),
        _ => dedicated_vram_mb(info).or_else(|| vulkan_vram_mb(adapter)),
    }
}
