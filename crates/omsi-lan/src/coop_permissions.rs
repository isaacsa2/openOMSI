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

/// A conductor may work only physical passenger-cabin controls. Ticket sales
/// require a separate, authoritative fare transaction protocol (not raw triggers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CabControl<'a> {
    Door(usize),
    Window(&'a str),
}

/// Deliberately narrow: a mod's window trigger must actually exist in its
/// compiled program; generic trigger prefixes are never arbitrary script access.
pub fn passenger_window_trigger(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let valid_prefix = [
        "window_",
        "bus_window",
        "fenster_",
        "bus_fenster",
        "fahrerfenster",
        "bus_fahrerfenster",
        "cp_fahrerfenster",
    ];
    let forbidden = [
        "motor",
        "engine",
        "steer",
        "brake",
        "gear",
        "throttle",
        "handbrake",
    ];
    !name.is_empty()
        && name.len() <= 80
        && name.is_ascii()
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        && valid_prefix.iter().any(|prefix| lower.starts_with(prefix))
        && !lower.ends_with("_off")
        && !forbidden.iter().any(|part| lower.contains(part))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CabGrant {
    pub actor: u32,
    pub connection: u64,
    pub session: u64,
    pub last_seq: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct CabRequest<'a> {
    pub actor: u32,
    pub connection: u64,
    pub session: u64,
    pub seq: u64,
    pub inside_owners_bus: bool,
    pub bus_present: bool,
    pub action: CabControl<'a>,
}

impl CabGrant {
    pub fn authorize_cab(
        &mut self,
        request: CabRequest<'_>,
        known_window: bool,
        door_count: usize,
    ) -> bool {
        let valid_control = match request.action {
            CabControl::Door(index) => (1..=door_count.min(8)).contains(&index),
            CabControl::Window(name) => known_window && passenger_window_trigger(name),
        };
        if self.actor == 0
            || self.actor != request.actor
            || self.connection != request.connection
            || self.session != request.session
            || !request.inside_owners_bus
            || !request.bus_present
            || request.seq <= self.last_seq
            || !valid_control
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

    #[test]
    fn cabin_controls_require_a_specific_grant_and_verified_bus_occupancy() {
        let grant = CabGrant {
            actor: 7,
            connection: 80,
            session: 91,
            last_seq: 0,
        };
        let request = CabRequest {
            actor: 7,
            connection: 80,
            session: 91,
            seq: 1,
            inside_owners_bus: true,
            bus_present: true,
            action: CabControl::Door(1),
        };
        let mut accepted = grant;
        assert!(accepted.authorize_cab(request, false, 3));
        assert!(!accepted.authorize_cab(request, false, 3), "replay");
        let rejected = |request| {
            let mut current = grant;
            !current.authorize_cab(request, false, 3)
        };
        assert!(rejected(CabRequest { actor: 8, ..request }));
        assert!(rejected(CabRequest { connection: 81, ..request }));
        assert!(rejected(CabRequest { session: 92, ..request }));
        assert!(rejected(CabRequest {
            inside_owners_bus: false,
            ..request
        }));
        assert!(rejected(CabRequest {
            action: CabControl::Door(9),
            ..request
        }));
        for name in [
            "window_driver",
            "bus_window_1",
            "fenster_2",
            "bus_fensterfahrer",
            "cp_fahrerfenster",
        ] {
            assert!(passenger_window_trigger(name));
            let mut current = grant;
            assert!(current.authorize_cab(
                CabRequest { action: CabControl::Window(name), ..request },
                true,
                3,
            ));
        }
        for name in [
            "engine_start",
            "bus_doorfront0",
            "window_handbrake",
            "window_driver_off",
            "window_0\\ntrigger drive",
        ] {
            assert!(!passenger_window_trigger(name));
        }
        let mut current = grant;
        assert!(!current.authorize_cab(
            CabRequest { action: CabControl::Window("window_driver"), ..request },
            false,
            3,
        ));
    }

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
