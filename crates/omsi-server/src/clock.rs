//! Simulation clock and timetable sync that is independent of the graphical
//! application. Keep wall-clock and simulated time separate for accurate TPS.

use omsi_lan::admin::ServerAdmin;
use omsi_sim::world_clock::WorldClock;
use std::time::Instant;

/// Advance the simulation clock and optionally correct real-time synchronization.
pub fn sync_clock(
    clock: &mut f64,
    wall: &mut WorldClock,
    admin: &mut ServerAdmin,
    dt: f64,
    speed: f64,
    initial: f64,
    real_time_seconds: Option<f64>,
) {
    *clock += wall.step(Instant::now(), dt, speed);
    if let Some(real) = real_time_seconds {
        let have = (initial + *clock + admin.shift).rem_euclid(86_400.0);
        let offset = (real - have + 43_200.0).rem_euclid(86_400.0) - 43_200.0;
        if offset.abs() > 0.5 {
            admin.shift += offset;
        }
    }
}

pub fn follow_server_clock(day_time: f64, want: f64) -> (f64, Option<f64>) {
    let jump = want - day_time;
    (want, (jump.abs() >= 120.0).then_some(jump))
}


#[cfg(test)]
mod tests {
    use super::follow_server_clock;

    #[test]
    fn the_traffic_follows_the_servers_clock_and_a_jump_restarts_the_timetable() {
        assert_eq!(follow_server_clock(54_000.0, 54_000.5), (54_000.5, None));
        assert_eq!(follow_server_clock(54_000.0, 54_119.0), (54_119.0, None));
        assert_eq!(follow_server_clock(54_000.0, 94_560.0), (94_560.0, Some(40_560.0)));
    }
}
