//! The SHA-256 a release file is checked against when GitHub's API lists none for it.

use super::{agent, short_error, with_retries, Release};

/// The SHA-256 `r`'s file is checked against: the one GitHub's API lists, or else its line in
/// the `SHA256SUMS` every release publishes beside its files (the github.com way of
/// `latest_from_site`, and releases the API lists no digest for). A file with neither is not
/// installed: nothing would tell a damaged or replaced one from the real one. (`file://`
/// releases of `OMSI_UPDATE_URL` tests are taken as they are.)
pub(super) fn expected_sha256(r: &Release) -> anyhow::Result<Option<String>> {
    if let Some(h) = &r.sha256 {
        return Ok(Some(h.clone()));
    }
    if r.asset_url.starts_with("file://") {
        return Ok(None);
    }
    let Some((folder, _)) = r.asset_url.rsplit_once('/') else { anyhow::bail!("the release file has no address") };
    let sums = format!("{folder}/SHA256SUMS");
    let text = with_retries("asking for the release's checksums", || agent().get(&sums).call())
        .map_err(|e| anyhow::anyhow!("openOMSI {} cannot be checked, so it is not installed: {}", r.version, short_error(&e)))?
        .into_string()?;
    sums_line(&text, &r.asset_name).map(Some).ok_or_else(|| anyhow::anyhow!("openOMSI {} cannot be checked, so it is not installed: SHA256SUMS does not list {}", r.version, r.asset_name))
}

/// The SHA-256 a `sha256sum` listing gives `name` (lowercase hex), if any.
fn sums_line(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (hash, file) = l.trim().split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[test]
    fn a_sums_line_names_its_file_only() {
        let h = "AB".repeat(32);
        let text = format!("{h}  openOMSI-linux.zip\n{}  *openOMSI-windows.zip\nshort  openOMSI-mac.zip\n", "cd".repeat(32));
        assert_eq!(sums_line(&text, "openOMSI-linux.zip"), Some("ab".repeat(32)));
        assert_eq!(sums_line(&text, "openOMSI-windows.zip"), Some("cd".repeat(32)));
        assert_eq!(sums_line(&text, "openOMSI-mac.zip"), None);
        assert_eq!(sums_line(&text, "openOMSI-linux.zip.part"), None);
    }

    /// A release with no SHA-256 from the API (the github.com way when the API refuses) is
    /// checked against the release's SHA256SUMS; one that does not match, or that has no
    /// SHA256SUMS, is not taken.
    #[test]
    fn a_release_without_a_listed_sha_is_checked_by_its_sums() {
        use sha2::Digest;
        use std::io::{BufRead, BufReader};
        let data: Vec<u8> = (0..50_000u32).map(|i| (i * 3) as u8).collect();
        let sha: String = sha2::Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
        for (case, sums) in [("right", Some(sha.clone())), ("wrong", Some("0".repeat(64))), ("none", None)] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let name = format!("openOMSI-sums-{case}-{}.zip", std::process::id());
            let (served, listed) = (data.clone(), sums.map(|h| format!("{h}  {name}\n")));
            std::thread::spawn(move || {
                for conn in listener.incoming() {
                    let Ok(mut c) = conn else { continue };
                    let mut reader = BufReader::new(c.try_clone().unwrap());
                    let mut first = String::new();
                    let _ = reader.read_line(&mut first);
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                    }
                    let body: Option<Vec<u8>> = if first.contains("/SHA256SUMS") { listed.clone().map(String::into_bytes) } else { Some(served.clone()) };
                    let _ = match body {
                        Some(b) => c.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len()).as_bytes()).and_then(|_| c.write_all(&b)),
                        None => c.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
                    };
                }
            });
            let r = Release { version: "9.9.8".into(), page: String::new(), notes: String::new(), asset_name: name.clone(), asset_url: format!("http://127.0.0.1:{port}/v9.9.8/{name}"), size: data.len() as u64, sha256: None };
            let to = std::env::temp_dir().join(&name);
            let _ = std::fs::remove_file(&to);
            let got = download(&r, &to, &Mutex::new(Status::Idle));
            if case == "right" {
                got.unwrap();
                assert_eq!(std::fs::read_to_string(checked_note(&to)).unwrap(), format!("{} {sha}", data.len()));
            } else {
                assert!(got.is_err(), "{case}: taken");
                assert!(!to.exists(), "{case}: put in place");
            }
            let _ = std::fs::remove_file(&to);
            let _ = std::fs::remove_file(checked_note(&to));
            let _ = std::fs::remove_file(to.with_extension("part"));
        }
    }
}
