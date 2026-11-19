//! CPU preview of the first frame (no GPU): same terrain, character, camera math and a
//! simplified version of the shader's lighting. Writes a PPM.
//! usage: cargo run --release -p gtm-app --example preview -- out.ppm [heading_rad] [pitch_rad]
use glam::{Mat4, Vec3, Vec4};
use gtm_render::{character, mesh::Vertex};
use gtm_world::{Chunk, ChunkCoord, HeightField};

const W: usize = 960;
const H: usize = 540;

struct Tri {
    v: [Vertex; 3],
    model: Mat4,
}

fn aces(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

fn srgb(x: f32) -> u8 {
    let c = if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or("preview.ppm".into());
    let heading: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let pitch: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.25);

    let field = HeightField::new(42);
    let spawn = (0.0f32, 2300.0f32);
    let ppos = Vec3::new(spawn.0, field.height_at(spawn.0, spawn.1), spawn.1);
    let pivot = ppos + Vec3::new(0.0, 1.45, 0.0);
    let (mut cam, target) = gtm_app::orbit_camera(pivot, heading, pitch, 3.6, 0.45);
    cam.y = cam.y.max(field.height_at(cam.x, cam.z) + 0.4);

    let view = Mat4::look_at_rh(cam, target, Vec3::Y);
    let proj = Mat4::perspective_rh(65f32.to_radians(), W as f32 / H as f32, 0.1, 1500.0);
    let vp = proj * view;

    let sky = gtm_lighting::LightingConfig::gta6_dusk().sample(15.0).unwrap();
    let sun = Vec3::new(sky.sun_dir_to_sun.x, sky.sun_dir_to_sun.y, sky.sun_dir_to_sun.z);

    let mut tris: Vec<Tri> = Vec::new();
    let c0 = ChunkCoord::from_world(ppos.x, ppos.z);
    for dz in -4..=4 {
        for dx in -4..=4 {
            let ch = Chunk::generate_with(ChunkCoord { cx: c0.cx + dx, cz: c0.cz + dz }, &field);
            for t in ch.indices.chunks(3) {
                tris.push(Tri { v: [ch.vertices[t[0] as usize], ch.vertices[t[1] as usize], ch.vertices[t[2] as usize]], model: Mat4::IDENTITY });
            }
        }
    }
    let bytes = std::fs::read("assets/third_party/defy/characters/template_player.glb")
        .or_else(|_| std::fs::read("../../assets/third_party/defy/characters/template_player.glb")).expect("glb");
    let model = Mat4::from_translation(ppos) * Mat4::from_rotation_y(heading);
    for mut m in character::load_bind_pose(&bytes, 1.8, Some("Male")).unwrap() {
        m.vertices.iter_mut().for_each(gtm_app::tint_vertex);
        for t in m.indices.chunks(3) {
            tris.push(Tri { v: [m.vertices[t[0] as usize], m.vertices[t[1] as usize], m.vertices[t[2] as usize]], model });
        }
    }

    let clear = [aces(sky.sky_horizon[0]), aces(sky.sky_horizon[1]), aces(sky.sky_horizon[2])];
    let mut color = vec![[clear[0], clear[1], clear[2]]; W * H];
    let mut depth = vec![1.0f32; W * H];
    let (mut front, mut back, mut clipped) = (0u32, 0u32, 0u32);

    for tri in &tris {
        let wp: Vec<Vec3> = tri.v.iter().map(|v| tri.model.transform_point3(Vec3::from_array(v.position))).collect();
        let clip: Vec<Vec4> = wp.iter().map(|p| vp * p.extend(1.0)).collect();
        if clip.iter().any(|c| c.w <= 0.1) { clipped += 1; continue; }
        let ndc: Vec<Vec3> = clip.iter().map(|c| Vec3::new(c.x / c.w, c.y / c.w, c.z / c.w)).collect();
        let area = (ndc[1].x - ndc[0].x) * (ndc[2].y - ndc[0].y) - (ndc[2].x - ndc[0].x) * (ndc[1].y - ndc[0].y);
        if area <= 0.0 { back += 1; continue; } // CCW is front, back faces culled like the pipeline
        front += 1;
        let sx: Vec<f32> = ndc.iter().map(|n| (n.x * 0.5 + 0.5) * W as f32).collect();
        let sy: Vec<f32> = ndc.iter().map(|n| (1.0 - (n.y * 0.5 + 0.5)) * H as f32).collect();
        let (minx, maxx) = (sx.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize, (sx.iter().cloned().fold(f32::MIN, f32::max).ceil().min(W as f32 - 1.0)) as usize);
        let (miny, maxy) = (sy.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize, (sy.iter().cloned().fold(f32::MIN, f32::max).ceil().min(H as f32 - 1.0)) as usize);
        let nrm: Vec<Vec3> = tri.v.iter().map(|v| tri.model.transform_vector3(Vec3::from_array(v.normal)).normalize_or_zero()).collect();
        let d = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sx[2] - sx[0]) * (sy[1] - sy[0]);
        if d.abs() < 1e-9 { continue; }
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w1 = ((px - sx[0]) * (sy[2] - sy[0]) - (sx[2] - sx[0]) * (py - sy[0])) / d;
                let w2 = ((sx[1] - sx[0]) * (py - sy[0]) - (px - sx[0]) * (sy[1] - sy[0])) / d;
                let w0 = 1.0 - w1 - w2;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 { continue; }
                let z = w0 * ndc[0].z + w1 * ndc[1].z + w2 * ndc[2].z;
                if z < 0.0 || z >= depth[y * W + x] { continue; }
                depth[y * W + x] = z;
                let n = (nrm[0] * w0 + nrm[1] * w1 + nrm[2] * w2).normalize_or_zero();
                let pos = wp[0] * w0 + wp[1] * w1 + wp[2] * w2;
                let albedo: Vec<f32> = (0..3).map(|k| tri.v[0].color[k] * w0 + tri.v[1].color[k] * w1 + tri.v[2].color[k] * w2).collect();
                let ndl = n.dot(sun).max(0.0);
                let dist = (cam - pos).length();
                let fog = 1.0 - (-dist * 0.0035f32).exp();
                let mut c = [0.0f32; 3];
                for k in 0..3 {
                    let lit = albedo[k] * (sky.sun_color[k] * sky.sun_intensity * ndl / std::f32::consts::PI + sky.ambient[k]);
                    c[k] = aces(lit * (1.0 - fog) + sky.sky_horizon[k] * fog);
                }
                color[y * W + x] = c;
            }
        }
    }
    let mut ppm = format!("P6\n{W} {H}\n255\n").into_bytes();
    for c in &color { for k in 0..3 { ppm.push(srgb(c[k])); } }
    std::fs::write(&out, ppm).unwrap();
    println!("tris front {front} back-culled {back} clipped {clipped}; cam {cam:?} -> {target:?}; ground y {}", ppos.y);
}
