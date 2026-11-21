//! Real UDP integration checks for cancelled passenger GRANT ownership.

use super::super::*;
use std::net::SocketAddr;

fn connected_pair() -> (LanSession, LanSession) {
    let world = WorldInfo::default();
    let mut host = LanSession::host(0, "host", world.clone(), true).unwrap();
    let addr: SocketAddr = host.local_addr().unwrap();
    let mut client = LanSession::join_addr(vec![addr], None, "passenger-test", world).unwrap();
    let bus = Pose {
        bus: "Vehicles/Bus.bus".into(),
        flags: FLAG_VEHICLE,
        x: 15.0,
        ..Default::default()
    };
    for _ in 0..180 {
        host.tick(0.06, &Pose::default());
        client.tick(0.06, &bus);
        if client.connected && host.peers().any(|p| p.has_pose && p.has_info) {
            return (host, client);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("loopback peers never connected");
}

#[test]
fn a_cancelled_grant_returns_only_from_its_authenticated_owner() {
    let (mut host, mut client) = connected_pair();
    client.return_unboarded(&[42, 43]);
    for _ in 0..10 {
        host.tick(0.01, &Pose::default());
        let returns = host.take_pax_returns();
        if !returns.is_empty() {
            assert_eq!(returns, vec![(client.my_id, vec![42, 43])]);
            host.acknowledge_returns(client.my_id, &[42, 43]);
            for _ in 0..10 {
                client.tick(0.01, &Pose::default());
                let received = client.take_pax_acks();
                if !received.is_empty() {
                    assert_eq!(received, vec![42, 43]);
                    return;
                }
            }
            panic!("return acknowledgement not delivered");
        }
    }
    panic!("return request not delivered");
}

#[test]
fn physical_boarding_uses_a_distinct_confirmation_from_grant() {
    let (mut host, client) = connected_pair();
    client.confirm_boarded(&[100]);
    for _ in 0..10 {
        host.tick(0.01, &Pose::default());
        let boarding = host.take_pax_boarded();
        if !boarding.is_empty() {
            assert_eq!(boarding, vec![(client.my_id, vec![100])]);
            assert!(host.take_pax_returns().is_empty());
            return;
        }
    }
    panic!("boarding confirmation not delivered");
}
