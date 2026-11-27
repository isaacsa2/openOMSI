//! Incremental diagnostic streams, shared by launcher processes. A stable lock file
//! serializes append and rotation; only these known files are ever rotated.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 4 * 1024 * 1024;

fn append_at(dir: &Path, stream: &str, line: &str, limit: u64) -> io::Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(format!("{stream}.lock")))?;
    lock.lock()?;
    let current = dir.join(format!("{stream}.log"));
    let previous = dir.join(format!("{stream}.1.log"));
    let oldest = dir.join(format!("{stream}.2.log"));
    if fs::metadata(&current).is_ok_and(|m| m.len() >= limit) {
        if oldest.exists() {
            fs::remove_file(&oldest)?;
        }
        if previous.exists() {
            fs::rename(&previous, &oldest)?;
        }
        fs::rename(&current, &previous)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(current)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn record(stream: &str, level: &str, component: &str, message: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let pid = std::process::id();
    // Never allow a multiline error to impersonate the next record.
    let message = message.replace('\r', "\\r").replace('\n', "\\n");
    let line = format!("[{secs}.{millis:03} {level:<5} {component} pid={pid}] {message}");
    if let Err(error) = append_at(&crate::data_dir(), stream, &line, MAX_BYTES) {
        eprintln!("openOMSI: cannot write {stream} log: {error}");
    }
}

pub fn launcher(level: &str, component: &str, message: &str) {
    record("launcher", level, component, message);
}

pub fn service(level: &str, component: &str, message: &str) {
    record("launcher-service", level, component, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_rotates_without_losing_the_previous_two_segments() {
        let dir = std::env::temp_dir().join(format!("omsi-log-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for line in ["first", "second", "third", "fourth"] {
            append_at(&dir, "launcher", line, 1).unwrap();
        }
        assert_eq!(fs::read_to_string(dir.join("launcher.log")).unwrap(), "fourth\n");
        assert_eq!(fs::read_to_string(dir.join("launcher.1.log")).unwrap(), "third\n");
        assert_eq!(fs::read_to_string(dir.join("launcher.2.log")).unwrap(), "second\n");
        let _ = fs::remove_dir_all(dir);
    }
}
