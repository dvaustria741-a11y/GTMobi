// Vice Dale road grid — axis-aligned city blocks
// Art Deco Miami-style layout: wide boulevards every ~200 m, side streets every ~80 m

use glam::Vec2;

pub const BOULEVARD_SPACING: f32 = 200.0;
pub const SIDE_STREET_SPACING: f32 = 80.0;
pub const BOULEVARD_WIDTH: f32 = 24.0;  // 4-lane road + median
pub const SIDE_STREET_WIDTH: f32 = 12.0;

#[derive(Debug, Clone)]
pub struct RoadSegment {
    pub start: Vec2,
    pub end: Vec2,
    pub width: f32,
    pub is_boulevard: bool,
}

pub struct RoadGrid {
    pub segments: Vec<RoadSegment>,
}

impl RoadGrid {
    /// Generate the Vice Dale city grid in a bounding box (meters).
    /// Only generates within UrbanCore biome bounds.
    pub fn generate(min_x: f32, max_x: f32, min_z: f32, max_z: f32) -> Self {
        let mut segments = Vec::new();
        let y = 3.0; // urban terrain height

        // ── East-West boulevards (run along X axis) ───────────────────────
        let mut z = snap_to_grid(min_z, BOULEVARD_SPACING);
        while z <= max_z {
            segments.push(RoadSegment {
                start: Vec2::new(min_x, z),
                end:   Vec2::new(max_x, z),
                width: BOULEVARD_WIDTH,
                is_boulevard: true,
            });
            z += BOULEVARD_SPACING;
        }

        // ── North-South boulevards ────────────────────────────────────────
        let mut x = snap_to_grid(min_x, BOULEVARD_SPACING);
        while x <= max_x {
            segments.push(RoadSegment {
                start: Vec2::new(x, min_z),
                end:   Vec2::new(x, max_z),
                width: BOULEVARD_WIDTH,
                is_boulevard: true,
            });
            x += BOULEVARD_SPACING;
        }

        // ── Side streets between boulevards ──────────────────────────────
        let mut z = snap_to_grid(min_z, SIDE_STREET_SPACING);
        while z <= max_z {
            // Skip if too close to a boulevard
            if !near_grid(z, BOULEVARD_SPACING, BOULEVARD_WIDTH * 0.6) {
                segments.push(RoadSegment {
                    start: Vec2::new(min_x, z),
                    end:   Vec2::new(max_x, z),
                    width: SIDE_STREET_WIDTH,
                    is_boulevard: false,
                });
            }
            z += SIDE_STREET_SPACING;
        }

        let mut x = snap_to_grid(min_x, SIDE_STREET_SPACING);
        while x <= max_x {
            if !near_grid(x, BOULEVARD_SPACING, BOULEVARD_WIDTH * 0.6) {
                segments.push(RoadSegment {
                    start: Vec2::new(x, min_z),
                    end:   Vec2::new(x, max_z),
                    width: SIDE_STREET_WIDTH,
                    is_boulevard: false,
                });
            }
            x += SIDE_STREET_SPACING;
        }

        log::debug!("RoadGrid: {} segments generated", segments.len());
        Self { segments }
    }

    /// Return road width at world position, or 0.0 if not on a road.
    /// Used by terrain generator to flatten road areas.
    pub fn road_width_at(&self, x: f32, z: f32) -> f32 {
        for seg in &self.segments {
            let (along, across, len) = project_to_segment(Vec2::new(x, z), seg.start, seg.end);
            if along >= 0.0 && along <= len && across.abs() < seg.width * 0.5 {
                return seg.width;
            }
        }
        0.0
    }
}

fn snap_to_grid(v: f32, spacing: f32) -> f32 {
    (v / spacing).ceil() * spacing
}

fn near_grid(v: f32, spacing: f32, margin: f32) -> bool {
    let rem = v.rem_euclid(spacing);
    rem < margin || rem > spacing - margin
}

/// Returns (along, across, segment_length) for point p relative to segment a→b.
fn project_to_segment(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32, f32) {
    let ab = b - a;
    let len = ab.length();
    if len < 1e-5 { return (0.0, (p - a).length(), 0.0); }
    let dir = ab / len;
    let ap  = p - a;
    let along  = ap.dot(dir);
    let across = ap.perp_dot(dir); // signed distance from line
    (along, across, len)
}
