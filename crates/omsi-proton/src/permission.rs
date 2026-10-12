//! An editable creator declaration, not verified identity or copy protection.
use anyhow::{ensure, Result};
use std::{io::Read, path::Path};

pub fn validate_file(path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= 64 * 1024,
        "map configuration exceeds 64 KiB; conversion blocked"
    );
    let mut bytes = Vec::new();
    file.by_ref().take(64 * 1024 + 1).read_to_end(&mut bytes)?;
    validate(&bytes)
}

/// Require one canonical declaration before any map output is created.
/// Do not use the general INI map: it discards duplicate keys and sections.
pub fn validate(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= 64 * 1024,
        "map configuration exceeds 64 KiB; conversion blocked"
    );
    let text = crate::ini_text(bytes);
    let mut section = "";
    let mut found = false;
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.starts_with([';', '#']) || line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = s;
            continue;
        }
        if !section.eq_ignore_ascii_case("openomsi") {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            ensure!(
                !line.eq_ignore_ascii_case("allowConversion"),
                "conversion blocked: allowConversion has no value"
            );
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("allowConversion") {
            continue;
        }
        ensure!(
            section == "openomsi" && key.trim() == "allowConversion",
            "conversion blocked: use exactly [openomsi] and allowConversion=1"
        );
        ensure!(!found, "conversion blocked: duplicate or conflicting allowConversion declarations; keep exactly one");
        ensure!(
            value.trim() == "1",
            "conversion blocked: [openomsi] allowConversion must be exactly 1; found {:?}",
            value.trim()
        );
        found = true;
    }
    ensure!(found, "conversion blocked: original main configuration (.map.txt, .bus.txt or .ini) must contain [openomsi] followed by allowConversion=1; only the creator should add this declaration");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_explicit_canonical_one_is_accepted() {
        for s in [
            "[openomsi]\nallowConversion=1",
            "\u{feff}[map]\nbaseDir=City\n\n[openomsi]\nallowConversion = 1\n",
        ] {
            validate(s.as_bytes()).unwrap();
        }
        let mut utf16 = vec![0xff, 0xfe];
        utf16.extend(
            "[openomsi]\r\nallowConversion=1\r\n"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        validate(&utf16).unwrap();
        assert!(validate(&vec![b'x'; 64 * 1024 + 1]).is_err());
        for s in [
            "",
            "[openomsi]",
            "[openomsi]\nallowConversion=",
            "[openomsi]\nallowConversion=0",
            "[openomsi]\nallowConversion=true",
            "[openomsi]\nallowConversion=01",
            "[openomsi]\nallowConversion=+1",
            "[openomsi]\nallowConversion=1 ; yes",
            "[map]\nallowConversion=1",
            "[OpenOMSI]\nallowConversion=1",
            "[openomsi]\nallowconversion=1",
            "[openomsi]\nallowConversion",
            "[openomsi]\nallowConversion=1\nallowConversion=0",
            "[openomsi]\nallowConversion=1\n[openomsi]\nallowConversion=1",
        ] {
            assert!(validate(s.as_bytes()).is_err(), "{s:?}");
        }
    }
}
