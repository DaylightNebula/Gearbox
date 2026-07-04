use magician_vgpu::{macros::BindableObject, rust::{macros::*, *}};

use crate::{basic_vertex::VertexOutput, common::CameraInput};

#[derive(ShaderGroup, BindableObject)]
pub struct BasicMaterial {
    #[uniform] pub diffuse: Vec4
}

#[allow(unused)]
#[derive(ShaderLayout)]
pub struct FragmentOutput {
    #[location = 0] color: Vec4
}

#[shader("./shader_out", fragment)]
pub fn primary_fs_main(
    #[group = 0] material: BasicMaterial,
    #[group = 1] _cam_in: CameraInput,
    _input: VertexOutput
) -> FragmentOutput {
    return FragmentOutput { color: material.diffuse };
}