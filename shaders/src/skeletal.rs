//! The skinning vertex shader used by `gearbox::skeletal` (the `skeletal` feature): blends
//! up to 4 bone matrices per vertex, or attaches unskinned meshes rigidly to a bone.

use bytemuck::{Pod, Zeroable};
use magician_macros::{ShaderGroup, ShaderLayout, shader};
use magician_rust::{Mat4, UVec4, Vec2, Vec3, Vec4, length_vec4};
use magician_vgpu::macros::{BindableObject, UniformBufferObject};

use crate::common::CameraInput;

/// Per-vertex data for the skeletal vertex shader: position, UV, normal, and up
/// to 4 skin weights with the joint indices they apply to. Layout matches
/// `gearbox::skeletal::vertex_buffer_layout()`.
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, ShaderLayout)]
pub struct SkeletalVertexInput {
    #[location = 0] pub position: Vec3,
    #[location = 1] pub uvs: Vec2,
    #[location = 2] pub normal: Vec3,
    #[location = 3] pub weights: Vec4,
    #[location = 4] pub joints: UVec4
}

/// Per-instance model matrix, passed as 4 `Vec4` rows (`mm0..mm3`) since a
/// `Mat4` can't be bound as a single vertex attribute. Layout matches
/// `gearbox::skeletal::instance_buffer_layout()`.
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, ShaderLayout)]
pub struct SkeletalInstanceInput {
    #[location = 5] pub mm0: Vec4,
    #[location = 6] pub mm1: Vec4,
    #[location = 7] pub mm2: Vec4,
    #[location = 8] pub mm3: Vec4
}

#[derive(ShaderLayout)]
pub struct SkeletalVertexOutput {
    #[builtin = "position"] pub clip_position: Vec4,
    #[location = 0] pub uvs: Vec2,
    #[location = 1] pub color: Vec4,
    #[location = 2] pub world_normal: Vec3,
    #[location = 3] pub world_position: Vec3
}


/// Bind group (group 3) wrapping `AnimationInfo`, bound per-bone while drawing
/// a `SkeletalMeshHandle` (see `SkeletalAnimationBuffers`).
#[derive(ShaderGroup, BindableObject)]
pub struct AnimationInfoInput {
    #[uniform] pub info: AnimationInfo
}

/// This frame's world matrices for one skeleton, capped at 128 joints/nodes (16 KiB, the
/// most a uniform buffer is guaranteed to hold on WebGL2). `MAX_SKELETAL_BONES` in gearbox
/// must match.
/// `bones[i]` is `world_matrix(i) * inverse_bind_pose(i)`, used to skin
/// vertices with non-zero weights. `nodes[i]` is the plain world matrix of
/// node `i`, used for rigid (unskinned) meshes attached directly to a bone
/// (props, weapons, etc). See `skeletal_vertex_main` for how the two are chosen.
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, UniformBufferObject)]
pub struct AnimationInfo {
    pub bones: [Mat4; 128],
    pub nodes: [Mat4; 128]
}

/// Skins the vertex by its joint weights if it has any (blending up to 4 bone
/// matrices), otherwise treats it as rigidly attached to its first joint's node
/// (a prop/weapon parented to a bone rather than a real skinned mesh vertex).
#[shader("./shader_out", vertex)]
pub fn skeletal_vertex_main(
    #[group = 3] anim_info: AnimationInfoInput,
    #[group = 2] cam_in: CameraInput,
    model: SkeletalVertexInput,
    instance: SkeletalInstanceInput
) -> SkeletalVertexOutput {
    let mut mm = Mat4::new(instance.mm0, instance.mm1, instance.mm2, instance.mm3);

    if length_vec4(model.weights) > 0.0 {
        // mm = (model.weights.x() * anim_info.info.bones[model.joints.x])
        //     + (model.weights.y() * anim_info.info.bones[model.joints.y])
        //     + (model.weights.z() * anim_info.info.bones[model.joints.z])
        //     + (model.weights.w() * anim_info.info.bones[model.joints.w]);
        // let bone_mat = 
        //     (anim_info.info.bones[model.joints.x() as usize] * model.weights.x()) + 
        //     (anim_info.info.bones[model.joints.y() as usize] * model.weights.y()) +
        //     (anim_info.info.bones[model.joints.z() as usize] * model.weights.z()) +
        //     (anim_info.info.bones[model.joints.w() as usize] * model.weights.w());
        let bone_mat = 
            (model.weights.x() * anim_info.info.bones[model.joints.x() as usize]) + 
            (model.weights.y() * anim_info.info.bones[model.joints.y() as usize]) + 
            (model.weights.z() * anim_info.info.bones[model.joints.z() as usize]) + 
            (model.weights.w() * anim_info.info.bones[model.joints.w() as usize]);
        mm = mm * bone_mat;
    } else {
        mm = mm * anim_info.info.nodes[model.joints.x() as usize];
    }

    let world_position = mm * Vec4::from_vec3_w(model.position, 1.0);

    return SkeletalVertexOutput {
        clip_position: cam_in.camera.view_proj * world_position, 
        uvs: model.uvs, 
        color: Vec4::new(1.0, 1.0, 1.0, 1.0), 
        world_normal: model.normal, 
        world_position: world_position.xyz()
    };
}
