//! Writes the baked bind-pose character to OBJ files for inspection.
//! usage: cargo run -p gtm-render --example dump_character -- <in.glb> <out_prefix> [mesh-name-filter]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).expect("read glb");
    let meshes = gtm_render::character::load_bind_pose(&bytes, 1.8, args.get(3).map(|s| s.as_str())).expect("load");
    for (i, m) in meshes.iter().enumerate() {
        let mut s = String::new();
        for v in &m.vertices {
            s += &format!("v {} {} {}\n", v.position[0], v.position[1], v.position[2]);
        }
        for t in m.indices.chunks(3) {
            s += &format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1);
        }
        std::fs::write(format!("{}_{}.obj", args[2], i), s).unwrap();
        println!("{} verts={} tris={}", m.name, m.vertices.len(), m.indices.len() / 3);
    }
}
