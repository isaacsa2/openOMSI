//! Optional co-op conductor: a peer may work only the current owner's bus
//! door/window controls after a visible per-player grant. The owner executes
//! script triggers locally; no bus scripts, snapshots or files are transferred.

use crate::App;
use omsi_lan::coop_permissions::{CabControl, CabGrant, CabRequest, passenger_window_trigger};

impl App {
    /// A room switch or disconnect never keeps a conductor role.
    pub(crate) fn coop_tick(&mut self) {
        let Some(lan) = self.net.lan.as_ref().filter(|lan| lan.connected || lan.role == omsi_net::Role::Host) else {
            self.net.conductor_grant = None;
            self.net.conductor_owner = None;
            self.net.conductor_seq = 0;
            return;
        };
        let session = lan.session;
        if self.net.conductor_grant.is_some_and(|g| g.session != session) {
            self.net.conductor_grant = None;
        }
        if self.net.conductor_owner.is_some_and(|(_, epoch)| epoch != session) {
            self.net.conductor_owner = None;
            self.net.conductor_seq = 0;
        }
    }

    /// Called only by the owner selecting a connected passenger in the player list.
    pub(crate) fn coop_toggle_conductor(&mut self, actor: u32, connection: u64) {
        let Some(lan) = self.net.lan.as_mut() else { return };
        if self.player.is_none() { return; }
        let inside = lan.peers().any(|peer| {
            peer.has_info && peer.has_pose
                && peer.pose.id == actor
                && peer.conn == connection
                && peer.pose.walker.and_then(|w| w.aboard).is_some_and(|aboard| aboard.owner == lan.my_id)
        });
        if !inside { return; }
        let previous = self.net.conductor_grant.take();
        if let Some(old) = previous {
            lan.command(old.actor, "coop-role 0");
            if old.actor == actor && old.connection == connection && old.session == lan.session {
                self.service_msg = Some(("Conductor permission revoked".into(), 3.0));
                return;
            }
        }
        self.net.conductor_grant = Some(CabGrant {
            actor,
            connection,
            session: lan.session,
            last_seq: 0,
        });
        lan.command(actor, "coop-role 1");
        self.service_msg = Some(("Conductor may now operate passenger doors and windows".into(), 4.0));
    }

    /// The guest requests a door/window action through the ordinary authenticated
    /// LAN CMD channel. The owner never trusts the guest's asserted role.
    pub(crate) fn coop_send_control(&mut self, owner: u32, kind: &str, arg: &str) {
        let Some(lan) = self.net.lan.as_mut() else { return };
        if self.net.conductor_owner != Some((owner, lan.session))
            || self.net.inside_remote != Some(owner)
            || !matches!(kind, "door" | "window")
        {
            return;
        }
        self.net.conductor_seq = self.net.conductor_seq.wrapping_add(1).max(1);
        lan.command(
            owner,
            &format!("coop-control {} {kind} {arg}", self.net.conductor_seq),
        );
    }

    /// A trusted session supplies the sender ID for CMD, not the packet text.
    /// Only the current physical owner of the bus executes the granted action.
    pub(crate) fn coop_command(&mut self, from: u32, text: &str) {
        let Some(lan) = self.net.lan.as_ref() else { return };
        if matches!(text, "coop-role 1" | "coop-role 0") {
            let aboard = lan.peers().any(|peer| peer.pose.id == from && peer.has_info && peer.has_pose)
                && self.net.inside_remote == Some(from);
            if text == "coop-role 1" && aboard {
                self.net.conductor_owner = Some((from, lan.session));
                self.net.conductor_seq = 0;
                self.service_msg = Some(("Conductor role granted: use the driver's player menu".into(), 4.0));
            } else if self.net.conductor_owner.is_some_and(|(owner, _)| owner == from) {
                self.net.conductor_owner = None;
                self.service_msg = Some(("Conductor role ended".into(), 3.0));
            }
            return;
        }
        let Some(words) = text.strip_prefix("coop-control ") else { return };
        let mut split = words.split_whitespace();
        let Some(seq) = split.next().and_then(|v| v.parse::<u64>().ok()) else { return };
        let kind = split.next().unwrap_or("");
        let value = split.next().unwrap_or("");
        if split.next().is_some() { return; }
        let action = match kind {
            "door" => match value.parse::<usize>() {
                Ok(index) => CabControl::Door(index),
                _ => return,
            },
            "window" => CabControl::Window(value),
            _ => return,
        };
        let Some(peer) = lan.peers().find(|peer| {
            peer.has_info && peer.has_pose && peer.pose.id == from
        }) else { return };
        let inside = peer.pose.walker.and_then(|w| w.aboard)
            .is_some_and(|aboard| aboard.owner == lan.my_id);
        let Some(player) = self.player.as_mut() else { return };
        let door_count = omsi_sim::door_keys::door_keys(&player.vehicle.ty).len();
        let window_known = matches!(action, CabControl::Window(name)
            if player.vehicle.ty.program.trigger(name).is_some() && passenger_window_trigger(name));
        let Some(grant) = self.net.conductor_grant.as_mut() else { return };
        if !grant.authorize_cab(
            CabRequest {
                actor: from,
                connection: peer.conn,
                session: lan.session,
                seq,
                inside_owners_bus: inside,
                bus_present: true,
                action,
            },
            window_known,
            door_count,
        ) {
            log::debug!("LAN: unauthorized conductor control from player {from}");
            return;
        }
        match action {
            CabControl::Door(index) => {
                let fired = player.door_key(index);
                player.door_key_off(&fired);
            }
            CabControl::Window(name) => {
                player.vehicle.trigger(name);
                let off = format!("{name}_off");
                if player.vehicle.ty.program.trigger(&off).is_some() {
                    player.vehicle.trigger(&off);
                }
            }
        }
    }
}
