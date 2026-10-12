//! OpenOMSI-only envelopes. This is a versioned format, not DRM or authentication.
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

const MAGIC: &[u8; 12] = b"OPENOMSI-OI\0";
pub const HEADER_SIZE: usize = 24;
pub const GLOBAL: &str = "global.cfgoi";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Kind {
    Config = 1,
    Tile,
    Scenery,
    Mesh,
    Terrain,
    Depot,
    Navigation,
    Vehicle,
}

pub fn kind(path: &Path) -> Option<Kind> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "cfgoi" => Some(Kind::Config),
        "tileoi" => Some(Kind::Tile),
        "oscoi" => Some(Kind::Scenery),
        "o3doi" => Some(Kind::Mesh),
        "terrainoi" => Some(Kind::Terrain),
        "hofoi" => Some(Kind::Depot),
        "jsonoi" => Some(Kind::Navigation),
        "ovhoi" | "busoi" => Some(Kind::Vehicle),
        _ => None,
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// The declared payload size is also the streaming geometry estimate, not compressed size.
pub fn payload_size(header: &[u8], expected: Kind) -> io::Result<u64> {
    if header.len() < HEADER_SIZE || &header[..12] != MAGIC {
        return Err(invalid("missing OpenOMSI OI header"));
    }
    if u16::from_le_bytes(header[12..14].try_into().unwrap()) != 1 {
        return Err(invalid("unsupported OpenOMSI OI version"));
    }
    if u16::from_le_bytes(header[14..16].try_into().unwrap()) != expected as u16 {
        return Err(invalid(
            "OpenOMSI OI file type does not match its extension",
        ));
    }
    let size = u64::from_le_bytes(header[16..24].try_into().unwrap());
    let limit = if expected == Kind::Mesh {
        1024 * 1024 * 1024
    } else {
        64 * 1024 * 1024
    };
    if size > limit {
        return Err(invalid("OpenOMSI OI payload exceeds file budget"));
    }
    Ok(size)
}

pub(crate) fn decode(path: &Path, bytes: Vec<u8>) -> io::Result<Vec<u8>> {
    let Some(kind) = kind(path) else {
        return Ok(bytes);
    };
    let size = payload_size(&bytes, kind)?;
    let mut decoder = ZlibDecoder::new(&bytes[HEADER_SIZE..]).take(size + 1);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    if out.len() as u64 != size {
        return Err(invalid("OpenOMSI OI payload length mismatch"));
    }
    if decoder.get_ref().total_in() as usize != bytes.len() - HEADER_SIZE {
        return Err(invalid("trailing data after OpenOMSI OI payload"));
    }
    Ok(out)
}

/// Encode a generated file with bounded buffers; source assets are never edited.
pub fn encode_file(source: &Path, target: &Path, kind: Kind) -> io::Result<()> {
    if source == target {
        return Err(invalid("OI source and destination must differ"));
    }
    let mut input = File::open(source)?;
    let size = input.metadata()?.len();
    let mut header = MAGIC.to_vec();
    header.extend(1u16.to_le_bytes());
    header.extend((kind as u16).to_le_bytes());
    header.extend(size.to_le_bytes());
    payload_size(&header, kind)?;
    let mut file = File::create(target)?;
    file.write_all(&header)?;
    let mut encoder = ZlibEncoder::new(file, Compression::fast());
    let copied = io::copy(&mut input, &mut encoder)?;
    if copied != size {
        return Err(invalid("generated file changed while encoding OI"));
    }
    encoder.finish()?.sync_all()?;
    Ok(())
}

/// Read the size of an installed OI mesh without decompressing its geometry.
pub fn file_payload_size(path: &Path) -> Option<u64> {
    let kind = kind(path)?;
    let mut header = [0u8; HEADER_SIZE];
    if let Ok(mut file) = File::open(path) {
        file.read_exact(&mut header).ok()?;
        return payload_size(&header, kind).ok();
    }
    crate::vfs::raw_prefix(path, HEADER_SIZE)
        .ok()
        .and_then(|header| payload_size(&header, kind).ok())
}

/// Stock OMSI keeps its ordinary manifest. Converted maps have only the OI manifest.
pub fn global_path(dir: &Path) -> std::path::PathBuf {
    let native = crate::resolve_path(dir, "global.cfg");
    if crate::vfs::is_file(&native) {
        native
    } else {
        crate::resolve_path(dir, GLOBAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelopes_require_their_version_type_length_and_compressed_payload() {
        let dir = std::env::temp_dir().join(format!("oi-envelope-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("source");
        let dst = dir.join("map.tileoi");
        std::fs::write(&src, "[version]\n14\n").unwrap();
        encode_file(&src, &dst, Kind::Tile).unwrap();
        let bytes = std::fs::read(&dst).unwrap();
        assert_eq!(
            crate::vfs::read(&dst).unwrap(),
            std::fs::read(&src).unwrap()
        );
        assert!(!bytes.windows(9).any(|b| b == b"[version]"));
        assert!(decode(&dir.join("bad.oscoi"), bytes.clone()).is_err());
        for index in [0, 12, 16] {
            let mut bad = bytes.clone();
            bad[index] ^= 1;
            assert!(decode(&dst, bad).is_err());
        }
        assert!(decode(&dst, b"[version]\n14\n".to_vec()).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode(&dst, trailing).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
