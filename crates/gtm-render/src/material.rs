use bytemuck::{Pod, Zeroable};

/// Flags packed into MaterialUniform.flags
pub const FLAG_HAS_ALBEDO:          u32 = 1 << 0;
pub const FLAG_HAS_NORMAL_MAP:      u32 = 1 << 1;
pub const FLAG_HAS_METALLIC_ROUGH:  u32 = 1 << 2;
pub const FLAG_HAS_EMISSIVE:        u32 = 1 << 3;
pub const FLAG_DOUBLE_SIDED:        u32 = 1 << 4;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color_factor: [f32; 4],
    /// x = metallic, y = roughness, z = occlusion_strength, w = unused
    pub metallic_roughness: [f32; 4],
    pub emissive_factor: [f32; 3],
    pub flags: u32,
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            metallic_roughness: [0.0, 0.5, 1.0, 0.0],
            emissive_factor: [0.0, 0.0, 0.0],
            flags: 0,
        }
    }
}

pub struct Material {
    pub uniform: MaterialUniform,
    pub albedo_texture: Option<wgpu::Texture>,
    pub normal_texture: Option<wgpu::Texture>,
    pub metallic_roughness_texture: Option<wgpu::Texture>,
    pub bind_group: Option<wgpu::BindGroup>,
    pub uniform_buffer: wgpu::Buffer,
}
