//! Isolated **vehicle dynamics preview** using openOMSI's real VehiclePhysics.
//!
//! This is not the full game. Vehicle scripts, gearbox, ignition, engine state,
//! axles/suspension, UI, NPCs and collision processing still belong to omsi-app.
//! The preview applies temporary explicit drive/brake forces only so a real
//! .bus definition can move a model above a real terrain heightfield.

use std::path::Path;

use glam::{Mat4, Vec3};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DriverKeys {
    pub accelerate: bool,
    pub brake: bool,
    pub left: bool,
    pub right: bool,
}

pub struct DrivePreview {
    pub physics: omsi_sim::VehiclePhysics,
    /// Position in rebased preview map coordinates, XYZ metres.
    pub pos: Vec3,
    /// Clockwise degrees from world +Y, where 0 is north.
    pub yaw_deg: f32,
}

impl DrivePreview {
    pub fn from_bus_file(path: &Path, start: Vec3) -> Result<Self, String> {
        let bus = omsi_vehicle::Vehicle::load(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if bus.axles.is_empty() {
            return Err("bus definition has no axles for physics".into());
        }
        Ok(Self::from_vehicle(&bus, start))
    }

    pub fn from_vehicle(bus: &omsi_vehicle::Vehicle, start: Vec3) -> Self {
        Self {
            physics: omsi_sim::VehiclePhysics::from_definition(bus),
            pos: start,
            yaw_deg: 0.0,
        }
    }

    /// Advance the existing openOMSI kinematic vehicle physics, not a cloned
    /// engine. The torque and brake forces are **provisional** and are NOT
    /// derived from OMSI scripts. The actual game must supply script values.
    pub fn advance(&mut self, dt: f32, keys: DriverKeys, ground_height: Option<f32>) {
        let dt = dt.clamp(0.0, 0.1);
        let throttle = if keys.accelerate { 1.0 } else { 0.0 };
        let brake = if keys.brake { 1.0 } else { 0.0 };
        let steering = f32::from(keys.right as i8) - f32::from(keys.left as i8);
        self.physics.controls = omsi_sim::Controls {
            throttle, brake, clutch: 0.0, steering,
        };
        let braking = vec![brake * 9000.0; self.physics.wheels.len() * 2];
        // The provisional torque only exercises physics input wiring.
        let (displacement, heading) = self.physics.step(dt, 14_000.0 * throttle, &braking);
        self.yaw_deg = (self.yaw_deg + heading).rem_euclid(360.0);
        let angle = self.yaw_deg.to_radians();
        self.pos.x += displacement * angle.sin();
        self.pos.y += displacement * angle.cos();
        if let Some(ground) = ground_height.filter(|h| h.is_finite()) {
            self.pos.z = ground;
        }
    }

    pub fn model_transform(&self) -> [f32; 16] {
        let translation = Mat4::from_translation(self.pos);
        let rotation = Mat4::from_rotation_z(-self.yaw_deg.to_radians());
        (translation * rotation).to_cols_array()
    }

    pub fn kmh(&self) -> f32 {
        self.physics.velocity_kmh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_uses_original_physics_and_turns_with_forward_speed() {
        let mut bus = omsi_vehicle::Vehicle::default();
        bus.mass = 11000.0;
        bus.rolling_resistance = 50.;
        bus.axles = vec![
            omsi_vehicle::Axle { long: 2.5, driven: false, ..Default::default() },
            omsi_vehicle::Axle { long: -3.0, driven: true, ..Default::default() },
        ];
        bus.inv_min_turn_radius = 0.08;
        let mut sim = DrivePreview::from_vehicle(&bus, Vec3::ZERO);
        for _ in 0..100 {
            sim.advance(0.016, DriverKeys { accelerate: true, right: true, ..Default::default() }, Some(1.5));
        }
        assert!(sim.kmh() > 0.0);
        assert!(sim.yaw_deg > 0.0);
        assert!(sim.pos.y > 0.0);
        assert_eq!(sim.pos.z, 1.5);
    }

    #[test]
    fn no_input_does_not_move_stopped_vehicle() {
        let bus = omsi_vehicle::Vehicle::default();
        let mut sim = DrivePreview::from_vehicle(&bus, Vec3::new(4., 5., 6.));
        sim.advance(0.016, DriverKeys::default(), None);
        assert_eq!(sim.pos, Vec3::new(4., 5., 6.));
    }
}
