use bytemuck::{Pod, Zeroable};
use wgpu::{util::DeviceExt, Buffer, Device};

/// Per-vertex data: position, normal, uv, tangent (w = handedness), vertex colour.
/// Tangent is included for normal-mapped characters.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
    pub color: [f32; 4],
}

impl Vertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3,  // position
        1 => Float32x3,  // normal
        2 => Float32x2,  // uv
        3 => Float32x4,  // tangent
        4 => Float32x4,  // color
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct Mesh {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
    pub material_index: Option<usize>,
}

impl Mesh {
    /// Uploads CPU geometry (e.g. a baked character) as a drawable mesh.
    pub fn from_cpu(device: &Device, vertices: &[Vertex], indices: &[u32]) -> Mesh {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_mesh_vb"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_mesh_ib"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Mesh { vertex_buffer, index_buffer, index_count: indices.len() as u32, material_index: None }
    }
}

pub struct GltfScene {
    pub meshes: Vec<Mesh>,
}

impl GltfScene {
    /// Load a .glb from bytes (Android: read from asset manager, desktop: read file).
    pub fn from_bytes(device: &Device, queue: &wgpu::Queue, bytes: &[u8]) -> anyhow::Result<Self> {
        let (doc, buffers, _images) = gltf::import_slice(bytes)?;
        let mut meshes = Vec::new();

        for mesh in doc.meshes() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));

                let positions: Vec<[f32; 3]> = reader
                    .read_positions()
                    .map(|p| p.collect())
                    .unwrap_or_default();

                let normals: Vec<[f32; 3]> = reader
                    .read_normals()
                    .map(|n| n.collect())
                    .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

                let uvs: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .map(|t| t.into_f32().collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

                let tangents: Vec<[f32; 4]> = reader
                    .read_tangents()
                    .map(|t| t.collect())
                    .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 1.0]; positions.len()]);

                let colors: Vec<[f32; 4]> = reader
                    .read_colors(0)
                    .map(|c| c.into_rgba_f32().collect())
                    .unwrap_or_else(|| vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]);

                let vertices: Vec<Vertex> = (0..positions.len())
                    .map(|i| Vertex {
                        position: positions[i],
                        normal: normals[i],
                        uv: uvs[i],
                        tangent: tangents[i],
                        color: colors[i],
                    })
                    .collect();

                let indices: Vec<u32> = reader
                    .read_indices()
                    .map(|ri| ri.into_u32().collect())
                    .unwrap_or_default();

                if indices.is_empty() {
                    continue;
                }

                let vertex_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("mesh_vertex_buffer"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                let index_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("mesh_index_buffer"),
                        contents: bytemuck::cast_slice(&indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });

                meshes.push(Mesh {
                    vertex_buffer,
                    index_buffer,
                    index_count: indices.len() as u32,
                    material_index: primitive.material().index(),
                });
            }
        }

        Ok(GltfScene { meshes })
    }
}
