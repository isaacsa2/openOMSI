//! Dedicated host networking without a graphical player, renderer or audio.
//!
//! The host's authoritative session has no local bus. It uses the same OMSI
//! protocol and admission, TCP content transport, timeouts and snapshots as
//! the existing game host. A world must finish loading before `ready()` is
//! called; until then new players wait instead of spawning in an empty map.

use omsi_net::{LanSession, Pose, WorldInfo};

pub struct HostTransport {
    session: LanSession,
    /// The host has no vehicle, no driver and therefore no bus model to load.
    host_pose: Pose,
}

impl HostTransport {
    /// Reserve UDP and TCP together. Passing port 0 chooses a free one.
    pub fn bind(
        port: u16,
        name: &str,
        world: WorldInfo,
        max_players: usize,
    ) -> std::io::Result<Self> {
        let mut session = LanSession::host(port, name, world, port == 0)?;
        session.set_peer_limit(max_players);
        session.set_starting(true);
        Ok(Self {
            session,
            host_pose: Pose::default(),
        })
    }

    /// Call this only once the CPU world, collisions, traffic and passenger
    /// simulation are fully installed. This is an explicit admission barrier.
    pub fn ready(&mut self) {
        self.session.set_starting(false);
    }

    /// The networking tick is independent of the render loop. It intentionally
    /// exposes no direct player-supplied script, disk or transfer operation.
    pub fn tick(&mut self, dt_seconds: f32) -> Vec<u32> {
        self.session.tick(dt_seconds, &self.host_pose)
    }

    pub fn session(&self) -> &LanSession {
        &self.session
    }

    /// Simulation-thread only. The existing LAN/admin layers keep all
    /// authorization and content-transfer policy enforcement.
    pub fn session_mut(&mut self) -> &mut LanSession {
        &mut self.session
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;

    fn world() -> WorldInfo {
        WorldInfo {
            map: "maps/Grundorf/global.cfg".into(),
            date: "1989-05-30".into(),
            time: 9.0 * 3600.0,
            weather: "Weather/Sommer.owt".into(),
            ..Default::default()
        }
    }

    /// Network-level regression: the headless process can host two ordinary
    /// clients and relay their vehicles, with no local bus or GPU initialized.
    #[test]
    fn two_clients_exchange_buses_through_a_host_without_a_player() {
        let expected = world();
        let mut host = HostTransport::bind(0, "Dedicated", expected.clone(), 8).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], host.session().local_addr().unwrap().port()));
        // This is the same admission barrier used while the CPU map is loading.
        host.ready();
        let mut a = LanSession::join_addr(vec![addr], None, "alpha", expected.clone()).unwrap();
        let mut b = LanSession::join_addr(vec![addr], None, "beta", expected.clone()).unwrap();
        let pa = Pose {
            bus: "Vehicles/Alpha.bus".into(),
            name: "alpha".into(),
            x: 101.0,
            ..Default::default()
        };
        let pb = Pose {
            bus: "Vehicles/Beta.bus".into(),
            name: "beta".into(),
            x: 202.0,
            ..Default::default()
        };
        let mut exchanged = false;
        for _ in 0..180 {
            host.tick(0.06);
            a.tick(0.06, &pa);
            b.tick(0.06, &pb);
            if a.connected
                && b.connected
                && host.session().peer_count() == 2
                && a.peers().any(|p| p.has_info && p.pose.name == "beta")
                && b.peers().any(|p| p.has_info && p.pose.name == "alpha")
            {
                exchanged = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            exchanged,
            "the headless host must relay two player vehicles"
        );
        assert_eq!(
            host.host_pose.bus, "",
            "a server must not allocate its own bus"
        );
        let welcome = a.welcome.as_ref().expect("server welcome");
        assert_eq!(welcome.world.map, expected.map);
        assert_eq!(welcome.world.date, expected.date);
        assert_eq!(welcome.world.weather, expected.weather);
    }

    #[test]
    fn zero_players_does_not_create_a_phantom_local_bus() {
        let mut host = HostTransport::bind(0, "Dedicated", world(), 1).unwrap();
        host.ready();
        for _ in 0..8 {
            host.tick(1.0 / 60.0);
        }
        assert_eq!(host.session().peer_count(), 0);
        assert!(host.host_pose.bus.is_empty());
        assert!(host.session().code().is_some());
    }
}
