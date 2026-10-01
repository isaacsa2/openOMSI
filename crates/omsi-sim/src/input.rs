//! Input actions: the names from `Inputs/keyboard.cfg` and how they reach a vehicle.
//!
//! Vehicle actions are script triggers with the same name; on key release the trigger
//! `<name>_off` fires. A few actions are handled by the engine itself (throttle, brake,
//! clutch, steering, views).

/// Actions the engine handles instead of forwarding to the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineAction {
    Throttle,
    ThrottleAmplify,
    Brake,
    Clutch,
    SteeringLeft,
    SteeringRight,
    SteeringNeutral,
}

pub fn engine_action(name: &str) -> Option<EngineAction> {
    Some(match name.to_ascii_lowercase().as_str() {
        "throttle" => EngineAction::Throttle,
        "throttle_amplify" => EngineAction::ThrottleAmplify,
        "brake" => EngineAction::Brake,
        "clutch" => EngineAction::Clutch,
        "steering_left" => EngineAction::SteeringLeft,
        "steering_right" => EngineAction::SteeringRight,
        "steering_neutral" => EngineAction::SteeringNeutral,
        _ => return None,
    })
}

/// Keyboard-driven analogue inputs, integrated per frame like the original (keys ramp the
/// pedal/steering position instead of setting it).
#[derive(Debug, Clone, Default)]
pub struct KeyboardAxes {
    pub throttle_key: bool,
    pub amplify_key: bool,
    pub brake_key: bool,
    pub clutch_key: bool,
    pub left_key: bool,
    pub right_key: bool,
    pub neutral_key: bool,
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steering: f32,
    /// Road speed in km/h: the steering returns to centre by itself only while rolling.
    pub speed_kmh: f32,
    /// Rate of the steering wheel (fraction per second) while it swings back on its own.
    pub steer_vel: f32,
    /// "Steering linearity": the keys turn the wheel as Omsi.exe does (0x7e64c6): the
    /// curvature changes by 0.00005 per millisecond whatever the speed, so the wheel goes at
    /// one steady pace; `lock_curvature` (`[inv_min_turnradius]`) turns that into a share
    /// of the lock.
    pub linear: bool,
    /// OMSI's Dynamic Wheel Speed: only keyboard steering is slowed as road speed rises.
    /// It changes how fast the requested steering moves, never the available steering lock.
    pub dynamic_wheel_speed: bool,
    /// "Old Steering": let go, the wheel stays where it is and is turned back by hand - OMSI
    /// without `[autoCenter]`.
    pub old_steering: bool,
    pub lock_curvature: f32,
    /// OMSI's held brake (the default): let go, the brake stays where the key left it until
    /// the throttle key is pressed - tap the brake and it keeps that pressure. Off, the brake
    /// comes off with its key, as in most games. (The throttle never stays: see `update`.)
    pub pedal_hold: bool,
    /// The steering is on its way back to the middle (Omsi.exe +0x5d6): set by the
    /// `steering_neutral` key, cleared by a steering key.
    pub centering: bool,
}

impl KeyboardAxes {
    pub fn set(&mut self, action: EngineAction, pressed: bool) {
        match action {
            EngineAction::Throttle => self.throttle_key = pressed,
            EngineAction::ThrottleAmplify => self.amplify_key = pressed,
            EngineAction::Brake => self.brake_key = pressed,
            EngineAction::Clutch => self.clutch_key = pressed,
            EngineAction::SteeringLeft => self.left_key = pressed,
            EngineAction::SteeringRight => self.right_key = pressed,
            EngineAction::SteeringNeutral => self.neutral_key = pressed,
        }
    }

