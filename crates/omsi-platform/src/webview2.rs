//! Microsoft's WebView2, which the desktop launcher draws its page with on Windows: part of
//! Windows 11 and of an updated Windows 10, missing on some older or trimmed systems. Where
//! the launcher's window could not open, the game offers to install it (its `launcher_link`)
//! with Microsoft's own small installer (the "Evergreen Bootstrapper", which downloads the
//! runtime itself) - only when Windows really has no runtime (`runtime`): a launcher that
//! failed for another reason with the runtime there was taken for one without it, and the
//! installer, which says "already installed", was offered at every start.

/// Microsoft's permanent link to the Evergreen Bootstrapper.
const BOOTSTRAPPER: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

/// Download the installer and run it only after Windows verifies that Microsoft
/// signed it. Each attempt gets a separate directory instead of reusing a predictable
/// executable name in the shared temp directory.
pub fn install() -> anyhow::Result<()> {
    if !cfg!(windows) {
        anyhow::bail!("WebView2 is only needed on Windows");
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!("openomsi-webview2-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&dir)?;
    let path = dir.join("MicrosoftEdgeWebview2Setup.exe");
    let result = (|| -> anyhow::Result<()> {
        use std::io::Read;
        let resp = crate::updater::agent().get(BOOTSTRAPPER).call()?;
        let mut out = std::fs::File::create(&path)?;
        // The Evergreen bootstrapper is small, not an offline 150 MB runtime.
        // Refuse implausibly large network responses before running an EXE.
        let mut reader = resp.into_reader().take(64 * 1024 * 1024 + 1);
        let size = std::io::copy(&mut reader, &mut out)?;
        drop(out);
        if size == 0 || size > 64 * 1024 * 1024 {
            anyhow::bail!("WebView2 bootstrapper has an unexpected size");
        }
        verify_microsoft_signature(&path)?;
        // (its own window shows the progress; Windows asks for administrator consent)
        let status = std::process::Command::new(&path).arg("/install").status()?;
        if !status.success() {
            anyhow::bail!("the WebView2 installer ended with {status}");
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(dir);
    result
}

/// Verify both the Authenticode trust chain AND Microsoft's signing identity.
/// A valid signature by an unrelated publisher must not authorize execution.
fn verify_microsoft_signature(path: &std::path::Path) -> anyhow::Result<()> {
    let system = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("Windows system directory was not found"))?;
    let powershell = system.join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    // Quote an arbitrary temp path as one PowerShell literal, including apostrophes.
    let quoted = format!("'{}'", path.to_string_lossy().replace('\'', "''"));
    let script = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath {quoted}; \
         if ($s.Status -ne 'Valid' -or $null -eq $s.SignerCertificate -or \
         $s.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false) -ne 'Microsoft Corporation') {{ exit 1 }}"
    );
    let status = std::process::Command::new(powershell)
        .arg("-NoProfile").arg("-NonInteractive").arg("-Command").arg(script)
        .status()?;
    if !status.success() {
        anyhow::bail!("WebView2 installer does not have a valid Microsoft Authenticode signature");
    }
    Ok(())
}

/// The WebView2 runtime as Windows has it registered.
#[derive(Clone, Debug)]
pub struct Runtime {
    pub version: String,
    /// Its program was found (a runtime whose files a "clean-up" tool removed stays registered,
    /// and its installer then says it is installed).
    pub present: bool,
}

/// The runtime Windows has registered (machine-wide or for this user), if any.
pub fn runtime() -> Option<Runtime> {
    #[cfg(windows)]
    {
        registered::runtime()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod registered {
    use super::Runtime;
    use std::path::PathBuf;
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    /// The runtime's client key of Microsoft's updater (the same for every installation).
    const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    fn text(root: HKEY, key: &str, name: &str) -> Option<String> {
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (key, name) = (wide(key), wide(name));
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: both names are NUL-terminated, the buffer's size in bytes is in `len`
        let r = unsafe { RegGetValueW(root, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) };
        if r.is_err() {
            return None;
        }
        buf.truncate(len as usize / 2);
        let s = String::from_utf16_lossy(&buf).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    pub(super) fn runtime() -> Option<Runtime> {
        let keys = [(HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\{CLIENT}")), (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\{CLIENT}")), (HKEY_CURRENT_USER, format!(r"Software\{CLIENT}"))];
        let mut found: Option<Runtime> = None;
        for (root, key) in keys {
            let Some(version) = text(root, &key, "pv").filter(|v| v != "0.0.0.0") else { continue };
            // (where it lives: the updater's own note, else the places its installers use)
            let mut dirs: Vec<PathBuf> = text(root, &key, "location").map(PathBuf::from).into_iter().collect();
            for (var, sub) in [("ProgramFiles(x86)", r"Microsoft\EdgeWebView\Application"), ("ProgramFiles", r"Microsoft\EdgeWebView\Application"), ("LOCALAPPDATA", r"Microsoft\EdgeWebView\Application")] {
                if let Some(base) = std::env::var_os(var) {
                    dirs.push(PathBuf::from(base).join(sub));
                }
            }
            let present = dirs.iter().any(|d| d.join(&version).join("msedgewebview2.exe").is_file());
            if present {
                return Some(Runtime { version, present });
            }
            found.get_or_insert(Runtime { version, present });
        }
        found
    }
}
