use glam::Vec3;
use noise::{NoiseFn, Perlin, Seedable};
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
        Vec3::new(
            self.cx as f32 * CHUNK_SIZE,
            0.0,
            self.cz as f32 * CHUNK_SIZE,
        )
    }
}

pub struct Chunk {
    pub coord: ChunkCoord,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Chunk {
    /// Generate terrain mesh for `coord` using layered Perlin noise.
    /// Biome is sampled per-vertex so colour and height blend at county edges.
    pub fn generate(coord: ChunkCoord, seed: u32) -> Self {
        let perlin_coarse = Perlin::new(seed);
        let perlin_fine   = Perlin::new(seed.wrapping_add(7919));
        let origin        = coord.origin();

        let step  = CHUNK_SIZE / (GRID - 1) as f32;
        let mut verts: Vec<Vertex> = Vec::with_capacity(GRID * GRID);
        let mut heights = [[0.0f32; GRID]; GRID]; // cache for normal calc

        // ── Pass 1: positions + heights ──────────────────────────────────
        for iz in 0..GRID {
            for ix in 0..GRID {
                let wx = origin.x + ix as f32 * step;
                let wz = origin.z + iz as f32 * step;
                let biome = Biome::at(wx, wz);

                let coarse = perlin_coarse.get([wx as f64 * 0.00045, wz as f64 * 0.00045]) as f32;
                let fine   = perlin_fine.get([wx as f64 * 0.003,    wz as f64 * 0.003])    as f32 * 0.25;
                let wy = biome.base_height() + (coarse + fine) * biome.height_variance();

                heights[iz][ix] = wy;

                verts.push(Vertex {
                    position: [wx, wy, wz],
                    normal:   [0.0, 1.0, 0.0], // patched in pass 2
                    uv:       [ix as f32 / (GRID - 1) as f32, iz as f32 / (GRID - 1) as f32],
                    tangent:  [1.0, 0.0, 0.0, 1.0],
                    color:    biome.ground_color(),
                });
            }
        }

        // ── Pass 2: normals via central differences ───────────────────────
        for iz in 0..GRID {
            for ix in 0..GRID {
                let h_l = if ix > 0       { heights[iz][ix - 1] } else { heights[iz][ix] };
                let h_r = if ix < GRID-1  { heights[iz][ix + 1] } else { heights[iz][ix] };
                let h_d = if iz > 0       { heights[iz - 1][ix] } else { heights[iz][ix] };
                let h_u = if iz < GRID-1  { heights[iz + 1][ix] } else { heights[iz][ix] };

                let dx = Vec3::new(2.0 * step, h_r - h_l, 0.0);
                let dz = Vec3::new(0.0, h_u - h_d, 2.0 * step);
                let n  = dx.cross(dz).normalize();

                let idx = iz * GRID + ix;
                verts[idx].normal = n.to_array();
            }
        }

        // ── Indices: two triangles per quad ──────────────────────────────
        let mut indices: Vec<u32> = Vec::with_capacity((GRID - 1) * (GRID - 1) * 6);
        for iz in 0..(GRID - 1) {
            for ix in 0..(GRID - 1) {
                let tl = (iz     * GRID + ix    ) as u32;
                let tr = (iz     * GRID + ix + 1) as u32;
                let bl = ((iz+1) * GRID + ix    ) as u32;
                let br = ((iz+1) * GRID + ix + 1) as u32;
                // CCW winding (right-hand, Y-up)
                indices.extend_from_slice(&[tl, bl, tr,  tr, bl, br]);
            }
        }

        Self { coord, vertices: verts, indices }
    }
}
