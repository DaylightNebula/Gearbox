//! Shader types shared across materials: the camera uniform bound in bind group 1.

use bytemuck::{Pod, Zeroable};
use magician_vgpu::{macros::*, rust::{macros::*, *}};

/// The bindable shader group exposing a [`Camera`] uniform, bound at group 1 by
/// every material/mesh pipeline in `gearbox`.
#[derive(ShaderGroup, BindableObject)]
pub struct CameraInput {
    #[uniform] pub camera: Camera
}

/// GPU-side layout of camera data: world-space view position and the combined
/// view-projection matrix, written each frame by `gearbox::Camera::update`.
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, UniformBufferObject)]
pub struct Camera {
    pub view_pos: Vec4,
    pub view_proj: Mat4
}

#[derive(ShaderGroup, BindableObject)]
pub struct BindlessTextures {
    pub textures: BindlessArray<Texture2D>,
    pub global_sampler: Sampler
}

#[derive(ShaderGroup, BindableObject)]
pub struct EmptyBindable {}
