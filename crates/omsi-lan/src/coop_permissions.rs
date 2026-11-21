//! Authorization policy for planned optional cooperative roles.
//!
//! This module has no network side effects. The caller must authenticate peer identity
//! and apply a successful decision on the host before any vehicle or fare state changes.
//! Neither role is enabled by default.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoopRoles {
    pub driver_handover: bool,
    pub conductor: bool,
}

/// Facts provided by a verified LAN session when transferring driving authority.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriverRequest {
    pub same_session: bool,
    pub host_enabled: bool,
    pub owner_consented: bool,
    pub guest_consented: bool,
    pub guest_inside_bus: bool,
    pub bus_content_installed: bool,
    pub bus_speed_kmh: f32,
    pub authority_token: u64,
}

pub fn may_handover_driver(roles: CoopRoles, request: DriverRequest) -> bool {
    roles.driver_handover
        && request.same_session
        && request.host_enabled
        && request.owner_consented
        && request.guest_consented
        && request.guest_inside_bus
        && request.bus_content_installed
        && request.bus_speed_kmh.is_finite()
        && request.bus_speed_kmh.abs() <= 0.5
        && request.authority_token != 0
}

/// A narrow in-cabin request. It never transfers driving authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CabinAction {
    Door(usize),
    Window(usize),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CabinRequest {
    pub same_session: bool,
    pub owner_consented: bool,
    pub passenger_inside_bus: bool,
    pub speed_kmh: f32,
}

/// A role can operate door buttons only while the bus is stationary. Windows
/// are allowed while moving; neither grant includes engine, gearbox or steering.
pub fn may_operate_cabin(roles: CoopRoles, req: CabinRequest, action: CabinAction) -> bool {
    roles.conductor
        && req.same_session
        && req.owner_consented
        && req.passenger_inside_bus
        && req.speed_kmh.is_finite()
        && match action {
            CabinAction::Door(index) => index < 9 && req.speed_kmh.abs() <= 0.5,
            CabinAction::Window(index) => index < 9,
        }
}

/// Commands are routed through the authenticated LAN session, not chat text.
impl CabinAction {
    pub fn command(self) -> String {
        match self {
            Self::Door(index) => format!("coop-door {index}"),
            Self::Window(index) => format!("coop-window {index}"),
        }
    }
}

pub fn parse_cabin_action(text: &str) -> Option<CabinAction> {
    let mut words = text.split_whitespace();
    let kind = words.next()?;
    let index = words.next()?.parse::<usize>().ok()?;
    if words.next().is_some() || index >= 9 {
        return None;
    }
    match kind {
        "coop-door" => Some(CabinAction::Door(index)),
        "coop-window" => Some(CabinAction::Window(index)),
        _ => None,
    }
}

/// A selected assistant is the only peer allowed to use cabin controls.
/// Id zero and an absent assignment always fail closed.
pub fn matches_assistant(assigned: Option<u32>, sender: u32) -> bool {
    sender != 0 && assigned == Some(sender)
}

/// Only explicit model window controls can be delegated. Never interpret a
/// string from a remote peer as an arbitrary script/engine trigger.
pub fn cabin_window_trigger(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    n.len() <= 96
        && (n.contains("window") || n.contains("fenster"))
        && !n.ends_with("_off")
        && !n.ends_with("_drag")
        && !["engine", "motor", "steer", "brake", "door"]
            .iter()
            .any(|word| n.contains(word))
}

/// Translate a real model click to the existing restricted cabin wire actions.
/// The event must belong to one of this vehicle's mouse-event meshes; never run
/// arbitrary script triggers supplied by network peers.
pub fn cabin_action_for_model(ty: &omsi_sim::VehicleType, event: &str) -> Option<CabinAction> {
    if !ty
        .model
        .meshes
        .iter()
        .any(|mesh| mesh.mouse_event.as_deref() == Some(event))
    {
        return None;
    }
    if let Some(index) = omsi_sim::door_keys::door_keys(ty).iter().position(|group| {
        group
            .iter()
            .any(|name| name.split('|').any(|trigger| trigger == event))
    }) {
        return (index < 9).then_some(CabinAction::Door(index));
    }
    if !cabin_window_trigger(event) {
        return None;
    }
    let mut windows: Vec<&str> = ty
        .model
        .meshes
        .iter()
        .filter_map(|mesh| mesh.mouse_event.as_deref())
        .filter(|name| cabin_window_trigger(name))
        .collect();
    windows.sort_unstable();
    windows.dedup();
    windows
        .iter()
        .position(|name| *name == event)
        .filter(|index| *index < 9)
        .map(CabinAction::Window)
}

/// Ticket operations are narrower than generic in-cab triggers. Normal passenger
/// stop bells are outside this grant; engine, door and steering actions are never fares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FareAction {
    IssueTicket,
    GiveChange,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ConductorRequest {
    pub host_enabled: bool,
    pub owner_consented: bool,
    pub peer_inside_bus: bool,
    pub same_session: bool,
    pub actor: u32,
    pub epoch: u64,
    pub desk: u32,
    pub seq: u64,
}

/// The currently delegated desk transaction. The epoch must change for a new grant.
/// A command from an earlier ticket or a repeated sequence number is rejected.
#[derive(Clone, Copy, Debug, Default)]
pub struct FareGrant {
    pub actor: u32,
    pub epoch: u64,
    pub desk: u32,
    pub last_seq: u64,
}

