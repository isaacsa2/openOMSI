//! Exercise the actual renderer-free executable with two independent LAN
//! clients over UDP. Unlike in-process HostTransport tests, this catches
//! bootstrap, socket lifetime and cross-process protocol regressions.
//! Synthetic fixture only: it does not prove bus-script/gameplay parity.

use omsi_net::{LanSession, Pose, WorldInfo};
use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("file parent")).unwrap();
    fs::write(path, content).unwrap();
}

fn unused_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct FixtureProcess {
    root: PathBuf,
    process: Child,
}

impl Drop for FixtureProcess {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn start_fixture() -> (FixtureProcess, SocketAddr) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openomsi-native-udp-{}-{nanos}",
        std::process::id()
    ));
    let map = root.join("Maps/Fixture");
    write(
        &map.join("global.cfg"),
        "[name]\nHeadless Fixture\n[map]\n0\n0\ntile_0_0.map\n",
    );
    write(
        &map.join("tile_0_0.map"),
        "[version]\n10\n[spline]\n0\nSplines/smoke.sli\n101\n-1\n10\n0\n10\n0\n40\n0\n0\n0\n0\n0\n0\n",
    );
    write(
        &root.join("Splines/smoke.sli"),
        "[length]\n40\n[texture]\nroad.bmp\n[profile]\n0\n[profilepnt]\n-1.5\n0\n0\n1\n[profilepnt]\n1.5\n0\n1\n1\n[path]\n0\n0\n0\n3\n0\n",
    );
    let port = unused_port();
    let mut web_port = unused_port();
    while web_port == port {
        web_port = unused_port();
    }
    let cfg = root.join("server-test.cfg");
    write(
        &cfg,
        &format!(
            "name = Native UDP Test\nmap = Maps/Fixture/global.cfg\ndate = 1989-05-30\ntime = 09:00\ntraffic = 0\ntimetable = 0\npassengers = 0\nport = {port}\nweb_port = {web_port}\nmax_players = 2\ntunnel = 0\npublic_server = 0\npublic_address = off\nreal_time = 0\nmetar_sync = 0\naccept_uploads = 0\nshare_maps = 0\nparked_buses = 0\nvoice = 0\nadmin_web = off\n"
        ),
    );
    let process = Command::new(env!("CARGO_BIN_EXE_omsi-server"))
        .arg(&root)
        .arg(&cfg)
        .arg("--experimental-free-roam")
        .arg("--ticks")
        .arg("1800")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start actual native headless server");
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    (FixtureProcess { root, process }, addr)
}

fn driver(name: &str, x: f64) -> Pose {
    Pose {
        name: name.into(),
        bus: format!("Vehicles/{name}.bus"),
        flags: omsi_net::FLAG_VEHICLE,
        x,
        y: 10.0,
        ..Default::default()
    }
}

#[test]
fn native_binary_relays_two_clients_without_a_host_vehicle() {
    let (mut server, addr) = start_fixture();
    let world = WorldInfo {
        map: "Maps/Fixture/global.cfg".into(),
        date: "1989-05-30".into(),
        time: 9.0 * 3600.0,
        ..Default::default()
    };
    let mut alpha = LanSession::join_addr(vec![addr], None, "alpha", world.clone()).unwrap();
    let mut beta = LanSession::join_addr(vec![addr], None, "beta", world).unwrap();
    let pa = driver("alpha", 101.0);
    let pb = driver("beta", 202.0);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut relayed = false;
    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("native server exited before relay handshake: {status}");
        }
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if alpha.connected
            && beta.connected
            && alpha
                .peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "beta")
            && beta
                .peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "alpha")
        {
            relayed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        relayed,
        "two LAN sessions did not exchange real poses via the separate native process"
    );
    // The dedicated process never publishes a player-controlled bus of its
    // own; each client should see only the other connected driver.
    assert_eq!(alpha.peers().filter(|p| p.has_info && p.has_pose).count(), 1);
    assert_eq!(beta.peers().filter(|p| p.has_info && p.has_pose).count(), 1);
    alpha.leave();
    beta.leave();
}
