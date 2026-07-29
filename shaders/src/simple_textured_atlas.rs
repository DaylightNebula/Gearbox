//! Fragment shader for `gearbox::SimpleTexturedMaterial` when running on the atlas-based
//! texture vault fallback (no `TEXTURE_BINDING_ARRAY` support): samples a single, arbitrarily
//! growable atlas texture, remapping via a permanent per-material pixel-space rect.

use magician_vgpu::{macros::BindableObject, rust::{macros::*, *}};

use crate::{basic_vertex::VertexOutput, common::{AtlasTextures, CameraInput}};

/// The bindable shader group exposing `gearbox::SimpleTexturedMaterial`'s atlas placement,
/// bound at group 0. `rect` is `(offset_x, offset_y, size_x, size_y)` in atlas pixels.
#[derive(ShaderGroup, BindableObject)]
pub struct SimpleTexturedAtlasMaterial {
    #[uniform] pub rect: Vec4
}

/// Output of [`simple_textured_atlas_main`]: the final fragment color.
#[allow(unused)]
#[derive(ShaderLayout)]
pub struct FragmentOutput {
    #[location = 0] color: Vec4
}

/// Remaps the interpolated UV coordinate into the material's atlas rect, then samples it.
#[shader("./shader_out", fragment)]
pub fn simple_textured_atlas_main(
    #[group = 0] material: SimpleTexturedAtlasMaterial,
    #[group = 2] _cam_in: CameraInput,
    #[group = 1] textures: AtlasTextures,
    input: VertexOutput
) -> FragmentOutput {
    let atlas_uv = (material.rect.xy() + input.uvs * material.rect.zw()) / textures.atlas_size.xy();
    let color = textureSample(textures.atlas, textures.global_sampler, atlas_uv);
    return FragmentOutput { color };
}
