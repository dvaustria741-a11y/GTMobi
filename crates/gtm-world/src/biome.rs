// Florida biome layout — mirrors GTA 6 county geography
//
// World space (meters, Y-up):
//   X: west (-) → east (+)
//   Z: north (-) → south (+)
//
// County layout (approximate, scaled to ~8 km × 10 km world):
//   Lummox County  — north,  swamp/rural,   Z < -2000
//   Leonard County — mid,    suburban/grass, -2000 ≤ Z < 0
//   Vice Dale      — center, dense urban,    0 ≤ Z < 2500,  abs(X) < 1800
//   Mariana County — south,  keys/beach,     Z ≥ 2500

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    /// Dense sawgrass marsh, standing water, cypress knees — Lummox County
    Swamp,
    /// Flat pasture, trailer parks, two-lane highways — Lummox/Leonard transition
    Rural,
    /// Suburban blocks, strip malls, canals — Leonard County
    Suburban,
    /// High-rises, neon, marinas, cruise terminals — Vice Dale core
    UrbanCore,
    /// Beach, waterfront boardwalk, Art Deco strip — Vice Dale coast
    Beach,
    /// Keys, mangroves, shallow reef flats — Mariana County
    Keys,
    /// Open ocean tile (rendered as water plane)
    Ocean,
}

impl Biome {
    /// Classify a world-space (x, z) position into a biome.
    /// No noise — county boundaries are hard-edged like the real GTA 6 map.
    pub fn at(x: f32, z: f32) -> Self {
        let dist_from_center = (x * x + z * z).sqrt();

        if dist_from_center > 5_200.0 {
            return Biome::Ocean;
        }

        // Mariana Keys — far south chain of islands
        if z > 3_200.0 {
            return if dist_from_center < 4_800.0 { Biome::Keys } else { Biome::Ocean };
        }

        // Vice Dale — urban core + beach coast
        if z >= 300.0 && z < 2_800.0 {
            let beach_dist = (x.abs() - 1_600.0).max(0.0);
            if beach_dist < 300.0 || (z > 2_200.0 && x.abs() < 2_000.0) {
                return Biome::Beach;
            }
            if x.abs() < 1_800.0 {
                return Biome::UrbanCore;
            }
        }

        // Leonard County
        if z >= -1_800.0 && z < 300.0 {
            return Biome::Suburban;
        }

        // Lummox County transition
        if z >= -3_200.0 && z < -1_800.0 {
            // Swamp pockets near rivers
            let swamp_noise = (x * 0.003).sin() * (z * 0.002).cos();
            return if swamp_noise > 0.1 { Biome::Swamp } else { Biome::Rural };
        }

        // Deep Lummox
        if z < -3_200.0 {
            return Biome::Swamp;
        }

        Biome::Rural
    }

    /// Base terrain height (meters) for this biome — Florida is FLAT.
    pub fn base_height(&self) -> f32 {
        match self {
            Biome::Swamp     => 0.4,  // just above sea level, often flooded
            Biome::Rural     => 1.8,
            Biome::Suburban  => 2.5,
            Biome::UrbanCore => 3.0,
            Biome::Beach     => 0.3,
            Biome::Keys      => 0.5,
            Biome::Ocean     => -6.0,
        }
    }

    /// Max noise amplitude on top of base_height — keeps Florida impressively flat
    pub fn height_variance(&self) -> f32 {
        match self {
            Biome::Swamp     => 0.6,
            Biome::Rural     => 1.2,
            Biome::Suburban  => 0.8,
            Biome::UrbanCore => 0.3, // roads flatten it
            Biome::Beach     => 0.4,
            Biome::Keys      => 0.5,
            Biome::Ocean     => 0.0,
        }
    }

    /// Vertex colour tint baked into terrain mesh
    pub fn ground_color(&self) -> [f32; 4] {
        match self {
            Biome::Swamp     => [0.22, 0.28, 0.14, 1.0], // dark olive-green
            Biome::Rural     => [0.42, 0.52, 0.22, 1.0], // pale Florida grass
            Biome::Suburban  => [0.38, 0.46, 0.20, 1.0], // manicured lawn
            Biome::UrbanCore => [0.30, 0.30, 0.32, 1.0], // asphalt grey
            Biome::Beach     => [0.88, 0.80, 0.60, 1.0], // sand
            Biome::Keys      => [0.72, 0.68, 0.44, 1.0], // coral sand
            Biome::Ocean     => [0.05, 0.22, 0.42, 1.0], // deep blue
        }
    }
}
