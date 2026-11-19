//! Loads a skinned glTF character and bakes it to its bind pose on the CPU.
//! All geometry comes from the .glb; nothing here authors a character shape.

use crate::mesh::Vertex;
use glam::{Mat4, Vec3};

#[derive(Clone, Debug)]
pub struct CharacterMesh {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

fn node_world_matrices(doc: &gltf::Document) -> Vec<Mat4> {
    fn visit(node: gltf::Node, parent: Mat4, out: &mut [Mat4]) {
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        out[node.index()] = world;
        for child in node.children() {
            visit(child, world, out);
        }
    }
    let mut out = vec![Mat4::IDENTITY; doc.nodes().len()];
    for scene in doc.scenes() {
        for node in scene.nodes() {
            visit(node, Mat4::IDENTITY, &mut out);
        }
    }
    out
}

/// Bind-pose meshes, scaled uniformly so the whole character is `target_height` tall,
/// feet on y = 0, centered on x/z. All meshes share one transform so they stay aligned.
pub fn load_bind_pose(
    bytes: &[u8],
    target_height: f32,
    name_contains: Option<&str>,
) -> anyhow::Result<Vec<CharacterMesh>> {
    anyhow::ensure!(target_height.is_finite() && target_height > 0.0, "bad target height");
    let (doc, buffers, _images) = gltf::import_slice(bytes)?;
    let worlds = node_world_matrices(&doc);
    let mut out: Vec<CharacterMesh> = Vec::new();

    for node in doc.nodes() {
        let Some(mesh) = node.mesh() else { continue };
        if let Some(filter) = name_contains {
            if !mesh.name().unwrap_or("").contains(filter) {
                continue;
            }
        }
        let node_world = worlds[node.index()];
        let joint_mats: Option<Vec<Mat4>> = node.skin().map(|skin| {
            let reader = skin.reader(|b| Some(&buffers[b.index()]));
            let ibms: Vec<[[f32; 4]; 4]> =
                reader.read_inverse_bind_matrices().map(|i| i.collect()).unwrap_or_default();
            skin.joints()
                .enumerate()
                .map(|(i, j)| {
                    let ibm = ibms.get(i).map(Mat4::from_cols_array_2d).unwrap_or(Mat4::IDENTITY);
                    worlds[j.index()] * ibm
                })
                .collect()
        });

        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(|b| Some(&buffers[b.index()]));
            let Some(positions) = reader.read_positions() else { continue };
            let positions: Vec<[f32; 3]> = positions.collect();
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|n| n.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
            anyhow::ensure!(normals.len() == positions.len(), "normal count mismatch");
            let joints: Option<Vec<[u16; 4]>> = reader.read_joints(0).map(|j| j.into_u16().collect());
            let weights: Option<Vec<[f32; 4]>> = reader.read_weights(0).map(|w| w.into_f32().collect());
            let color = prim.material().pbr_metallic_roughness().base_color_factor();
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|t| t.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            let mut vertices = Vec::with_capacity(positions.len());
            for (i, p) in positions.iter().enumerate() {
                let skin_m = match (&joint_mats, &joints, &weights) {
                    (Some(jm), Some(js), Some(ws)) if i < js.len() && i < ws.len() => {
                        let mut acc = Mat4::ZERO;
                        let mut total = 0.0f32;
                        for k in 0..4 {
                            let w = ws[i][k];
                            if w > 0.0 {
                                if let Some(m) = jm.get(js[i][k] as usize) {
                                    acc += *m * w;
                                    total += w;
                                }
                            }
                        }
                        (total > 1e-6).then_some(acc)
                    }
                    _ => None,
                };
                let m = skin_m.unwrap_or(node_world);
                let wp = m.transform_point3(Vec3::from_array(*p));
                let wn = m.transform_vector3(Vec3::from_array(normals[i])).normalize_or_zero();
                anyhow::ensure!(wp.is_finite() && wn.is_finite(), "non-finite vertex");
                vertices.push(Vertex {
                    position: wp.to_array(),
                    normal: if wn == Vec3::ZERO { [0.0, 1.0, 0.0] } else { wn.to_array() },
                    uv: uvs[i],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    color,
                });
            }
            let base_indices: Vec<u32> = match reader.read_indices() {
                Some(idx) => idx.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            anyhow::ensure!(
                base_indices.len() % 3 == 0 && base_indices.iter().all(|&i| (i as usize) < vertices.len()),
                "bad indices"
            );
            out.push(CharacterMesh {
                name: mesh.name().unwrap_or("mesh").to_string(),
                vertices,
                indices: base_indices,
            });
        }
    }
    anyhow::ensure!(!out.is_empty(), "model has no triangle meshes");

    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for m in &out {
        for v in &m.vertices {
            let p = Vec3::from_array(v.position);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let height = hi.y - lo.y;
    anyhow::ensure!(height.is_finite() && height > 1e-6, "model has zero height");
    let scale = target_height / height;
    let (cx, cz) = ((lo.x + hi.x) * 0.5, (lo.z + hi.z) * 0.5);
    for m in &mut out {
        for v in &mut m.vertices {
            v.position = [(v.position[0] - cx) * scale, (v.position[1] - lo.y) * scale, (v.position[2] - cz) * scale];
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(bytes: &[u8], filter: Option<&str>) {
        let meshes = load_bind_pose(bytes, 1.8, filter).expect("loads");
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        let mut tris = 0;
        for m in &meshes {
            tris += m.indices.len() / 3;
            for v in &m.vertices {
                lo = lo.min(v.position[1]);
                hi = hi.max(v.position[1]);
            }
        }
        assert!(tris > 500, "only {tris} triangles");
        assert!(lo.abs() < 1e-4, "feet at {lo}");
        assert!((hi - 1.8).abs() < 1e-3, "height {hi}");
    }

    #[test]
    fn template_player_male_loads() {
        check(include_bytes!("../../../assets/third_party/defy/characters/template_player.glb"), Some("Male"));
    }

    #[test]
    fn name_filter_selects_one_mesh() {
        let bytes = include_bytes!("../../../assets/third_party/defy/characters/template_player.glb");
        let male = load_bind_pose(bytes, 1.8, Some("Male")).unwrap();
        assert_eq!(male.len(), 1);
        assert!(load_bind_pose(bytes, 1.8, Some("NoSuchMesh")).is_err());
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(load_bind_pose(&[0u8; 16], 1.8, None).is_err());
    }
}