impl FareGrant {
    pub fn authorize(
        &mut self,
        roles: CoopRoles,
        request: ConductorRequest,
        _action: FareAction,
    ) -> bool {
        if !roles.conductor
            || !request.host_enabled
            || !request.owner_consented
            || !request.peer_inside_bus
            || !request.same_session
            || request.actor == 0
            || request.actor != self.actor
            || request.epoch == 0
            || request.epoch != self.epoch
            || request.desk == 0
            || request.desk != self.desk
            || request.seq <= self.last_seq
        {
            return false;
        }
        self.last_seq = request.seq;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver() -> DriverRequest {
        DriverRequest {
            same_session: true,
            host_enabled: true,
            owner_consented: true,
            guest_consented: true,
            guest_inside_bus: true,
            bus_content_installed: true,
            bus_speed_kmh: 0.0,
            authority_token: 42,
        }
    }

    #[test]
    fn driving_authority_is_explicitly_opt_in_and_requires_a_stopped_bus() {
        let enabled = CoopRoles {
            driver_handover: true,
            ..Default::default()
        };
        assert!(!may_handover_driver(CoopRoles::default(), driver()));
        assert!(may_handover_driver(enabled, driver()));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                bus_speed_kmh: 1.0,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                bus_speed_kmh: f32::NAN,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                owner_consented: false,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                bus_content_installed: false,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                guest_inside_bus: false,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                authority_token: 0,
                ..driver()
            }
        ));
    }

    fn fare() -> ConductorRequest {
        ConductorRequest {
            host_enabled: true,
            owner_consented: true,
            peer_inside_bus: true,
            same_session: true,
            actor: 7,
            epoch: 123,
            desk: 11,
            seq: 1,
        }
    }

    #[test]
    fn cabin_controls_require_owner_opt_in_and_real_presence() {
        let roles = CoopRoles {
            conductor: true,
            ..Default::default()
        };
        let valid = CabinRequest {
            same_session: true,
            owner_consented: true,
            passenger_inside_bus: true,
            speed_kmh: 0.0,
        };
        assert!(!may_operate_cabin(
            CoopRoles::default(),
            valid,
            CabinAction::Door(0)
        ));
        assert!(may_operate_cabin(roles, valid, CabinAction::Door(0)));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                owner_consented: false,
                ..valid
            },
            CabinAction::Door(0)
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                passenger_inside_bus: false,
                ..valid
            },
            CabinAction::Window(0)
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                speed_kmh: 30.0,
                ..valid
            },
            CabinAction::Door(0)
        ));
        assert!(may_operate_cabin(
            roles,
            CabinRequest {
                speed_kmh: 30.0,
                ..valid
            },
            CabinAction::Window(0)
        ));
        assert!(!may_operate_cabin(roles, valid, CabinAction::Window(9)));
    }

    #[test]
    fn cabin_commands_cannot_be_used_to_inject_engine_or_steering_triggers() {
        assert_eq!(
            parse_cabin_action("coop-door 2"),
            Some(CabinAction::Door(2))
        );
        assert_eq!(
            parse_cabin_action("coop-window 0"),
            Some(CabinAction::Window(0))
        );
        for invalid in [
            "coop-door 9",
            "coop-door -1",
            "coop-door 0 engine_start",
            "trigger engine_start",
            "coop-window 100",
            "coop-open 0",
        ] {
            assert!(parse_cabin_action(invalid).is_none());
        }
        assert!(cabin_window_trigger("cp_fenster_rechts"));
        assert!(cabin_window_trigger("bus_window_left"));
        assert!(!cabin_window_trigger("window_engine_start"));
        assert!(!cabin_window_trigger("window_slider_drag"));
        assert!(!cabin_window_trigger("bus_doorfront0"));
    }

    #[test]
    #[test]
    fn generic_cabin_consent_cannot_authorize_other_players() {
        assert!(matches_assistant(Some(7), 7));
        assert!(!matches_assistant(Some(7), 8));
        assert!(!matches_assistant(Some(7), 0));
        assert!(!matches_assistant(None, 7));
    }

    fn ticket_grants_reject_replays_other_passengers_and_revoked_roles() {
        let enabled = CoopRoles {
            conductor: true,
            ..Default::default()
        };
        let grant = FareGrant {
            actor: 7,
            epoch: 123,
            desk: 11,
            last_seq: 0,
        };
        let mut g = grant;
        assert!(!g.authorize(CoopRoles::default(), fare(), FareAction::IssueTicket));
        assert_eq!(g.last_seq, 0);
        assert!(g.authorize(enabled, fare(), FareAction::IssueTicket));
        assert!(!g.authorize(enabled, fare(), FareAction::GiveChange));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                actor: 8,
                seq: 2,
                ..fare()
            },
            FareAction::IssueTicket
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                epoch: 124,
                seq: 2,
                ..fare()
            },
            FareAction::IssueTicket
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                desk: 12,
                seq: 2,
                ..fare()
            },
            FareAction::GiveChange
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                owner_consented: false,
                seq: 2,
                ..fare()
            },
            FareAction::GiveChange
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                peer_inside_bus: false,
                seq: 2,
                ..fare()
            },
            FareAction::GiveChange
        ));
        assert!(g.authorize(
            enabled,
            ConductorRequest { seq: 2, ..fare() },
            FareAction::GiveChange
        ));
    }
}
