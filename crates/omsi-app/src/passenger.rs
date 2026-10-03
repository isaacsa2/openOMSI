//! Passenger controls run on the bus owner's game, then its ordinary LAN state carries
//! the result. A remote AI copy never decides whether a real door may open.

use crate::{App, humans::BusId};
use glam::{DVec3, Vec3};
use omsi_sim::VehicleInstance;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action { Stop, Control(usize, usize) }

impl Action {
    fn parse(text: &str) -> Option<Self> {
        let fields: Vec<_> = text.split_whitespace().collect();
        match fields.as_slice() {
            ["stop"] => Some(Self::Stop),
            ["control", part, mesh] => Some(Self::Control(part.parse().ok()?, mesh.parse().ok()?)),
            _ => None,
        }
    }
    fn command(self) -> String {
        match self {
            Self::Stop => "passenger stop".into(),
            Self::Control(part, mesh) => format!("passenger control {part} {mesh}"),
        }
    }
}

pub(crate) fn passenger_event(event: &str) -> bool {
    let event = event.to_ascii_lowercase();
    if ["cockpit", "driver", "starter", "ignition", "brake", "throttle"].iter().any(|s| event.contains(s)) {
        return false;
    }
    ["haltewunsch", "stop_request", "request_stop", "campainha", "bell", "buzzer",
        "door_request", "door_button", "tuerwunsch", "pedido_parada",
        "passenger_window", "pax_window", "salon_window", "saloon_window"]
        .iter().any(|s| event.contains(s))
}

fn stop_event(program: &omsi_script::Program) -> Option<&'static str> {
    // The same event as AI passengers, rather than the driver's stop/brake switch.
    ["int_haltewunsch", "passenger_stop_request", "stop_request", "request_stop", "campainha"]
        .into_iter().find(|event| program.trigger(event).is_some())
}

fn passenger_aboard(pose: &omsi_net::Pose, owner: u32) -> Option<omsi_net::Aboard> {
    pose.walker?.aboard.filter(|a| a.owner == owner && a.local.iter().all(|v| v.is_finite()))
}

fn control_event(vehicle: &VehicleInstance, part: usize, def_index: usize, eye: DVec3) -> Option<String> {
    let (ty, position, props, transform) = if part == 0 {
        let i = vehicle.ty.meshes.iter().position(|m| m.def_index == def_index)?;
        (&vehicle.ty, vehicle.position, vehicle.mesh_props.get(i)?, (i, vehicle.mesh_local_transform(i)))
    } else {
        let rear = vehicle.trailers.get(part - 1)?;
        let i = rear.ty.meshes.iter().position(|m| m.def_index == def_index)?;
        (&rear.ty, rear.position, rear.mesh_props.get(i)?, (i, rear.mesh_local_transform(i)))
    };
    let event = ty.model.meshes.get(def_index)?.mouse_event.as_deref()?;
    if !props.visible || !passenger_event(event) { return None; }
    let (i, xf) = transform;
    let (center, radius) = *ty.mesh_bounds.get(i)?;
    let scale = xf.x_axis.truncate().length().max(xf.y_axis.truncate().length()).max(xf.z_axis.truncate().length());
    let radius = radius * scale;
    let distance = (eye - (position + xf.transform_point3(center).as_dvec3())).length();
    // Passenger buttons and window handles, not an entire draggable cockpit panel.
    if !distance.is_finite() || !radius.is_finite() || radius > 0.75 || distance > 2.0 + radius as f64 {
        return None;
    }
    Some(event.to_string())
}

fn pulse(vehicle: &mut VehicleInstance, event: &str) -> bool {
    let fired = vehicle.trigger(event);
    if fired { vehicle.trigger(&format!("{event}_off")); }
    fired
}

impl App {
    fn passenger_sounds(&mut self, sounds: &[String]) {
        let Some(lan) = self.lan.as_mut() else { return };
        for event in sounds.iter().take(8) {
            lan.command(0, &format!("passenger-sound {event}"));
        }
    }

