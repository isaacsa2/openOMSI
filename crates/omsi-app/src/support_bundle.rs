//! Issue attachments built from an allowlist, never from configuration dumps.
//!
//! Free-text logs can contain chat, credentials and filenames. Export their known
//! diagnostic event and numeric measurements only; unknown lines are omitted.

use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Conservative for labels: a path, network address or credential is never useful
/// as a GPU/driver/controller label. Paths of content are handled separately.
pub(crate) fn redact_label(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let sensitive = ["token", "password", "passwd", "secret", "cookie", "authorization",
        "credential", "session", "discord", "serial", "api_key", "apikey", "bearer"];
    if text.len() > 256 || text.contains('/') || text.contains('\\') || text.contains('@')
        || text.contains(':') || sensitive.iter().any(|s| lower.contains(s))
        || text.split_whitespace().any(|w| w.trim_matches(['[', ']', '(', ')', ',']).parse::<std::net::IpAddr>().is_ok())
    {
        "[redacted]".into()
    } else {
        text.chars().filter(|c| !c.is_control()).collect()
    }
}

fn content_name(text: &str) -> String {
    let text = text.replace('\\', "/");
    if !(text.starts_with("maps/") || text.starts_with("Vehicles/"))
        || text.split('/').any(|c| c == ".." || c.is_empty())
        || text.contains(':')
    {
        return "[redacted]".into();
    }
    text.split('/').map(redact_label).collect::<Vec<_>>().join("/")
}

fn clean_json(value: &mut Value) {
    match value {
        Value::String(s) => {
            *s = if s.starts_with("maps/") || s.starts_with("Vehicles/") {
                content_name(s)
            } else { redact_label(s) };
        }
        Value::Array(a) => a.iter_mut().for_each(clean_json),
        Value::Object(o) => o.values_mut().for_each(clean_json),
        _ => {}
    }
}

/// No raw strings from a log enter a bundle. Even known events carry only finite
/// numbers and a fixed event name (so a credential on a continuation line is safe).
fn log_projection(text: &str) -> String {
    const EVENTS: &[(&str, &str)] = &[
        ("stutter: frame", "stutter"), ("profile ", "profile"),
        ("graphics device was lost", "device_lost"), ("device lost (", "device_lost"),
        ("pipelines failed", "pipeline_failure"), ("graphics device opened", "device_opened"),
        ("graphics adapter:", "adapter"), ("opening graphics device:", "device_request"),
        ("renderer:", "renderer"), ("system:", "system"),
        ("without multisampling", "msaa_fallback"), ("basic pipelines", "basic_fallback"),
        ("graphics card cannot keep up", "graphics_governor"),
        ("status:", "status"), ("game stopped on an error", "panic"),
    ];
    let mut out = String::from("Privacy projection: free text, paths, addresses and unrecognised log lines omitted. Numeric fields are positional, not named.\n");
    for (line_index, line) in text.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        if let Some((_, event)) = EVENTS.iter().find(|(needle, _)| lower.contains(needle)) {
            // Do not harvest numbers from credential-bearing lines either.
            let private = ["token", "password", "secret", "cookie", "authorization", "serial", "bearer"]
                .iter().any(|word| lower.contains(word));
            let numbers: Vec<f64> = if private { vec![] } else {
                // Parse only the fixed stutter header; a free-form number could
                // itself be a password, phone number or hardware serial.
                line.split_once("stutter: frame ").and_then(|(_, rest)| {
                    let words: Vec<_> = rest.split_whitespace().take(5).collect();
                    if words.len() != 5 || words[1] != "took" || words[3] != "ms" { return None; }
                    let frame = words[0].parse::<u32>().ok()?;
                    let ms = words[2].parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0)?;
                    Some(vec![frame as f64, ms])
                }).unwrap_or_default()
            };
            out.push_str(&format!("{} {} {:?}\n", line_index + 1, event, numbers));
        }
    }
    out
}

fn tail(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(LOG_BYTES)))?;
    let mut bytes = Vec::new();
    file.take(LOG_BYTES).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    // A tail starting inside a line cannot reliably identify that line's context.
    Ok(if size > LOG_BYTES { text.split_once('\n').map(|(_, t)| t).unwrap_or("").into() } else { text.into_owned() })
}

