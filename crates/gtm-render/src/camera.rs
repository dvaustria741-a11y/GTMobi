use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub view_pos: [f32; 4],
}

pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fovy_deg: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            position: Vec3::new(0.0, 2.0, 8.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fovy_deg: 70.0,
            aspect,
            near: 0.1,
            far: 2000.0,
        }
    }

    pub fn to_uniform(&self) -> CameraUniform {
        let view = Mat4::look_at_rh(self.position, self.target, self.up);
        let proj = Mat4::perspective_rh(
            self.fovy_deg.to_radians(),
            self.aspect,
            self.near,
            self.far,
        );
        let view_proj = proj * view;
        CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            view_pos: [self.position.x, self.position.y, self.position.z, 1.0],
        }
    }
}
