use bytemuck::{Pod, Zeroable};
use magician_vgpu::rust::{macros::*, *};

use crate::{common::CameraInput};

#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, ShaderLayout)]
pub struct VertexInput {
    #[location = 0] pub position: Vec3,
    #[location = 1] pub tex_coords: Vec2
}


#[derive(ShaderLayout)]
pub struct InstanceInput {
    #[location = 2] pub mm0: Vec4,
    #[location = 3] pub mm1: Vec4,
    #[location = 4] pub mm2: Vec4,
    #[location = 5] pub mm3: Vec4
}


#[allow(unused)]
#[derive(ShaderLayout)]
pub struct VertexOutput {
    #[builtin = "position"] clip_position: Vec4,
    #[location = 0] tex_coords: Vec2
}


#[shader("./shader_out", vertex)]
pub fn primary_vs_main(
    #[group = 1] cam_in: CameraInput,
    model: VertexInput,
    instance: InstanceInput
) -> VertexOutput {
    let mm = Mat4::new(instance.mm0, instance.mm1, instance.mm2, instance.mm3);
    let world_position = mm * Vec4::from_vec3_w(model.position, 1.0);

    return VertexOutput { 
        clip_position: cam_in.camera.view_proj * world_position, 
        tex_coords: model.tex_coords
    };
}