    fn passenger_send(&mut self, owner: u32, action: Action) {
        let Some(lan) = self.lan.as_mut().filter(|l| l.connected) else { return };
        let now = Instant::now();
        if self.remotes.passenger_actions.get(&0).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(1)) { return; }
        self.remotes.passenger_actions.insert(0, now);
        lan.command(owner, &action.command());
        self.service_msg = Some(("Passenger request sent".into(), 3.0));
    }

    pub(crate) fn passenger_stop(&mut self) {
        match self.foot_bus() {
            Some(BusId::Player) => {
                let mut sounds = Vec::new();
                let accepted = self.player.as_mut().is_some_and(|p| {
                    stop_event(&p.vehicle.ty.program).is_some_and(|event| {
                        let before = p.vehicle.host.fired_triggers.len();
                        let accepted = pulse(&mut p.vehicle, event);
                        sounds.extend_from_slice(&p.vehicle.host.fired_triggers[before..]);
                        accepted
                    })
                });
                if accepted { self.passenger_sounds(&sounds); }
                self.service_msg = Some((if accepted { "Stop requested" } else { "This bus has no supported stop request button" }.into(), 3.0));
            }
            Some(BusId::Ai(id)) => {
                if let Some(owner) = crate::humans::remote_bus_player(id) {
                    self.passenger_send(owner, Action::Stop);
                }
            }
            None => self.service_msg = Some(("Board a bus before requesting a stop".into(), 3.0)),
        }
    }

    fn passenger_control(&self) -> Option<(u32, Action, String)> {
        let BusId::Ai(id) = self.foot_bus()? else { return None };
        let owner = crate::humans::remote_bus_player(id)?;
        let (eye, direction, spread) = self.cursor_ray_now()?;
        let remote = self.remotes.remotes.get(&owner).filter(|r| !r.stand_in)?;
        let vehicle = remote.vehicle();
        let hit = crate::player::pick_in(vehicle, eye, direction, spread)
            .map(|i| (0, vehicle.ty.meshes[i].def_index))
            .or_else(|| crate::player::pick_trailer_in(vehicle, eye, direction, spread)
                .map(|(part, i)| (part + 1, vehicle.trailers[part].ty.meshes[i].def_index)));
        let (part, mesh) = hit?;
        let event = control_event(vehicle, part, mesh, eye)?;
        Some((owner, Action::Control(part, mesh), event))
    }

    pub(crate) fn passenger_hover(&self) -> Option<String> {
        self.passenger_control().map(|(_, _, event)| event)
    }

    pub(crate) fn passenger_click(&mut self, pressed: bool) {
        if pressed {
            if let Some((owner, action, _)) = self.passenger_control() {
                self.passenger_send(owner, action);
            }
        }
    }

    pub(crate) fn passenger_command(&mut self, from: u32, text: &str) -> bool {
        if let Some(event) = text.strip_prefix("passenger-sound ") {
            if let Some(remote) = self.remotes.remotes.get_mut(&from) { remote.passenger_sound(event); }
            return true;
        }
        if let Some(result) = text.strip_prefix("passenger-result ") {
            let owner = self.foot_bus().and_then(|b| match b { BusId::Ai(id) => crate::humans::remote_bus_player(id), _ => None });
            if owner == Some(from) {
                let msg = match result {
                    "ok" => "Passenger request accepted",
                    "outside" => "Board the bus before using its passenger controls",
                    _ => "This bus does not support that passenger control",
                };
                self.service_msg = Some((msg.into(), 3.0));
            }
            return true;
        }
        let Some(command) = text.strip_prefix("passenger ") else { return false };
        let Some(action) = Action::parse(command) else { return true };
        let Some(lan) = self.lan.as_ref() else { return true };
        let aboard = lan.peers().find(|p| p.pose.id == from)
            .and_then(|p| passenger_aboard(&p.pose, lan.my_id));
        let mut sounds = Vec::new();
        let result = if let Some(aboard) = aboard {
            let now = Instant::now();
            if self.remotes.passenger_actions.get(&from).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(1)) { return true; }
            self.remotes.passenger_actions.insert(from, now);
            let local = Vec3::from_array(aboard.local);
            let eye = self.humans.as_ref().and_then(|h| h.cabin_world(BusId::Player, local)).map(|w| w.0)
                .or_else(|| self.player.as_ref().map(|p| p.vehicle.position + p.vehicle.body_rotation().transform_point3(local).as_dvec3()))
                .map(|feet| feet + DVec3::Z * 1.4);
            let accepted = self.player.as_mut().is_some_and(|p| {
                let event = match action {
                    Action::Stop => stop_event(&p.vehicle.ty.program).map(str::to_string),
                    Action::Control(part, mesh) => eye.and_then(|at| control_event(&p.vehicle, part, mesh, at)),
                };
                event.is_some_and(|event| {
                    let before = p.vehicle.host.fired_triggers.len();
                    let accepted = pulse(&mut p.vehicle, &event);
                    sounds.extend_from_slice(&p.vehicle.host.fired_triggers[before..]);
                    accepted
                })
            });
            if accepted { "ok" } else { "unsupported" }
        } else { "outside" };
        if result == "ok" { self.passenger_sounds(&sounds); }
        if let Some(lan) = self.lan.as_mut() { lan.command(from, &format!("passenger-result {result}")); }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vehicle() -> VehicleInstance {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("openomsi-passenger-{}-{}", std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("passenger.osc");
        std::fs::write(&path, "{trigger:int_haltewunsch}\n1 (S.L.haltewunsch) 1 (S.L.button) (T.L.stop_bell)\n{end}\n{trigger:int_haltewunsch_off}\n0 (S.L.button)\n{end}\n").unwrap();
        let program = omsi_script::compile(&omsi_script::CompileInput {
            builtin_vars: vec!["haltewunsch".into(), "button".into(), "window_open".into()], scripts: vec![path], ..Default::default()
        });
        assert!(program.errors.is_empty(), "{:?}", program.errors);
        std::fs::remove_dir_all(&dir).unwrap();
        let mut model = omsi_model::Model::default();
        model.meshes.push(omsi_model::MeshDef { mouse_event: Some("int_haltewunsch".into()),
            animations: vec![omsi_model::Animation { variable: "window_open".into(), ..Default::default() }],
            ..Default::default() });
        let ty = omsi_sim::VehicleType {
            def: Default::default(), model, model_dir: Default::default(), program: std::sync::Arc::new(program),
            meshes: vec![omsi_sim::vehicle::VehicleMesh {
                def_index: 0, data: Default::default(), file: Default::default(), materials: vec![], overrides: vec![],
                pivot: glam::Mat4::IDENTITY, viewpoint: 0, skin: vec![], keep_winding: false,
            }],
            paint_schemes: vec![], texchanges: vec![], wheel_meshes: vec![], suspension_axles: vec![], missing_packs: vec![],
            mesh_bounds: vec![(Vec3::new(0.0, 0.0, 1.4), 0.1)], mesh_boxes: vec![(Vec3::ZERO, Vec3::ONE)],
        };
        VehicleInstance::new(std::sync::Arc::new(ty), omsi_sim::VehicleHost::new(Default::default()))
    }

    #[test]
    fn a_stop_request_runs_the_bus_script_rings_and_releases_the_button() {
        let mut vehicle = test_vehicle();
        let event = stop_event(&vehicle.ty.program).unwrap();
        assert!(pulse(&mut vehicle, event));
        assert_eq!(vehicle.var("haltewunsch"), Some(1.0));
        assert_eq!(vehicle.var("button"), Some(0.0));
        assert_eq!(vehicle.host.fired_triggers, vec!["stop_bell"]);
    }

    #[test]
    fn a_control_must_exist_be_visible_and_be_within_passenger_reach() {
        let mut vehicle = test_vehicle();
        let eye = DVec3::new(0.0, 0.0, 1.4);
        assert_eq!(control_event(&vehicle, 0, 0, eye), Some("int_haltewunsch".into()));
        assert_eq!(control_event(&vehicle, 0, 0, eye + DVec3::Y * 10.0), None);
        assert_eq!(control_event(&vehicle, 0, usize::MAX, eye), None);
        assert_eq!(control_event(&vehicle, 1, 0, eye), None);
        vehicle.mesh_props[0].visible = false;
        assert_eq!(control_event(&vehicle, 0, 0, eye), None);
    }

    #[test]
    fn passenger_control_animation_is_in_the_network_sync_table() {
        let vehicle = test_vehicle();
        let table = crate::lan::SyncTable::new(&vehicle.ty, &[]);
        assert!(table.values.iter().any(|(name, _)| name == "window_open"));
    }

    #[test]
    fn only_a_passenger_of_this_bus_can_request_a_control() {
        let mut pose = omsi_net::Pose::default();
        assert!(passenger_aboard(&pose, 1).is_none());
        pose.walker = Some(omsi_net::Walker { aboard: Some(omsi_net::Aboard { owner: 1, local: [0.0; 3], seat: None }), ..Default::default() });
        assert!(passenger_aboard(&pose, 1).is_some());
        assert!(passenger_aboard(&pose, 2).is_none());
        pose.walker.as_mut().unwrap().aboard.as_mut().unwrap().local[0] = f32::NAN;
        assert!(passenger_aboard(&pose, 1).is_none());
    }

    #[test]
    fn passenger_commands_only_accept_known_actions_and_valid_indices() {
        assert_eq!(Action::parse("stop"), Some(Action::Stop));
        assert_eq!(Action::parse("control 0 12"), Some(Action::Control(0, 12)));
        for bad in ["stop extra", "trigger engine_start", "control -1 3", "control 0 NaN", "control 0 3 extra"] {
            assert_eq!(Action::parse(bad), None);
        }
    }

    #[test]
    fn passenger_controls_exclude_driving_switches() {
        for event in ["int_haltewunsch", "door_request_2", "campainha", "pax_window_1"] {
            assert!(passenger_event(event), "{event}");
        }
        for event in ["engine_start", "cockpit_haltewunsch", "driver_window", "brake", "bus_doorfront", "ignition"] {
            assert!(!passenger_event(event), "{event}");
        }
    }

    #[test]
    fn stop_request_discovers_a_declared_event_and_prefers_the_passenger_event() {
        let mut program = omsi_script::Program::default();
        assert_eq!(stop_event(&program), None);
        program.triggers.insert("campainha".into(), 0);
        assert_eq!(stop_event(&program), Some("campainha"));
        program.triggers.insert("int_haltewunsch".into(), 1);
        assert_eq!(stop_event(&program), Some("int_haltewunsch"));
    }
}