pub(crate) fn snapshot(
    renderer: Option<&omsi_render::Renderer>,
    settings: &crate::settings::Settings,
    devices: Option<Vec<crate::controllers::Connected>>,
    session: Option<(&str, Option<&str>, Option<&str>, Option<&str>)>,
    source: &str,
) -> Value {
    let graphics = renderer.map(|r| {
        let info = &r.adapter_info;
        json!({
            "backend": format!("{:?}", info.backend), "adapter": redact_label(&info.name),
            "vendor_id": info.vendor, "device_id": info.device,
            "driver": redact_label(&info.driver), "driver_info": redact_label(&info.driver_info),
            "device_features": format!("{:?}", r.device.features()),
            "device_limits": {"max_texture_dimension_2d": r.device.limits().max_texture_dimension_2d,
                "max_buffer_size": r.device.limits().max_buffer_size,
                "max_storage_buffer_binding_size": r.device.limits().max_storage_buffer_binding_size,
                "max_bind_groups": r.device.limits().max_bind_groups,
                "max_sampled_textures_per_shader_stage": r.device.limits().max_sampled_textures_per_shader_stage,
                "max_storage_buffers_per_shader_stage": r.device.limits().max_storage_buffers_per_shader_stage},
            "msaa": r.options.msaa, "ssao": r.options.ssao,
            "render_scale": r.options.render_scale, "shadow_size": r.options.shadow_size,
            "basic_pipelines": r.basic_rendering(), "device_lost": r.device_lost().is_some(),
        })
    });
    let controllers: Option<Vec<Value>> = devices.map(|d| d.into_iter().map(|d| {
        json!({"name": redact_label(&d.name), "vid_pid": d.hardware_id,
            "axes": d.axes.len(), "buttons": d.buttons, "gamepad": d.gamepad,
            "ffb_capable": d.ff_capable, "ffb_effect_available": d.ff})
    }).collect());
    let roots: Vec<Value> = omsi_cfg::content_roots().iter().enumerate()
        .map(|(i, _)| json!({"alias": format!("root-{}", i + 1), "priority": i})).collect();
    let archives: Vec<Value> = omsi_cfg::vfs::mounts().iter().enumerate().map(|(i, a)| {
        json!({"alias": format!("archive-{}", i + 1), "files": a.file_count(), "bytes": a.total_size()})
    }).collect();
    let mut value = json!({
        "schema_version": 1,
        "recorded_unix_seconds": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0),
        "source": source, "version": crate::startup::VERSION, "build": crate::startup::BUILD,
        "commit": env!("OMSI_BUILD_COMMIT"), "build_unix_seconds": env!("OMSI_BUILD_TIMESTAMP"),
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "os_version": crate::applog::os_version(),
        "graphics": graphics,
        "graphics_settings": {"preset": settings.graphics, "backend_requested": settings.graphics_api,
            "render_scale": settings.render_scale, "msaa": settings.msaa, "ssao": settings.ssao,
            "shadows": settings.shadows, "shadow_size": settings.shadow_size,
            "mirrors": settings.mirror_refresh, "mirror_size": settings.mirror_size,
            "reflections": settings.reflections, "vsync": settings.vsync, "max_fps": settings.max_fps},
        "session": session.map(|(map, bus, line, tour)| json!({"map": content_name(map),
            "bus": bus.map(content_name), "line": line.map(redact_label), "tour": tour.map(redact_label)})),
        "content_roots": roots, "mounted_archives": archives,
        "loaded_content_packs": null,
        "controllers": controllers, "physical_memory_bytes": crate::memory::physical_memory(),
        "adapter_vram_mb": omsi_render::ADAPTER_VRAM_MB.load(std::sync::atomic::Ordering::Relaxed),
        "adapter_texture_mb": omsi_render::ADAPTER_TEXTURE_MB.load(std::sync::atomic::Ordering::Relaxed),
    });
    clean_json(&mut value);
    value
}

