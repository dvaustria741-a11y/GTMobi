//! Time-of-day lighting: sun direction and a keyframed sky/sun/ambient palette.
//!
//! Art-direction values are UNCALIBRATED and chosen by eye from the GTA 6 reference screenshots
//! (violet/magenta/orange dusk, teal accents). All colors are linear RGB, 0..1.
//! Coordinates: +Y up; the sun travels from +X (sunrise) over +Y to -X (sunset), tilted toward +Z.

use gtm_math::Vec3;

pub type Rgb = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyKey {
    /// Hour of day, 0..24, strictly increasing across the key list.
    pub hour: f32,
    pub sun_color: Rgb,
    pub sun_intensity: f32,
    pub ambient: Rgb,
    pub sky_zenith: Rgb,
    pub sky_horizon: Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyState {
    pub sun_dir_to_sun: Vec3,
    pub sun_color: Rgb,
    pub sun_intensity: f32,
    pub ambient: Rgb,
    pub sky_zenith: Rgb,
    pub sky_horizon: Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightingError {
    TooFewKeys,
    KeysNotAscending,
    HourOutOfRange,
    InvalidSunTimes,
}

#[derive(Clone, Debug)]
pub struct LightingConfig {
    keys: Vec<SkyKey>,
    /// Hours at which the sun crosses the horizon. Assumption, not sourced from the reference.
    pub sunrise_hour: f32,
    pub sunset_hour: f32,
    /// Tilt of the sun's path toward +Z in radians; 0 = straight east-west.
    pub path_tilt: f32,
}

fn c(hex: u32) -> Rgb {
    // sRGB hex -> linear
    let f = |v: u32| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    [f((hex >> 16) & 0xFF), f((hex >> 8) & 0xFF), f(hex & 0xFF)]
}

impl LightingConfig {
    pub fn new(keys: Vec<SkyKey>, sunrise_hour: f32, sunset_hour: f32, path_tilt: f32) -> Result<Self, LightingError> {
        if keys.len() < 2 {
            return Err(LightingError::TooFewKeys);
        }
        if keys.iter().any(|k| !(k.hour.is_finite() && (0.0..24.0).contains(&k.hour))) {
            return Err(LightingError::HourOutOfRange);
        }
        if keys.windows(2).any(|w| w[1].hour <= w[0].hour) {
            return Err(LightingError::KeysNotAscending);
        }
        if !(sunrise_hour.is_finite() && sunset_hour.is_finite() && path_tilt.is_finite())
            || sunrise_hour < 0.0
            || sunset_hour > 24.0
            || sunrise_hour >= sunset_hour
        {
            return Err(LightingError::InvalidSunTimes);
        }
        Ok(Self { keys, sunrise_hour, sunset_hour, path_tilt })
    }

    /// Dusk-heavy default palette tuned by eye to the reference screenshots.
    pub fn gta6_dusk() -> Self {
        let keys = vec![
            SkyKey { hour: 0.0, sun_color: c(0x3A4A8C), sun_intensity: 0.05, ambient: c(0x1A1F3D), sky_zenith: c(0x070B1C), sky_horizon: c(0x1B2147) },
            SkyKey { hour: 5.5, sun_color: c(0x6A5ACD), sun_intensity: 0.10, ambient: c(0x2B2A55), sky_zenith: c(0x1A1D4A), sky_horizon: c(0x7B4B8F) },
            SkyKey { hour: 7.0, sun_color: c(0xFFB27A), sun_intensity: 2.0, ambient: c(0x7E8DB5), sky_zenith: c(0x4F83C8), sky_horizon: c(0xF2B48C) },
            SkyKey { hour: 13.0, sun_color: c(0xFFF4E0), sun_intensity: 5.0, ambient: c(0x9DB8E0), sky_zenith: c(0x2F6FC4), sky_horizon: c(0xA9CCEA) },
            SkyKey { hour: 18.0, sun_color: c(0xFFA45C), sun_intensity: 3.0, ambient: c(0x8A6FA8), sky_zenith: c(0x3F5FAE), sky_horizon: c(0xFF9A5A) },
            SkyKey { hour: 19.5, sun_color: c(0xFF8A3D), sun_intensity: 1.2, ambient: c(0x5A3F8A), sky_zenith: c(0x2B1B4A), sky_horizon: c(0xE5398C) },
            SkyKey { hour: 21.0, sun_color: c(0x4B4FA8), sun_intensity: 0.12, ambient: c(0x241F52), sky_zenith: c(0x0C0F2A), sky_horizon: c(0x4B2A6E) },
        ];
        Self::new(keys, 6.5, 19.5, 0.35).expect("built-in keys are valid")
    }

    /// Direction pointing from the world toward the sun (unit length). Below the horizon at night.
    pub fn sun_direction(&self, hour: f32) -> Option<Vec3> {
        if !hour.is_finite() {
            return None;
        }
        let h = hour.rem_euclid(24.0);
        let day_len = self.sunset_hour - self.sunrise_hour;
        let night_len = 24.0 - day_len;
        // Angle 0 at sunrise, PI at sunset, continuing below the horizon at night.
        let angle = if h >= self.sunrise_hour && h <= self.sunset_hour {
            (h - self.sunrise_hour) / day_len * std::f32::consts::PI
        } else {
            let since_sunset = if h > self.sunset_hour { h - self.sunset_hour } else { h + 24.0 - self.sunset_hour };
            std::f32::consts::PI + since_sunset / night_len * std::f32::consts::PI
        };
        let dir = Vec3::new(angle.cos(), angle.sin(), self.path_tilt.sin() * angle.sin().abs().max(0.2));
        Some(dir.normalize_or_zero())
    }

    pub fn sample(&self, hour: f32) -> Option<SkyState> {
        if !hour.is_finite() {
            return None;
        }
        let h = hour.rem_euclid(24.0);
        let n = self.keys.len();
        // Find the segment [a, b] containing h, wrapping last -> first (+24h).
        let (a, b, span_start, span_len) = match self.keys.iter().position(|k| k.hour > h) {
            Some(0) | None => {
                let a = self.keys[n - 1];
                let b = self.keys[0];
                let start = a.hour;
                let len = b.hour + 24.0 - a.hour;
                (a, b, start, len)
            }
            Some(i) => {
                let (a, b) = (self.keys[i - 1], self.keys[i]);
                (a, b, a.hour, b.hour - a.hour)
            }
        };
        let hh = if h < span_start { h + 24.0 } else { h };
        let t = ((hh - span_start) / span_len).clamp(0.0, 1.0);
        let lerp = |x: f32, y: f32| x + (y - x) * t;
        let mix = |x: Rgb, y: Rgb| [lerp(x[0], y[0]), lerp(x[1], y[1]), lerp(x[2], y[2])];
        Some(SkyState {
            sun_dir_to_sun: self.sun_direction(h)?,
            sun_color: mix(a.sun_color, b.sun_color),
            sun_intensity: lerp(a.sun_intensity, b.sun_intensity),
            ambient: mix(a.ambient, b.ambient),
            sky_zenith: mix(a.sky_zenith, b.sky_zenith),
            sky_horizon: mix(a.sky_horizon, b.sky_horizon),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_is_high_at_noon_and_below_horizon_at_midnight() {
        let cfg = LightingConfig::gta6_dusk();
        assert!(cfg.sun_direction(13.0).unwrap().y > 0.9);
        assert!(cfg.sun_direction(0.0).unwrap().y < -0.3);
        assert!(cfg.sun_direction(cfg.sunrise_hour).unwrap().y.abs() < 1e-3);
        assert!(cfg.sun_direction(f32::NAN).is_none());
    }

    #[test]
    fn sample_hits_keyframes_exactly_and_wraps() {
        let cfg = LightingConfig::gta6_dusk();
        let dusk = cfg.sample(19.5).unwrap();
        assert!((dusk.sky_horizon[0] - cfg.keys[5].sky_horizon[0]).abs() < 1e-6);
        // 23:00 and -1:00 are the same moment
        let a = cfg.sample(23.0).unwrap();
        let b = cfg.sample(-1.0).unwrap();
        assert_eq!(a.sky_zenith, b.sky_zenith);
        // between last key (21:00) and first key (0:00 next day) interpolation stays finite
        assert!(cfg.sample(22.5).unwrap().ambient.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn invalid_configs_rejected() {
        let k = SkyKey { hour: 1.0, sun_color: [0.0; 3], sun_intensity: 0.0, ambient: [0.0; 3], sky_zenith: [0.0; 3], sky_horizon: [0.0; 3] };
        assert_eq!(LightingConfig::new(vec![k], 6.0, 18.0, 0.0).unwrap_err(), LightingError::TooFewKeys);
        assert_eq!(LightingConfig::new(vec![k, k], 6.0, 18.0, 0.0).unwrap_err(), LightingError::KeysNotAscending);
        let k2 = SkyKey { hour: 30.0, ..k };
        assert_eq!(LightingConfig::new(vec![k, k2], 6.0, 18.0, 0.0).unwrap_err(), LightingError::HourOutOfRange);
        let k3 = SkyKey { hour: 2.0, ..k };
        assert_eq!(LightingConfig::new(vec![k, k3], 18.0, 6.0, 0.0).unwrap_err(), LightingError::InvalidSunTimes);
    }
}
