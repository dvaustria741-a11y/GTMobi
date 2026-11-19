use std::collections::HashMap;
use wgpu::{util::DeviceExt, Buffer, Device, Queue};
use glam::Vec3;
use gtm_render::mesh::Vertex;
use crate::chunk::{Chunk, ChunkCoord, HeightField};
use crate::biome::Biome;

/// How many chunks to keep loaded in each direction from the camera.
const LOAD_RADIUS: i32 = 4;   // 4 × 128 m = 512 m draw distance

pub struct GpuChunk {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
}

pub struct TerrainGen {
    heights: HeightField,
    pub chunks: HashMap<ChunkCoord, GpuChunk>,
}

impl TerrainGen {
    pub fn new(seed: u32) -> Self {
        Self { heights: HeightField::new(seed), chunks: HashMap::new() }
    }

    /// Ground height in meters at a world position.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.heights.height_at(x, z)
    }

    /// Call every frame with current camera position.
    /// Loads missing chunks, unloads distant ones.
    pub fn update(&mut self, cam: Vec3, device: &Device, queue: &Queue) {
        let center = ChunkCoord::from_world(cam.x, cam.z);

        // Unload chunks outside radius + 1
        let unload_r = LOAD_RADIUS + 1;
        self.chunks.retain(|coord, _| {
            (coord.cx - center.cx).abs() <= unload_r
                && (coord.cz - center.cz).abs() <= unload_r
        });

        // Load missing chunks in radius
        for dz in -LOAD_RADIUS..=LOAD_RADIUS {
            for dx in -LOAD_RADIUS..=LOAD_RADIUS {
                let coord = ChunkCoord {
                    cx: center.cx + dx,
                    cz: center.cz + dz,
                };
                if !self.chunks.contains_key(&coord) {
                    let chunk = Chunk::generate_with(coord, &self.heights);
                    let gpu   = upload_chunk(device, &chunk);
                    self.chunks.insert(coord, gpu);
                }
            }
        }
    }

    pub fn iter_gpu(&self) -> impl Iterator<Item = &GpuChunk> {
        self.chunks.values()
    }
}

fn upload_chunk(device: &Device, chunk: &Chunk) -> GpuChunk {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("terrain_vb"),
        contents: bytemuck::cast_slice(&chunk.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("terrain_ib"),
        contents: bytemuck::cast_slice(&chunk.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuChunk {
        vertex_buffer,
        index_buffer,
        index_count: chunk.indices.len() as u32,
    }
}
