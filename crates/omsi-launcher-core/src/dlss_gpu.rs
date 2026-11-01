//! Conservative GPU preflight for the NVIDIA DLSS / DLAA settings row.
//!
//! RTX name + NVIDIA vendor is a *candidate*, not a promise that the NVIDIA
//! Streamline runtime, driver and the feature's own capability query work.
//! The native renderer always performs that final test before using DLSS.
//! Never turn a failed enumeration into a visible DLSS toggle.

/// Public so the game renderer and launcher can apply the same conservative
/// identification rule to the selected adapter. Names can vary by driver.
pub fn nvidia_rtx_candidate(vendor: u32, name: &str) -> bool {
    vendor == 0x10de && name.to_ascii_uppercase().contains("RTX")
}

/// Whether Windows currently enumerates at least one NVIDIA RTX candidate.
/// Multi-GPU machines may have a different adapter selected by the game;
/// the renderer must recheck the *actual* device before enabling DLSS.
#[cfg(windows)]
pub fn dlss_gpu_candidate() -> bool {
    static RTX_PRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *RTX_PRESENT.get_or_init(enumerate_rtx_adapters)
}

#[cfg(windows)]
fn enumerate_rtx_adapters() -> bool {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return false;
    };
    for index in 0..64 {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            break;
        };
        let Ok(info) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        let end = info.Description.iter().position(|&unit| unit == 0).unwrap_or(info.Description.len());
        let name = String::from_utf16_lossy(&info.Description[..end]);
        if nvidia_rtx_candidate(info.VendorId, &name) {
            return true;
        }
    }
    false
}

#[cfg(not(windows))]
pub fn dlss_gpu_candidate() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtx_name_is_not_enough_without_nvidia_vendor() {
        assert!(nvidia_rtx_candidate(0x10de, "NVIDIA GeForce RTX 4060"));
        assert!(nvidia_rtx_candidate(0x10de, "NVIDIA GeForce RTX 2080 Ti"));
        assert!(nvidia_rtx_candidate(0x10de, "NVIDIA RTX A4000"));
        assert!(!nvidia_rtx_candidate(0x10de, "NVIDIA GeForce GTX 1080"));
        assert!(!nvidia_rtx_candidate(0x1002, "NVIDIA GeForce RTX 4060"));
        assert!(!nvidia_rtx_candidate(0x8086, "Intel Arc A770"));
        assert!(!nvidia_rtx_candidate(0, "Microsoft Basic Render Driver"));
    }

    #[test]
    fn empty_or_unidentified_adapter_is_not_supported() {
        assert!(!nvidia_rtx_candidate(0x10de, ""));
        assert!(!nvidia_rtx_candidate(0x10de, "NVIDIA"));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_launcher_never_offers_dlss() {
        assert!(!dlss_gpu_candidate());
    }
}
