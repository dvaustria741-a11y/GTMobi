//! Vehicle drivetrain and steering logic.
//!
//! Ported from Defy (https://github.com/openfw-game/defy, MIT, (c) 2024 FOSS Supremacy),
//! files scripts/gameplay/vehicles/VehicleEngine.gd and VehicleSteering.gd.
//! See THIRD_PARTY_NOTICES.md. Pure functions of (state, input, dt); no engine types.
//!
//! Units: meters, seconds, km/h only where noted. Steering sign: positive = left.

#[derive(Clone, Debug, PartialEq)]
pub struct EngineConfig {
    /// Max RPM per gear, index = gear. Source: scene-exported array, no default in the script.
    pub gears: Vec<u32>,
    /// Source default 100.0.
    pub engine_power: f32,
    /// Reverse speed cap in km/h. Source default 10.
    pub reverse_speed_cap_kmh: f32,
    /// Seconds between gear changes. Source default 0.5.
    pub shift_delay_s: f32,
    /// Source default 10.
    pub brake_force: f32,
    /// Source default 0.
    pub handbrake_force: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    NoGears,
    InvalidValue,
}

impl EngineConfig {
    /// Gears must be supplied by the caller; the source script has no default.
    pub fn with_gears(gears: Vec<u32>) -> Result<Self, ConfigError> {
        let cfg = Self {
            gears,
            engine_power: 100.0,
            reverse_speed_cap_kmh: 10.0,
            shift_delay_s: 0.5,
            brake_force: 10.0,
            handbrake_force: 0.0,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.gears.is_empty() {
            return Err(ConfigError::NoGears);
        }
        let finite_nonneg = |v: f32| v.is_finite() && v >= 0.0;
        if !(finite_nonneg(self.engine_power)
            && finite_nonneg(self.reverse_speed_cap_kmh)
            && finite_nonneg(self.shift_delay_s)
            && finite_nonneg(self.brake_force)
            && finite_nonneg(self.handbrake_force))
        {
            return Err(ConfigError::InvalidValue);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriveInput {
    /// Accelerator 0..1.
    pub accelerate: f32,
    /// Brake / reverse pedal 0..1.
    pub brake: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriveOutput {
    /// Force to apply to the vehicle's drive wheels.
    pub engine_force: f32,
    pub brake_force: f32,
    pub rpm: u32,
    pub gear: usize,
}

#[derive(Clone, Debug)]
pub struct Engine {
    cfg: EngineConfig,
    gear: usize,
    rpm: u32,
    clock_s: f32,
    last_shift_s: f32,
    /// Signed speed along the vehicle's forward axis from the previous step, m/s.
    prev_forward_speed: f32,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Result<Self, ConfigError> {
        cfg.validate()?;
        Ok(Self { cfg, gear: 0, rpm: 0, clock_s: 0.0, last_shift_s: 0.0, prev_forward_speed: 0.0 })
    }

    pub fn gear(&self) -> usize {
        self.gear
    }

    /// `speed_mps` is |velocity|; `forward_speed_mps` is signed, positive when moving forward.
    /// The source uses the previous step's forward speed and RPM; that order is kept.
    /// Returns `None` for non-finite inputs or dt (state is left untouched).
    pub fn step(
        &mut self,
        input: DriveInput,
        speed_mps: f32,
        forward_speed_mps: f32,
        dt: f32,
    ) -> Option<DriveOutput> {
        if ![input.accelerate, input.brake, speed_mps, forward_speed_mps, dt]
            .iter()
            .all(|v| v.is_finite())
            || dt < 0.0
            || speed_mps < 0.0
        {
            return None;
        }
        self.clock_s += dt;
        let accelerate = input.accelerate.clamp(0.0, 1.0);
        let brake = input.brake.clamp(0.0, 1.0);
        let throttle = accelerate - brake;
        let speed_kmh = (speed_mps * 3.6) as i32 as f32;

        // EngineForce(): cut drive at the gear's RPM limit, or when reversing past the cap.
        let top_rpm = self.cfg.gears[self.gear];
        let cut = self.rpm >= top_rpm
            || (self.prev_forward_speed < 0.0 && speed_kmh >= self.cfg.reverse_speed_cap_kmh);

        // GearController()
        let since_shift = self.clock_s - self.last_shift_s;
        if throttle != 0.0 {
            if self.rpm >= top_rpm && self.gear + 1 < self.cfg.gears.len() && since_shift > self.cfg.shift_delay_s {
                self.gear += 1;
                self.last_shift_s = self.clock_s;
            }
        } else if (self.rpm as f32) < top_rpm as f32 * 0.8 && self.gear > 0 && since_shift > self.cfg.shift_delay_s {
            self.gear -= 1;
            self.last_shift_s = self.clock_s;
        }

        // RPM proxy from speed, clamped to the (possibly new) gear's limit.
        let raw_rpm = (speed_mps * 60.0 / (2.0 * std::f32::consts::PI)) as u32;
        self.rpm = raw_rpm.min(self.cfg.gears[self.gear]);

        let mut force = throttle * self.cfg.engine_power;
        if brake > 0.1 {
            force -= self.cfg.brake_force / 2.0;
        }
        let brake_total = self.cfg.brake_force * brake + self.cfg.handbrake_force;

        self.prev_forward_speed = forward_speed_mps;
        Some(DriveOutput {
            engine_force: if cut { 0.0 } else { force },
            brake_force: brake_total,
            rpm: self.rpm,
            gear: self.gear,
        })
    }
}

/// Wheel order: front-left, front-right, back-left, back-right.
pub const WHEEL_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SteeringConfig {
    /// Source default 10.0.
    pub steering_speed: f32,
    /// Source default 2.0.
    pub suspension_damping: f32,
    /// Source default 0.05.
    pub max_suspension_shift: f32,
    /// Source constant 0.15.
    pub base_rest_length: f32,
    /// Source default 1.5.
    pub tire_grip: f32,
    /// Source threshold 3 m/s below which no lean is applied.
    pub lean_min_speed: f32,
}

impl Default for SteeringConfig {
    fn default() -> Self {
        Self {
            steering_speed: 10.0,
            suspension_damping: 2.0,
            max_suspension_shift: 0.05,
            base_rest_length: 0.15,
            tire_grip: 1.5,
            lean_min_speed: 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SteeringState {
    pub steering: f32,
    pub rest_length: [f32; WHEEL_COUNT],
}

pub fn move_toward(current: f32, target: f32, max_delta: f32) -> f32 {
    if (target - current).abs() <= max_delta {
        target
    } else {
        current + (target - current).signum() * max_delta
    }
}

/// Unlike the source, the interpolation factor is clamped to 0..1 so a long frame cannot overshoot.
pub fn lerp_clamped(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

impl SteeringState {
    pub fn new(cfg: &SteeringConfig) -> Self {
        Self { steering: 0.0, rest_length: [cfg.base_rest_length; WHEEL_COUNT] }
    }

    /// `steer_input`: -1..1, positive = left (source: get_axis(right, left)).
    /// `lean_left`/`lean_right` mirror the source's pressed-key tests; right wins if both.
    /// Returns false (state untouched) on non-finite input.
    pub fn step(&mut self, cfg: &SteeringConfig, steer_input: f32, speed_mps: f32, dt: f32) -> bool {
        if !(steer_input.is_finite() && speed_mps.is_finite() && dt.is_finite()) || dt < 0.0 {
            return false;
        }
        let steer = steer_input.clamp(-1.0, 1.0);
        let mut shift = 0.0;
        if speed_mps > cfg.lean_min_speed {
            if steer < 0.0 {
                shift = -cfg.max_suspension_shift;
            } else if steer > 0.0 {
                shift = cfg.max_suspension_shift;
            }
        }
        let t = dt * cfg.suspension_damping;
        let targets = [
            cfg.base_rest_length + shift,
            cfg.base_rest_length - shift,
            cfg.base_rest_length + shift,
            cfg.base_rest_length - shift,
        ];
        for (rest, target) in self.rest_length.iter_mut().zip(targets) {
            *rest = lerp_clamped(*rest, target, t);
        }
        self.steering = move_toward(self.steering, steer, dt * cfg.steering_speed);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::new(EngineConfig::with_gears(vec![20, 40, 60]).unwrap()).unwrap()
    }

    const GAS: DriveInput = DriveInput { accelerate: 1.0, brake: 0.0 };

    #[test]
    fn empty_gears_rejected() {
        assert_eq!(EngineConfig::with_gears(vec![]), Err(ConfigError::NoGears));
    }

    #[test]
    fn upshift_waits_for_delay_and_rpm_limit() {
        let mut e = engine();
        // 5 m/s -> rpm proxy ~47, clamped to gear 0 limit 20, so the next step wants an upshift.
        let first = e.step(GAS, 5.0, 5.0, 0.1).unwrap();
        assert_eq!(first.rpm, 20);
        assert_eq!(first.gear, 0, "shift delay (0.5 s) not elapsed yet");
        let mut gear = 0;
        for _ in 0..5 {
            gear = e.step(GAS, 5.0, 5.0, 0.1).unwrap().gear;
        }
        assert_eq!(gear, 1);
    }

    #[test]
    fn drive_is_cut_at_rpm_limit() {
        let mut e = engine();
        e.step(GAS, 5.0, 5.0, 0.01).unwrap();
        let out = e.step(GAS, 5.0, 5.0, 0.01).unwrap();
        assert_eq!(out.engine_force, 0.0);
    }

    #[test]
    fn reverse_cap_cuts_drive() {
        let mut e = engine();
        let rev = DriveInput { accelerate: 0.0, brake: 1.0 };
        e.step(rev, 4.0, -4.0, 0.01).unwrap(); // 14 km/h while reversing
        let out = e.step(rev, 4.0, -4.0, 0.01).unwrap();
        assert_eq!(out.engine_force, 0.0);
    }

    #[test]
    fn non_finite_input_is_rejected_without_state_change() {
        let mut e = engine();
        assert!(e.step(GAS, f32::NAN, 0.0, 0.1).is_none());
        assert!(e.step(GAS, 1.0, 0.0, f32::INFINITY).is_none());
        assert_eq!(e.gear(), 0);
    }

    #[test]
    fn steering_moves_toward_target_at_limited_rate() {
        let cfg = SteeringConfig::default();
        let mut s = SteeringState::new(&cfg);
        assert!(s.step(&cfg, 1.0, 0.0, 0.05));
        assert!((s.steering - 0.5).abs() < 1e-6);
        assert!(s.step(&cfg, 1.0, 0.0, 1.0));
        assert_eq!(s.steering, 1.0);
    }

    #[test]
    fn lean_only_above_speed_threshold_and_converges() {
        let cfg = SteeringConfig::default();
        let mut s = SteeringState::new(&cfg);
        s.step(&cfg, 1.0, 1.0, 0.1);
        assert_eq!(s.rest_length, [cfg.base_rest_length; WHEEL_COUNT]);
        for _ in 0..200 {
            s.step(&cfg, 1.0, 10.0, 0.1);
        }
        assert!((s.rest_length[0] - (cfg.base_rest_length + cfg.max_suspension_shift)).abs() < 1e-4);
        assert!((s.rest_length[1] - (cfg.base_rest_length - cfg.max_suspension_shift)).abs() < 1e-4);
    }
}