    /// Let go of every key: the window losing focus (alt-tab, a click outside it, an OS
    /// dialog) never delivers the matching key-up, so without this a throttle or steering
    /// key held at that moment stayed "pressed" forever (and, with a modifier key stuck the
    /// same way, a later plain key press could be misread as held with that modifier).
    pub fn release_all(&mut self) {
        *self = KeyboardAxes {
            throttle: self.throttle,
            brake: self.brake,
            clutch: self.clutch,
            steering: self.steering,
            speed_kmh: self.speed_kmh,
            linear: self.linear,
            dynamic_wheel_speed: self.dynamic_wheel_speed,
            old_steering: self.old_steering,
            lock_curvature: self.lock_curvature,
            pedal_hold: self.pedal_hold,
            centering: self.centering,
            ..Default::default()
        };
    }

    pub fn update(&mut self, dt: f32) {
        // The pedals as Omsi.exe works them from the keys (key handler sub_7e614c, frame
        // sub_7d5124): the throttle key raises the throttle at 2 a second up to 0.85 - to
        // the floor only with throttle_amplify held - and takes the brake off at once; let
        // go, the throttle falls at 1 a second. The brake key raises the brake at 1 a second
        // and takes the throttle off; let go, the brake stays where it is until the throttle
        // key is pressed, or eases off at 0.5 a second while throttle_amplify is held.
        let top = if self.amplify_key { 1.0 } else { 0.85 };
        if self.throttle_key {
            self.brake = 0.0;
            self.throttle = (self.throttle + 2.0 * dt).min(top);
        } else {
            self.throttle = (self.throttle - dt).max(0.0);
        }
        if self.brake_key {
            self.throttle = 0.0;
            self.brake = (self.brake + dt).min(1.0);
        } else if !self.pedal_hold {
            self.brake = (self.brake - 3.0 * dt).max(0.0);
        } else if self.amplify_key {
            self.brake = (self.brake - 0.5 * dt).max(0.0);
        }
        // The clutch as Omsi.exe works it from a key (0x7e648f, 0x7d59c0): pressed, the
        // pedal is down at once; let go, it comes up at 0.7 a second - a foot letting the
        // clutch in, which is what makes a gear change on the keyboard smooth.
        if self.clutch_key {
            self.clutch = 1.0;
        } else {
            self.clutch = (self.clutch - 0.7 * dt).max(0.0);
        }
        // Keyboard steering. Dynamic Wheel Speed is deliberately only the rate at which a
        // steering key moves the wheel: it never shortens the available lock. At parking
        // speed the keyboard can turn briskly; as road speed rises, the same held key moves
        // progressively more slowly. Auto-centring keeps its separate castor-like curve.
        let v = self.speed_kmh.abs();
        let (rate, back) = if self.linear {
            // OMSI's linear mode: 0.05 of curvature a second regardless of road speed.
            let r = (0.05 / self.lock_curvature.max(0.01)).clamp(0.05, 5.0);
            (r, r)
        } else {
            let dynamic_rate = 0.8 / (1.0 + v / 45.0);
            let rate = if self.dynamic_wheel_speed { dynamic_rate } else { 0.8 };
            // Keep automatic return separate from Dynamic Wheel Speed. It is weak while
            // standing and grows as the front axle starts rolling.
            let back_base = 0.8 / (1.0 + v / 45.0);
            let back = back_base * (0.25 + 0.75 * (v / 25.0).min(1.0));
            (rate, back)
        };
        if self.neutral_key {
            self.centering = true;
        }
        if self.left_key || self.right_key {
            self.centering = false;
        }
        if self.left_key && self.right_key {
            // Both directions held: hold the wheel exactly where it is. Releasing either
            // key immediately hands control to the still-held direction.
            self.steer_vel = 0.0;
        } else if self.left_key || self.right_key {
            let direction = if self.left_key { -1.0 } else { 1.0 };
            let mut left = dt.max(0.0);
            // Counter-steering is intentionally quicker only until the wheel reaches centre;
            // any frame time left after crossing zero continues at the normal (possibly
            // Dynamic Wheel Speed) rate, so the boost cannot make full-lock inputs twitchy.
            if self.steering * direction < 0.0 {
                let counter_rate = rate * 1.7;
                let to_centre = self.steering.abs() / counter_rate.max(0.001);
                if to_centre >= left {
                    self.steering += direction * counter_rate * left;
                    left = 0.0;
                } else {
                    self.steering = 0.0;
                    left -= to_centre;
                }
            }
            if left > 0.0 {
                self.steering += direction * rate * left;
            }
            self.steering = self.steering.clamp(-1.0, 1.0);
            self.steer_vel = 0.0;
        } else if self.centering {
            // Num 5 / steering_neutral: a deliberate quick return. It is still progressive,
            // never a teleport to zero, and remains latched after a tap until the wheel is
            // centred or a direction key is pressed.
            let centre_rate = (rate * 2.0).max(1.25);
            let step = centre_rate * dt;
            self.steering -= self.steering.clamp(-step, step);
            self.steer_vel = 0.0;
        } else if self.old_steering {
            // Old Steering: the wheel stays where the hands left it
            self.steer_vel = 0.0;
        } else {
            // Released: the wheel comes back at `back`, easing out over the last bit so that
            // it settles instead of stopping dead in the middle.
            let ease = (self.steering.abs() / 0.08).clamp(0.3, 1.0);
            let step = back * ease * dt;
            self.steering -= self.steering.clamp(-step, step);
            self.steer_vel = 0.0;
            // (a snap from farther out was a visible jolt of the wheel and the driver's hands)
            if self.steering.abs() < 0.0003 {
                self.steering = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_and_old_steering_as_omsi() {
        let mut a = KeyboardAxes { linear: true, old_steering: true, lock_curvature: 0.1, speed_kmh: 50.0, ..Default::default() };
        a.right_key = true;
        for _ in 0..100 {
            a.update(0.01);
        }
        // 0.05 1/m a second against a lock of 0.1 1/m: half the lock in a second, at any speed
        assert!((a.steering - 0.5).abs() < 1e-3, "{}", a.steering);
        a.right_key = false;
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.steering - 0.5).abs() < 1e-3, "old steering: it stays");
        a.old_steering = false;
        for _ in 0..50 {
            a.update(0.01);
        }
        assert!((a.steering - 0.25).abs() < 0.02, "it comes back at the same pace: {}", a.steering);
    }

    /// Omsi.exe's keyboard pedals: the brake stays until the throttle key, the throttle
    /// goes up to 0.85 (1 with throttle_amplify) and comes back by itself.
    #[test]
    fn pedals_as_omsi_works_them_from_the_keys() {
        let mut a = KeyboardAxes { pedal_hold: true, ..Default::default() };
        a.brake_key = true;
        for _ in 0..30 {
            a.update(0.01);
        }
        assert!((a.brake - 0.3).abs() < 1e-3, "1 a second: {}", a.brake);
        a.brake_key = false;
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.brake - 0.3).abs() < 1e-3, "the brake stays: {}", a.brake);
        a.throttle_key = true;
        a.update(0.01);
        assert_eq!(a.brake, 0.0);
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.throttle - 0.85).abs() < 1e-3, "up to 0.85: {}", a.throttle);
        a.amplify_key = true;
        for _ in 0..20 {
            a.update(0.01);
        }
        assert!((a.throttle - 1.0).abs() < 1e-3, "amplified: {}", a.throttle);
        a.amplify_key = false;
        a.throttle_key = false;
        for _ in 0..50 {
            a.update(0.01);
        }
        assert!((a.throttle - 0.5).abs() < 1e-3, "falls at 1 a second: {}", a.throttle);
        // the other way: the brake comes off with its key
        let mut b = KeyboardAxes { pedal_hold: false, brake: 0.6, ..Default::default() };
        for _ in 0..10 {
            b.update(0.01);
        }
        assert!((b.brake - 0.3).abs() < 1e-3, "{}", b.brake);
    }

    /// The centring key is a quick but progressive return and stays active after a tap.
    #[test]
    fn steering_neutral_returns_quickly_without_snapping() {
        let mut a = KeyboardAxes { old_steering: true, steering: 0.8, ..Default::default() };
        a.neutral_key = true;
        a.update(0.1);
        a.neutral_key = false;
        assert!(a.steering < 0.8 && a.steering > 0.5, "progressive: {}", a.steering);
        for _ in 0..6 {
            a.update(0.1);
        }
        assert_eq!(a.steering, 0.0);
        // A direction key cancels the latched centring and Old Steering then keeps it there.
        a.steering = 0.5;
        a.right_key = true;
        a.update(0.01);
        a.right_key = false;
        let held = a.steering;
        a.update(0.5);
        assert!((a.steering - held).abs() < 1e-6, "old steering stays: {}", a.steering);
    }

    #[test]
    fn the_clutch_goes_down_at_once_and_comes_up_slowly() {
        let mut a = KeyboardAxes::default();
        a.clutch_key = true;
        a.update(0.01);
        assert_eq!(a.clutch, 1.0);
        a.clutch_key = false;
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.clutch - 0.3).abs() < 1e-3, "{}", a.clutch);
    }

    #[test]
    fn dynamic_wheel_speed_slows_keyboard_steering_only_by_speed() {
        let step = 1.0 / 60.0;
        let mut parked = KeyboardAxes { dynamic_wheel_speed: true, ..Default::default() };
        parked.set(EngineAction::SteeringRight, true);
        for _ in 0..60 { parked.update(step); }

        let mut city = KeyboardAxes { dynamic_wheel_speed: true, speed_kmh: 30.0, ..Default::default() };
        city.set(EngineAction::SteeringRight, true);
        for _ in 0..60 { city.update(step); }

        let mut fast = KeyboardAxes { dynamic_wheel_speed: true, speed_kmh: 80.0, ..Default::default() };
        fast.set(EngineAction::SteeringRight, true);
        for _ in 0..60 { fast.update(step); }

        assert!((parked.steering - 0.8).abs() < 0.02, "parked {}", parked.steering);
        assert!(city.steering < parked.steering && city.steering > 0.4, "city {}", city.steering);
        assert!(fast.steering < city.steering && fast.steering > 0.2, "fast {}", fast.steering);

        let mut off = KeyboardAxes { dynamic_wheel_speed: false, speed_kmh: 80.0, ..Default::default() };
        off.set(EngineAction::SteeringRight, true);
        for _ in 0..60 { off.update(step); }
        assert!((off.steering - parked.steering).abs() < 0.02, "off {} parked {}", off.steering, parked.steering);
    }

    #[test]
    fn holding_both_steering_keys_holds_the_wheel() {
        let mut a = KeyboardAxes { dynamic_wheel_speed: true, speed_kmh: 40.0, steering: 0.42, ..Default::default() };
        a.left_key = true;
        a.right_key = true;
        a.update(1.0);
        assert!((a.steering - 0.42).abs() < 1e-6, "both keys moved it: {}", a.steering);

        a.left_key = false;
        a.update(0.2);
        assert!(a.steering > 0.42, "releasing left did not hand over to right: {}", a.steering);
    }

    #[test]
    fn opposite_key_countersteers_faster_until_centre() {
        let mut boosted = KeyboardAxes { dynamic_wheel_speed: true, speed_kmh: 50.0, steering: -0.6, ..Default::default() };
        boosted.right_key = true;
        boosted.update(1.0);
        assert!(boosted.steering > 0.0, "counter-steer did not cross centre: {}", boosted.steering);

        let mut normal = KeyboardAxes { dynamic_wheel_speed: true, speed_kmh: 50.0, steering: 0.0, ..Default::default() };
        normal.right_key = true;
        normal.update(0.1);
        assert!(normal.steering > 0.0);
        // Once already on the requested side there is no boost.
        let before = normal.steering;
        normal.update(0.1);
        assert!((normal.steering - before) < 0.05, "normal turn unexpectedly boosted");
    }
}