/// Retain the last session's safe metadata even if the game cannot be reopened.
pub(crate) fn record(app: &crate::App) {
    let mut value = snapshot(app.renderer.as_ref(), &app.settings, app.input.controllers.as_ref().map(|c| c.connected()),
        Some((&app.args.map, app.args.bus.as_deref(), app.args.line.as_deref(), app.args.tour.as_deref())), "game");
    if let (Some(r), Some(scene)) = (app.renderer.as_ref(), app.scene.as_ref()) {
        value["gpu_memory_bytes"] = json!({"textures": r.texture_bytes(scene), "meshes": r.mesh_bytes(scene)});
    }
    let dir = omsi_launcher_lib::data_dir();
    let path = dir.join(format!("support-session-{}.json", std::process::id()));
    let temp = path.with_extension("tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(&value) {
        if std::fs::write(&temp, bytes).is_ok() {
            // Windows cannot rename over an existing file.
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::rename(temp, path);
        }
    }
}

fn summary(value: &Value) -> String {
    format!("openOMSI support package (schema 1)\n\n{}\n\nLogs are privacy projections, not raw log copies. Null means unavailable.\nRoots and archives are aliases; no absolute paths or archive filenames are exported.\nLoaded pack tracking is not available. Settings are allowlisted; no environment, command line, chat, player identity, configuration files or serial numbers are attached.\n", serde_json::to_string_pretty(value).unwrap_or_default())
}

fn archive(out: &Path, value: &Value, logs: &[(String, String)], performance: Option<&Value>) -> anyhow::Result<()> {
    // create_new: a mistaken filename must never overwrite an unrelated file.
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let result = (|| -> anyhow::Result<()> {
        zip.start_file("diagnostics.txt", opts)?;
        zip.write_all(summary(value).as_bytes())?;
        zip.start_file("diagnostics.json", opts)?;
        zip.write_all(&serde_json::to_vec_pretty(value)?)?;
        for (name, text) in logs {
            zip.start_file(format!("logs/{name}"), opts)?;
            zip.write_all(log_projection(text).as_bytes())?;
        }
        if let Some(performance) = performance {
            zip.start_file("performance/performance.json", opts)?;
            zip.write_all(&serde_json::to_vec_pretty(performance)?)?;
            zip.start_file("performance/performance-summary.txt", opts)?;
            zip.write_all(format!("Sanitized performance capture summary\n{}\n",
                serde_json::to_string_pretty(&performance["summary"])?).as_bytes())?;
        }
        Ok(())
    })();
    let result = result.and_then(|_| zip.finish().map(|_| ()).map_err(Into::into));
    if result.is_err() { let _ = std::fs::remove_file(out); }
    result
}

/// The caller supplies trusted in-process metadata. Only fixed app log basenames
/// are read; no directory recursion and no user's configuration is attached.
pub(crate) fn export(out: &Path, value: &Value) -> anyhow::Result<()> {
    let dir = omsi_launcher_lib::data_dir();
    let mut value = value.clone();
    let recent = omsi_launcher_lib::list_instances().into_iter().max_by_key(|i| i.started);
    if let Some(instance) = &recent {
        let path = dir.join(format!("support-session-{}.json", instance.pid));
        if let Ok(file) = std::fs::File::open(path) {
            let mut bytes = Vec::new();
            file.take(64 * 1024).read_to_end(&mut bytes)?;
            if let Ok(mut game) = serde_json::from_slice::<Value>(&bytes) {
                // Cache files are not raw attachments. Unknown keys are discarded,
                // and all remaining strings pass the same privacy filter again.
                filter_cached(&mut game);
                clean_json(&mut game);
                value["last_recorded_game"] = game;
            }
        }
    }
    let game_log = recent.as_ref().filter(|i| i.slot > 1 && i.slot <= 128)
        .map(|i| format!("game-{}.log", i.slot)).unwrap_or_else(|| "game.log".into());
    let logs: Vec<_> = [game_log.as_str(), "launcher.log", "crash.log"].iter()
        .filter_map(|name| tail(&dir.join(name)).ok().map(|text| (name.to_string(), text))).collect();
    let performance = recent.as_ref().and_then(|i| {
        let path = dir.join(format!("performance-{}/performance.json", i.pid));
        let file = std::fs::File::open(path).ok()?;
        if file.metadata().ok()?.len() > 64 * 1024 * 1024 { return None; }
        let raw: Value = serde_json::from_reader(file.take(64 * 1024 * 1024)).ok()?;
        performance_projection(&raw)
    });
    archive(out, &value, &logs, performance.as_ref())
}

