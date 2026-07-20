//! Fragment shader for `gearbox::SimpleTexturedMaterial`: samples a single albedo texture.

use magician_vgpu::{macros::BindableObject, rust::{macros::*, *}};

use crate::{basic_vertex::VertexOutput, common::{BindlessTextures, CameraInput}};

/// The bindable shader group exposing `gearbox::SimpleTexturedMaterial`'s texture
/// and sampler, bound at group 0.
#[derive(ShaderGroup, BindableObject)]
pub struct SimpleTexturedMaterial {
    #[uniform] pub texture_id: u32
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
    #[group = 2] textures: BindlessTextures,
    input: VertexOutput
) -> FragmentOutput {
    let color = textureSample(textures.textures[material.texture_id as usize], textures.global_sampler, input.uvs);
    return FragmentOutput { color };
}