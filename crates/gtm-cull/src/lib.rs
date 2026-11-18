//! Visibility culling for GTMobi: view-frustum planes and a ground-plane (XZ)
//! spatial grid with distance and frustum rejection.
//!
//! Conventions follow gtm-math: right-handed, +Y up, meters.

use gtm_math::{Aabb, Mat4, Vec3};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
struct Plane {
    normal: Vec3,
    d: f32,
}

/// Six frustum planes, normals pointing inward.
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [Plane; 6],
}

impl Frustum {
    /// Extracts planes from a column-major view-projection matrix with 0..1 clip depth.
    /// Returns `None` if the matrix is non-finite or yields a degenerate plane.
    pub fn from_view_proj(m: &Mat4) -> Option<Frustum> {
        let r0 = m.row(0);
        let r1 = m.row(1);
        let r2 = m.row(2);
        let r3 = m.row(3);
        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
        let raw = [
            add(r3, r0), // left
            sub(r3, r0), // right
            add(r3, r1), // bottom
            sub(r3, r1), // top
            r2,          // near (0..1 depth)
            sub(r3, r2), // far
        ];
        let mut planes = [Plane { normal: Vec3::ZERO, d: 0.0 }; 6];
        for (slot, p) in planes.iter_mut().zip(raw.iter()) {
            let n = Vec3::new(p[0], p[1], p[2]);
            let len = n.length();
            if !len.is_finite() || len <= 1e-12 || !p[3].is_finite() {
                return None;
            }
            *slot = Plane { normal: n * (1.0 / len), d: p[3] / len };
        }
        Some(Frustum { planes })
    }

