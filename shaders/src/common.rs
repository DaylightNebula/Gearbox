//! Shader types shared across materials: the camera uniform bound in bind group 2.

use bytemuck::{Pod, Zeroable};
use magician_vgpu::{macros::*, rust::{macros::*, *}};

/// The bindable shader group exposing a [`Camera`] uniform, bound at group 2 by
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

/// The bindless-array fallback: a single growable texture atlas plus its current
/// pixel dimensions, used to remap a permanent per-material pixel-space rect into a
/// normalized atlas UV. See [`gearbox::AtlasTextureVault`](../../src/vault/atlas_textures.rs).
#[derive(ShaderGroup, BindableObject)]
pub struct AtlasTextures {
    pub atlas: Texture2D,
    /// (width, height, unused, unused), in pixels; only changes when the atlas grows.
    #[uniform] pub atlas_size: Vec4,
    pub global_sampler: Sampler
}

#[derive(ShaderGroup, BindableObject)]
pub struct EmptyBindable {}
