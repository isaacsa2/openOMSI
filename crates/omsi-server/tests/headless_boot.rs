//! End-to-end native binary smoke test without a window, GPU or OMSI assets.
//! The synthetic content is used solely inside CI and is not redistributed.

use std::fs;
use std::path::Path;
use std::process::Command;

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn native_server_boots_a_cpu_map_and_stops_after_bounded_ticks() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openomsi-headless-{}-{nanos}",
        std::process::id()
    ));
    let map = root.join("Maps/Fixture");
    write(
        &map.join("global.cfg"),
        "[name]\nHeadless Fixture\n[map]\n0\n0\ntile_0_0.map\n",
    );
    // Tile format v10: detail level, definition, object ID, unlinked flag,
    // x/height/y, heading, length, radius, grades, cant, texture offset.
    write(
        &map.join("tile_0_0.map"),
        "[version]\n10\n[spline]\n0\nSplines/smoke.sli\n101\n-1\n10\n0\n10\n0\n40\n0\n0\n0\n0\n0\n0\n",
    );
    write(
        &root.join("Splines/smoke.sli"),
        "[length]\n40\n[texture]\nroad.bmp\n[profile]\n0\n[profilepnt]\n-1.5\n0\n0\n1\n[profilepnt]\n1.5\n0\n1\n1\n[path]\n0\n0\n0\n3\n0\n",
    );
    let cfg = root.join("server-test.cfg");
    write(
        &cfg,
        "name = CI Headless Fixture\nmap = Maps/Fixture/global.cfg\ndate = 1989-05-30\ntime = 09:00\ntraffic = 0\ntimetable = 0\npassengers = 0\nport = 0\nweb_port = 0\nmax_players = 2\ntunnel = 0\npublic_server = 0\npublic_address = off\nreal_time = 0\nmetar_sync = 0\naccept_uploads = 0\nshare_maps = 0\nparked_buses = 0\nvoice = 0\nadmin_web = off\n",
    );
    let result = Command::new(env!("CARGO_BIN_EXE_omsi-server"))
        .arg(&root)
        .arg(&cfg)
        .arg("--experimental-free-roam")
        .arg("--ticks")
        .arg("3")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("start standalone headless executable");
    let _ = fs::remove_dir_all(&root);
    assert!(
        result.status.success(),
        "native host failed with code {:?}\nstdout:\n{}\nstderr:\n{}",
        result.status.code(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("stopped cleanly after 3 ticks"),
        "headless host did not finish its bounded simulation: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
