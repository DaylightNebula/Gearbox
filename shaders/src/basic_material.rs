//! Fragment shader for `gearbox::BasicMaterial`: outputs a flat, unlit color.

use magician_vgpu::{macros::BindableObject, rust::{macros::*, *}};

use crate::{basic_vertex::VertexOutput, common::CameraInput};

/// The bindable shader group exposing `gearbox::BasicMaterial`'s flat color, bound
/// at group 0.
#[derive(ShaderGroup, BindableObject)]
pub struct BasicMaterial {
    #[uniform] pub diffuse: Vec4
}

/// Output of [`primary_fs_main`]: the final fragment color.
#[allow(unused)]
#[derive(ShaderLayout)]
pub struct FragmentOutput {
    #[location = 0] color: Vec4
}

/// Outputs the material's diffuse color unchanged, ignoring lighting and camera input.
#[shader("./shader_out", fragment)]
pub fn primary_fs_main(
    #[group = 0] material: BasicMaterial,
    #[group = 2] _cam_in: CameraInput,
    _input: VertexOutput
) -> FragmentOutput {
    return FragmentOutput { color: material.diffuse };
}