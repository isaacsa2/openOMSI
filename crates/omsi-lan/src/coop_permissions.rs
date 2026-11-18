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