/// Copy numeric capture fields only. Dynamic stage keys are a fixed catalog;
/// arbitrary text files and user-supplied metadata never enter the ZIP.
fn performance_projection(raw: &Value) -> Option<Value> {
    if raw.get("schema_version")?.as_u64()? != 1 { return None; }
    const KEYS: &[&str] = &["schema_version", "requested_seconds", "warmup_seconds", "summary",
        "frames", "measured_seconds", "average_fps", "p50_ms", "p95_ms", "p99_ms", "worst_frame_ms",
        "frames_over_16_7_ms", "frames_over_33_3_ms", "frames_over_50_ms", "frames_over_100_ms",
        "frame_limit_reached", "stage_average_ms", "frame_number", "timestamp_seconds", "frame_time_ms",
        "stages_ms", "graphics", "msaa", "ssao", "render_scale", "shadows", "lan_active",
        "memory_start", "memory_end", "physical_memory_bytes", "gpu_textures_bytes", "gpu_meshes_bytes"];
    const STAGES: &[&str] = &["acquire", "present", "gpu", "streaming", "traffic", "traffic.populate",
        "traffic.schedule", "traffic.tick", "traffic.audio", "traffic.sync", "player", "player.hover",
        "lan", "humans", "lights.atlas", "lights.collect", "lights.rain", "lights.spray",
        "lights.ambience", "lights+rain", "scripted", "scripted.boards", "hud", "hud.navigator",
        "mirrors", "render", "limiter", "setup", "prepare", "ray tracing", "shadow items", "cull",
        "items", "upload", "bundles", "encode", "finish", "submit", "finish.shadow", "finish.prepass",
        "finish.main", "finish.wait shadow", "finish.wait prepass"];
    fn numeric(value: &Value, stages: bool) -> Value {
        match value {
            Value::Number(_) | Value::Bool(_) | Value::Null => value.clone(),
            Value::Array(a) => Value::Array(a.iter().take(120_000).map(|v| numeric(v, false)).collect()),
            Value::Object(o) => Value::Object(o.iter().filter_map(|(k, v)| {
                let allowed = if stages {
                    let name = k.strip_prefix("render.").unwrap_or(k);
                    STAGES.contains(&name.strip_prefix("mirror.").unwrap_or(name))
                } else { KEYS.contains(&k.as_str()) };
                if !allowed || v.is_string() { return None; }
                if stages && !v.is_number() { return None; }
                Some((k.clone(), numeric(v, k == "stages_ms" || k == "stage_average_ms")))
            }).collect()),
            Value::String(_) => Value::Null,
        }
    }
    Some(numeric(raw, false))
}

fn filter_cached(value: &mut Value) {
    const KEYS: &[&str] = &["schema_version", "recorded_unix_seconds", "source", "version", "build",
        "commit", "build_unix_seconds", "os", "architecture", "os_version", "graphics", "backend",
        "adapter", "vendor_id", "device_id", "driver", "driver_info", "device_features", "device_limits",
        "max_texture_dimension_2d", "max_buffer_size", "max_storage_buffer_binding_size", "max_bind_groups",
        "max_sampled_textures_per_shader_stage", "max_storage_buffers_per_shader_stage", "msaa", "ssao",
        "render_scale", "shadow_size", "basic_pipelines", "device_lost", "graphics_settings", "preset",
        "backend_requested", "shadows", "mirrors", "mirror_size", "reflections", "vsync", "max_fps",
        "session", "map", "bus", "line", "tour", "content_roots", "mounted_archives", "alias", "priority",
        "files", "bytes", "loaded_content_packs", "controllers", "name", "vid_pid", "axes", "buttons",
        "gamepad", "ffb_capable", "ffb_effect_available", "physical_memory_bytes", "adapter_vram_mb",
        "adapter_texture_mb", "gpu_memory_bytes", "textures", "meshes"];
    match value {
        Value::Object(o) => { o.retain(|k, _| KEYS.contains(&k.as_str())); o.values_mut().for_each(filter_cached); }
        Value::Array(a) => a.iter_mut().for_each(filter_cached),
        _ => {}
    }
}

