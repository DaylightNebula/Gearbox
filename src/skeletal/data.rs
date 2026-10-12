use ahash::AHashMap;
use magician_vgpu::glam::*;

/// A material as decoded from a loaded model file (glTF/FBX), before it is
/// uploaded to the GPU. Textures are kept as raw encoded bytes (e.g. PNG);
/// the vault turns the `albedo_texture` into a `SimpleTexturedMaterial` handle.
#[derive(Default, Debug, Clone)]
pub struct SkeletalMaterial {
    pub albedo_texture: Option<Vec<u8>>,
    pub roughness_texture: Option<Vec<u8>>,
    pub emissive_texture: Option<Vec<u8>>,
    pub normal_texture: Option<Vec<u8>>,
    pub occlusion_texture: Option<Vec<u8>>,
    pub albedo_color: [f32; 4],
    pub emissive_color: [f32; 3],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub alpha_mode: f32,
    pub alpha_cutoff: f32
}

/// A node of a model that contains an id, meshes, and children nodes.
///
/// `id` is the node's index as assigned by the source file's loader, used to key
/// `Animation`/`Channel` data and the computed bone matrices during rendering.
/// `mesh` is an optional reference into the owning `SkeletalMesh`'s `meshes` map
/// (keyed by that source format's mesh index), based on the path format
/// `"{file path}#Mesh{mesh id}"`. `children` are this node's child nodes.
#[derive(Debug, Clone)]
pub struct ModelBone {
    pub id: u16,
    pub transform: Mat4,
    pub mesh: Option<usize>,
    pub children: Vec<ModelBone>,
}

/// Contains all information related to a single hard-coded animation.
#[derive(Debug, Clone)]
pub struct Animation {
    pub name: Option<String>,
    pub channels: AHashMap<usize, Channel>,
    pub length: f32
}

impl Animation {
    pub fn from_preprocessed_animation(
        animation: &PreProcessAnimation,
        node_id_map: &AHashMap<String, usize>,
        warn_not_found: bool
    ) -> Self {
        let channels = animation.channels
            .iter()
            .filter_map(|(id, channel)| {
                let Some(id) = node_id_map.get(id) else {
                    if warn_not_found {
                        println!("No animated node with id {} found for animations", id);
                    }
                    return None
                };

                Some((
                    *id,
                    Channel {
                        id: *id,
                        positions: channel.positions.clone(),
                        rotations: channel.rotations.clone(),
                        scale: channel.scale.clone()
                    }
                ))
            })
            .collect::<AHashMap<usize, Channel>>();

        Self {
            name: animation.name.clone(),
            channels,
            length: animation.length
        }
    }
}

/// A channel contains the interpolation information for a single node/bone.
#[derive(Debug, Clone)]
pub struct Channel {
    pub id: usize,
    pub positions: Option<(Interpolation, Vec<f32>, Vec<Vec3>)>,
    pub rotations: Option<(Interpolation, Vec<f32>, Vec<Quat>)>,
    pub scale: Option<(Interpolation, Vec<f32>, Vec<Vec3>)>,
}

/// All forms of supported interpolation between two keyframes.
///
/// `Linear` blends directly between the two values. `Step` holds the first
/// keyframe's value until the next keyframe is reached (no blending). `Wave`
/// eases in/out with a sine curve instead of blending linearly; it is also
/// what glTF's `CubicSpline` interpolation is mapped to, since true cubic
/// spline (in/out tangents) interpolation isn't implemented.
#[derive(Debug, Clone, Copy)]
pub enum Interpolation {
    Linear,
    Wave,
    Step,
}

/// Contains all information related to a single hard-coded animation.
#[derive(Debug, Clone)]
pub struct PreProcessAnimation {
    pub name: Option<String>,
    pub channels: AHashMap<String, PreProcessChannel>,
    pub length: f32
}

/// A channel contains the interpolation information for a single node/bone.
#[derive(Debug, Clone)]
pub struct PreProcessChannel {
    pub positions: Option<(Interpolation, Vec<f32>, Vec<Vec3>)>,
    pub rotations: Option<(Interpolation, Vec<f32>, Vec<Quat>)>,
    pub scale: Option<(Interpolation, Vec<f32>, Vec<Vec3>)>,
}

/// The per-vertex buffer layout (position, uvs, normal, skin weights, joint indices)
/// matching `crate::shaders::skeletal::SkeletalVertexInput`, at shader locations 0-4.
pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    use std::mem;
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<crate::shaders::skeletal::SkeletalVertexInput>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                shader_location: 4,
                format: wgpu::VertexFormat::Uint32x4,
            }
        ]
    }
}

/// The per-instance buffer layout (a `Mat4` model matrix, one row per shader
/// location) matching `crate::shaders::skeletal::SkeletalInstanceInput`, at shader locations 5-8.
pub fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    use std::mem;
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<crate::shaders::skeletal::SkeletalInstanceInput>() as wgpu::BufferAddress,
        // We need to switch from using a step mode of Vertex to Instance
        // This means that our shaders will only change to use the next
        // instance when the shader starts processing a new instance
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                // While our vertex shader only uses locations 0, and 1 now, in later tutorials we'll
                // be using 2, 3, and 4, for Vertex. We'll start at slot 5 not conflict with them later
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            },
            // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
            // for each vec4. We don't have to do this in code though.
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                shader_location: 6,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                shader_location: 7,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                shader_location: 8,
                format: wgpu::VertexFormat::Float32x4,
            }
        ],
    }
}
