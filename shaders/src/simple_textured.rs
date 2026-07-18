//! Fragment shader for `gearbox::SimpleTexturedMaterial`: samples a single albedo texture.

use magician_vgpu::{macros::BindableObject, rust::{macros::*, *}};

use crate::{basic_vertex::VertexOutput, common::CameraInput};

/// The bindable shader group exposing `gearbox::SimpleTexturedMaterial`'s texture
/// and sampler, bound at group 0.
#[derive(ShaderGroup, BindableObject)]
pub struct SimpleTexturedMaterial {
    pub albedo_texture: Texture2D,
    pub albedo_sampler: Sampler
}

/// Output of [`simple_textured_main`]: the final fragment color.
#[allow(unused)]
#[derive(ShaderLayout)]
pub struct FragmentOutput {
    #[location = 0] color: Vec4
}

/// Samples the albedo texture at the interpolated UV coordinate, ignoring lighting
/// and camera input.
#[shader("./shader_out", fragment)]
pub fn simple_textured_main(
    #[group = 0] material: SimpleTexturedMaterial,
    #[group = 1] _cam_in: CameraInput,
    input: VertexOutput
) -> FragmentOutput {
    let color = textureSample(material.albedo_texture, material.albedo_sampler, input.uvs);
    return FragmentOutput { color };
}