pub(crate) fn default_output() -> PathBuf {
    let dir = omsi_launcher_lib::data_dir().join("diagnostics");
    let _ = std::fs::create_dir_all(&dir);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_nanos()).unwrap_or(0);
    dir.join(format!("openomsi-support-{stamp}.zip"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_redact_personal_paths_addresses_and_credentials() {
        for s in ["/home/isaac/private.txt", "C:\\Users\\Isaac\\Desktop\\private.txt",
            "\\\\server\\share\\private.txt", "192.168.0.1", "2001:db8::1",
            "https://user:password@server", "Bearer abcd", "cookie=abcd",
            "Discord credentials", "serial ABC123", "api_key=abcd", "token abcd"] {
            assert_eq!(redact_label(s), "[redacted]", "{s}");
        }
        assert_eq!(redact_label("AMD Radeon RX 6750 XT"), "AMD Radeon RX 6750 XT");
        assert_eq!(content_name("Vehicles/Bus/vehicle.bus"), "Vehicles/Bus/vehicle.bus");
        assert_eq!(content_name("maps/../../private.txt"), "[redacted]");
    }

    #[test]
    fn cached_snapshot_is_filtered_again_before_export() {
        let mut v = json!({"password": "123456", "unknown_filename.txt": "private",
            "graphics": {"adapter": "C:\\Users\\Isaac\\gpu", "vendor_id": 4098, "cookie": "SECRET"}});
        filter_cached(&mut v);
        clean_json(&mut v);
        assert_eq!(v, json!({"graphics": {"adapter": "[redacted]", "vendor_id": 4098}}));
    }

    #[test]
    fn log_projection_never_copies_free_text_even_in_known_events() {
        let input = "renderer: /home/isaac/private.txt 192.168.0.1 cookie=SECRET\n\
            profile gpu: password SECRET 123\nsecret continuation line\n\
            stutter: frame 42 took 100 ms (render 80)\n\
            renderer: unrelated-personal-filename.txt\n";
        let out = log_projection(input);
        for private in ["isaac", "private.txt", "192.168", "cookie", "SECRET", "123", "unrelated-personal"] {
            assert!(!out.contains(private), "{out}");
        }
        assert!(out.contains("stutter [42.0, 100.0]"));
    }

    #[test]
    fn archive_has_only_fixed_entries_and_refuses_overwrite() {
        let path = std::env::temp_dir().join(format!("omsi-support-test-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&path);
        archive(&path, &json!({"schema_version": 1}), &[("game.log".into(), "chat: SECRET".into())], None).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(z.len(), 3);
        assert_eq!(z.by_index(0).unwrap().name(), "diagnostics.txt");
        assert_eq!(z.by_index(1).unwrap().name(), "diagnostics.json");
        let mut log = String::new();
        z.by_name("logs/game.log").unwrap().read_to_string(&mut log).unwrap();
        assert!(!log.contains("SECRET"));
        assert!(archive(&path, &Value::Null, &[], None).is_err());
        drop(z);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn capture_projection_rejects_unknown_keys_and_all_free_text() {
        let raw = json!({"schema_version": 1, "password": 123456,
            "summary": {"average_fps": 60, "frames": "secret", "stage_average_ms": {
                "render.mirror.cull": 1, "unrelated-personal.txt": 999, "gpu": "token"}},
            "frames": [{"frame_number": 1, "graphics": {"lan_active": true, "msaa": 4,
                "mirror_refresh": "C:\\Users\\Isaac\\private"}}]});
        let safe = performance_projection(&raw).unwrap();
        assert_eq!(safe["summary"]["stage_average_ms"], json!({"render.mirror.cull": 1}));
        assert!(safe.get("password").is_none());
        assert!(safe["summary"].get("frames").is_none());
        let text = safe.to_string();
        for s in ["secret", "Isaac", "token", "unrelated-personal"] { assert!(!text.contains(s)); }
        assert!(performance_projection(&json!({"schema_version": 2})).is_none());
    }
}
