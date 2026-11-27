//! One incremental launcher log for the UI, core and preview service. A stable lock
//! serializes append and rotation across launcher processes.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 4 * 1024 * 1024;

fn append_at(dir: &Path, line: &str, limit: u64) -> io::Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("launcher.lock"))?;
    lock.lock()?;
    let current = dir.join("launcher.log");
    let previous = dir.join("launcher.1.log");
    let oldest = dir.join("launcher.2.log");
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

fn record(source: &str, level: &str, component: &str, message: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let pid = std::process::id();
    // Never allow a multiline error to impersonate the next record.
    let message = message.replace('\r', "\\r").replace('\n', "\\n");
    let timestamp = crate::local_now().map_or_else(
        || format!("{secs}.{millis:03}"),
        |(y, m, d, h, min)| format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{:02}.{millis:03} local", secs % 60),
    );
    let line = format!("[{timestamp} {level:<5} {source}::{component} pid={pid}] {message}");
    if let Err(error) = append_at(&crate::data_dir(), &line, MAX_BYTES) {
        eprintln!("openOMSI: cannot write launcher log: {error}");
    }
}

pub fn launcher(level: &str, component: &str, message: &str) {
    record("launcher", level, component, message);
}

pub fn service(level: &str, component: &str, message: &str) {
    record("service", level, component, message);
}

/// Preserve the severity of env_logger records coming from the preview process.
pub fn helper_line(pid: u32, line: &str) {
    let severity = line.strip_prefix('[').and_then(|s| s.split_whitespace().nth(1)).filter(|s| matches!(*s, "ERROR" | "WARN" | "INFO" | "DEBUG" | "TRACE")).unwrap_or("INFO");
    service(severity, "helper", &format!("helper_pid={pid} {line}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_rotates_without_losing_the_previous_two_segments() {
        let dir = std::env::temp_dir().join(format!("omsi-log-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for line in ["launcher::core first", "service::helper second", "launcher::core third", "service::request fourth"] {
            append_at(&dir, line, 1).unwrap();
        }
        assert_eq!(fs::read_to_string(dir.join("launcher.log")).unwrap(), "service::request fourth\n");
        assert_eq!(fs::read_to_string(dir.join("launcher.1.log")).unwrap(), "launcher::core third\n");
        assert_eq!(fs::read_to_string(dir.join("launcher.2.log")).unwrap(), "service::helper second\n");
        let _ = fs::remove_dir_all(dir);
    }
}
