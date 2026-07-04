use bytemuck::{Pod, Zeroable};
use magician_vgpu::{macros::*, rust::{macros::*, *}};

#[derive(ShaderGroup, BindableObject)]
pub struct CameraInput {
    #[uniform] pub camera: Camera
}

#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, UniformBufferObject)]
pub struct Camera {
    pub view_pos: Vec4,
    pub view_proj: Mat4
}