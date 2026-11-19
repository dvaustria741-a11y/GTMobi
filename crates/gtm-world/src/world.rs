use glam::Vec3;
use wgpu::{Device, Queue};
use crate::{
    road::RoadGrid,
    terrain::TerrainGen,
};

pub struct World {
    pub terrain: TerrainGen,
    pub roads:   RoadGrid,
}

impl World {
    pub fn new(seed: u32) -> Self {
        // Vice Dale urban grid covers the city bounds
        let roads = RoadGrid::generate(-1_800.0, 1_800.0, 300.0, 2_800.0);
        Self {
            terrain: TerrainGen::new(seed),
            roads,
        }
    }

    pub fn update(&mut self, cam: Vec3, device: &Device, queue: &Queue) {
        self.terrain.update(cam, device, queue);
    }
}