    pub fn intersects_aabb(&self, b: &Aabb) -> bool {
        for p in &self.planes {
            let v = Vec3::new(
                if p.normal.x >= 0.0 { b.max.x } else { b.min.x },
                if p.normal.y >= 0.0 { b.max.y } else { b.min.y },
                if p.normal.z >= 0.0 { b.max.z } else { b.min.z },
            );
            if p.normal.dot(v) + p.d < 0.0 {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GridConfig {
    /// Cell edge length in meters on the XZ plane.
    pub cell_size: f32,
    /// World height range used for the per-cell frustum test.
    pub min_y: f32,
    pub max_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridError {
    InvalidConfig,
    InvalidBounds,
    OutOfRange,
    TooManyCells,
}

const MAX_ABS_COORD: f32 = 1.0e6;
const MAX_CELLS_PER_ITEM: i64 = 4096;
const MAX_QUERY_CELLS: i64 = 1 << 20;

struct Item {
    id: u32,
    bounds: Aabb,
}

pub struct CullGrid {
    cfg: GridConfig,
    items: Vec<Item>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

/// Reusable per-query state so repeated queries don't allocate.
#[derive(Default)]
pub struct CullScratch {
    stamp: Vec<u32>,
    epoch: u32,
}

impl CullGrid {
    pub fn new(cfg: GridConfig) -> Result<Self, GridError> {
        if !(cfg.cell_size.is_finite() && cfg.cell_size > 0.0)
            || !cfg.min_y.is_finite()
            || !cfg.max_y.is_finite()
            || cfg.min_y > cfg.max_y
        {
            return Err(GridError::InvalidConfig);
        }
        Ok(Self { cfg, items: Vec::new(), cells: HashMap::new() })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn cell_of(&self, v: f32) -> i32 {
        (v / self.cfg.cell_size).floor() as i32
    }

    /// Inserts an object. `id` is returned verbatim by queries.
    pub fn insert(&mut self, id: u32, bounds: Aabb) -> Result<(), GridError> {
        if !bounds.is_valid() {
            return Err(GridError::InvalidBounds);
        }
        let lim = MAX_ABS_COORD;
        if bounds.min.x < -lim || bounds.max.x > lim || bounds.min.z < -lim || bounds.max.z > lim {
            return Err(GridError::OutOfRange);
        }
        let (x0, x1) = (self.cell_of(bounds.min.x), self.cell_of(bounds.max.x));
        let (z0, z1) = (self.cell_of(bounds.min.z), self.cell_of(bounds.max.z));
        let span = (x1 as i64 - x0 as i64 + 1) * (z1 as i64 - z0 as i64 + 1);
        if span > MAX_CELLS_PER_ITEM {
            return Err(GridError::TooManyCells);
        }
        let slot = self.items.len() as u32;
        self.items.push(Item { id, bounds });
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                self.cells.entry((cx, cz)).or_default().push(slot);
            }
        }
        Ok(())
    }

    /// Writes ids of objects within `max_distance` of `camera_pos` that intersect
    /// `frustum` into `out` (cleared first). Returns `Err` if the query is invalid
    /// or would touch an unreasonable number of cells.
    pub fn visible(
        &self,
        frustum: &Frustum,
        camera_pos: Vec3,
        max_distance: f32,
        scratch: &mut CullScratch,
        out: &mut Vec<u32>,
    ) -> Result<(), GridError> {
        out.clear();
        if !camera_pos.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return Err(GridError::InvalidConfig);
        }
        if camera_pos.x.abs() > MAX_ABS_COORD || camera_pos.z.abs() > MAX_ABS_COORD {
            return Err(GridError::OutOfRange);
        }
        let (x0, x1) = (self.cell_of(camera_pos.x - max_distance), self.cell_of(camera_pos.x + max_distance));
        let (z0, z1) = (self.cell_of(camera_pos.z - max_distance), self.cell_of(camera_pos.z + max_distance));
        let span = (x1 as i64 - x0 as i64 + 1) * (z1 as i64 - z0 as i64 + 1);
        if span > MAX_QUERY_CELLS {
            return Err(GridError::TooManyCells);
        }

        if scratch.stamp.len() < self.items.len() {
            scratch.stamp.resize(self.items.len(), 0);
        }
        scratch.epoch = scratch.epoch.wrapping_add(1);
        if scratch.epoch == 0 {
            scratch.stamp.iter_mut().for_each(|s| *s = 0);
            scratch.epoch = 1;
        }
        let epoch = scratch.epoch;
        let max_sq = max_distance * max_distance;
        let cs = self.cfg.cell_size;

        for cz in z0..=z1 {
            for cx in x0..=x1 {
                let Some(list) = self.cells.get(&(cx, cz)) else { continue };
                let cell_box = Aabb::new(
                    Vec3::new(cx as f32 * cs, self.cfg.min_y, cz as f32 * cs),
                    Vec3::new((cx + 1) as f32 * cs, self.cfg.max_y, (cz + 1) as f32 * cs),
                );
                if cell_box.distance_squared_to_point(camera_pos) > max_sq
                    || !frustum.intersects_aabb(&cell_box)
                {
                    continue;
                }
                for &slot in list {
                    let s = slot as usize;
                    if scratch.stamp[s] == epoch {
                        continue;
                    }
                    scratch.stamp[s] = epoch;
                    let item = &self.items[s];
                    if item.bounds.distance_squared_to_point(camera_pos) <= max_sq
                        && frustum.intersects_aabb(&item.bounds)
                    {
                        out.push(item.id);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(c: Vec3, h: f32) -> Aabb {
        Aabb::new(c - Vec3::new(h, h, h), c + Vec3::new(h, h, h))
    }

    fn test_frustum() -> Frustum {
        let view = Mat4::look_to_rh(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 500.0);
        Frustum::from_view_proj(&proj.mul_mat4(&view)).expect("valid matrix")
    }

    #[test]
    fn frustum_accepts_front_rejects_back_and_side() {
        let f = test_frustum();
        assert!(f.intersects_aabb(&cube(Vec3::new(0.0, 0.0, -10.0), 1.0)));
        assert!(!f.intersects_aabb(&cube(Vec3::new(0.0, 0.0, 10.0), 1.0)));
        assert!(!f.intersects_aabb(&cube(Vec3::new(1000.0, 0.0, -10.0), 1.0)));
        assert!(!f.intersects_aabb(&cube(Vec3::new(0.0, 0.0, -600.0), 1.0)));
    }

    #[test]
    fn grid_returns_each_visible_item_once() {
        let mut g = CullGrid::new(GridConfig { cell_size: 16.0, min_y: -10.0, max_y: 200.0 }).unwrap();
        g.insert(1, cube(Vec3::new(0.0, 0.0, -10.0), 1.0)).unwrap();
        // spans several cells; must still be reported once
        g.insert(2, Aabb::new(Vec3::new(-20.0, 0.0, -60.0), Vec3::new(20.0, 5.0, -40.0))).unwrap();
        g.insert(3, cube(Vec3::new(0.0, 0.0, 50.0), 1.0)).unwrap(); // behind camera
        g.insert(4, cube(Vec3::new(0.0, 0.0, -400.0), 1.0)).unwrap(); // beyond distance
        let mut scratch = CullScratch::default();
        let mut out = Vec::new();
        g.visible(&test_frustum(), Vec3::ZERO, 200.0, &mut scratch, &mut out).unwrap();
        out.sort_unstable();
        assert_eq!(out, vec![1, 2]);
    }

    #[test]
    fn grid_rejects_bad_input() {
        let mut g = CullGrid::new(GridConfig { cell_size: 16.0, min_y: 0.0, max_y: 100.0 }).unwrap();
        let nan = Aabb::new(Vec3::new(f32::NAN, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(g.insert(1, nan), Err(GridError::InvalidBounds));
        let far = Aabb::new(Vec3::new(2.0e6, 0.0, 0.0), Vec3::new(2.0e6 + 1.0, 1.0, 1.0));
        assert_eq!(g.insert(2, far), Err(GridError::OutOfRange));
        assert!(CullGrid::new(GridConfig { cell_size: 0.0, min_y: 0.0, max_y: 1.0 }).is_err());
    }
}
