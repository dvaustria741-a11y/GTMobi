use glam::Vec3;
use noise::{NoiseFn, Perlin};
use gtm_render::mesh::Vertex;
use crate::biome::Biome;

/// Chunk side length in world-space meters.
pub const CHUNK_SIZE: f32 = 128.0;
/// Vertices per side of the terrain grid within one chunk.
/// 33 gives 32×32 quads = 2048 triangles per chunk.
const GRID: usize = 33;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkCoord {
    pub cx: i32,
    pub cz: i32,
}

impl ChunkCoord {
    pub fn from_world(x: f32, z: f32) -> Self {
        Self {
            cx: (x / CHUNK_SIZE).floor() as i32,
            cz: (z / CHUNK_SIZE).floor() as i32,
        }
    }

    pub fn origin(&self) -> Vec3 {
        Vec3::new(self.cx as f32 * CHUNK_SIZE, 0.0, self.cz as f32 * CHUNK_SIZE)
    }
}

/// Analytic terrain height, shared by mesh generation and gameplay queries.
pub struct HeightField {
    coarse: Perlin,
    fine: Perlin,
}

impl HeightField {
    pub fn new(seed: u32) -> Self {
        Self { coarse: Perlin::new(seed), fine: Perlin::new(seed.wrapping_add(7919)) }
    }

    pub fn height_at(&self, wx: f32, wz: f32) -> f32 {
        let biome = Biome::at(wx, wz);
        let coarse = self.coarse.get([wx as f64 * 0.00045, wz as f64 * 0.00045]) as f32;
        let fine = self.fine.get([wx as f64 * 0.003, wz as f64 * 0.003]) as f32 * 0.25;
        biome.base_height() + (coarse + fine) * biome.height_variance()
    }

    /// Upward-facing unit normal from central differences of the height field.
    pub fn normal_at(&self, wx: f32, wz: f32, step: f32) -> Vec3 {
        let dhdx = (self.height_at(wx + step, wz) - self.height_at(wx - step, wz)) / (2.0 * step);
        let dhdz = (self.height_at(wx, wz + step) - self.height_at(wx, wz - step)) / (2.0 * step);
        Vec3::new(-dhdx, 1.0, -dhdz).normalize()
    }
}

pub struct Chunk {
    pub coord: ChunkCoord,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Chunk {
    pub fn generate(coord: ChunkCoord, seed: u32) -> Self {
        Self::generate_with(coord, &HeightField::new(seed))
    }

    /// Terrain mesh for `coord`. Normals are computed analytically, so chunk borders have no seams.
    pub fn generate_with(coord: ChunkCoord, field: &HeightField) -> Self {
        let origin = coord.origin();
        let step = CHUNK_SIZE / (GRID - 1) as f32;
        let mut verts: Vec<Vertex> = Vec::with_capacity(GRID * GRID);

        for iz in 0..GRID {
            for ix in 0..GRID {
                let wx = origin.x + ix as f32 * step;
                let wz = origin.z + iz as f32 * step;
                let biome = Biome::at(wx, wz);
                verts.push(Vertex {
                    position: [wx, field.height_at(wx, wz), wz],
                    normal: field.normal_at(wx, wz, step).to_array(),
                    uv: [ix as f32 / (GRID - 1) as f32, iz as f32 / (GRID - 1) as f32],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    color: biome.ground_color(),
                });
            }
        }

        // Counter-clockwise when viewed from +Y (front face for the pipeline's default winding).
        let mut indices: Vec<u32> = Vec::with_capacity((GRID - 1) * (GRID - 1) * 6);
        for iz in 0..(GRID - 1) {
            for ix in 0..(GRID - 1) {
                let tl = (iz * GRID + ix) as u32;
                let tr = (iz * GRID + ix + 1) as u32;
                let bl = ((iz + 1) * GRID + ix) as u32;
                let br = ((iz + 1) * GRID + ix + 1) as u32;
                indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
            }
        }

        Self { coord, vertices: verts, indices }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normals_point_up_and_are_unit() {
        let f = HeightField::new(42);
        for &(x, z) in &[(0.0, 2300.0), (-500.0, -400.0), (900.0, 1200.0)] {
            let n = f.normal_at(x, z, 4.0);
            assert!(n.y > 0.5, "normal must face up, got {n:?}");
            assert!((n.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn first_triangle_faces_up() {
        let c = Chunk::generate(ChunkCoord { cx: 0, cz: 18 }, 42);
        let v = |i: u32| Vec3::from_array(c.vertices[i as usize].position);
        let (a, b, d) = (v(c.indices[0]), v(c.indices[1]), v(c.indices[2]));
        assert!((b - a).cross(d - a).y > 0.0);
    }
}
