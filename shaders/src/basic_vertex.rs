//! The standard vertex shader used by every mesh/material pairing in `gearbox`:
//! transforms a per-vertex position by a per-instance model matrix and the camera's
//! view-projection matrix.

use bytemuck::{Pod, Zeroable};
use magician_vgpu::rust::{macros::*, *};

use crate::{common::CameraInput};

/// Per-vertex mesh attributes, matching `gearbox::BasicMesh`'s vertex buffer layout.
#[repr(C)]
#[derive(Default, Pod, Zeroable, Clone, Copy, ShaderLayout)]
pub struct VertexInput {
    #[location = 0] pub position: Vec3,
    #[location = 1] pub uvs: Vec2,
    #[location = 2] pub normals: Vec3
}

/// Per-instance model matrix, passed as four `vec4` rows (`mm0..mm3`) since a
/// `mat4` vertex attribute must be split across four shader locations.
#[derive(ShaderLayout)]
pub struct InstanceInput {
    #[location = 3] pub mm0: Vec4,
    #[location = 4] pub mm1: Vec4,
    #[location = 5] pub mm2: Vec4,
    #[location = 6] pub mm3: Vec4
}

/// Output of [`primary_vs_main`], consumed by the matching fragment shader.
#[allow(unused)]
#[derive(ShaderLayout)]
pub struct VertexOutput {
    #[builtin = "position"] pub clip_position: Vec4,
    #[location = 0] pub uvs: Vec2
}

/// Transforms `model.position` by the instance's model matrix and the camera's
/// view-projection matrix, passing UVs through unchanged.
#[shader("./shader_out", vertex)]
pub fn primary_vs_main(
    #[group = 2] cam_in: CameraInput,
    model: VertexInput,
    instance: InstanceInput
) -> VertexOutput {
    let mm = Mat4::new(instance.mm0, instance.mm1, instance.mm2, instance.mm3);
    let world_position = mm * Vec4::from_vec3_w(model.position, 1.0);

    return VertexOutput { 
        clip_position: cam_in.camera.view_proj * world_position, 
        uvs: model.uvs
    };
}
