use anarchy::{ComponentMeta, extract_comps_distributed, macros::Getters};
use magician_vgpu::{DrawSettings, ImmutableBuffer, MutableBuffer, Pipeline, PipelineBuilder, ShaderSource, ShaderType, SinglePass, VirtualGpu, WritableBuffer, glam::Mat4};
use mutual::{CastableSharedData, CowData, RefCastGuard};
use wgpu::BufferUsages;

use crate::{Mesh, Transform, shaders};


/// A basic mesh with a simple vertex determined by `shaders::common::VertexInput` 
/// and a `Mat4` instance.
#[derive(Getters)]
pub struct BasicMesh {
    pub vertex_buffer: ImmutableBuffer<[shaders::basic_vertex::VertexInput]>,
    pub index_buffer: ImmutableBuffer<[u32]>,
    pub instance_buffer: CowData<MutableBuffer<[Mat4]>>
}

impl BasicMesh {
    pub fn from_raw(
        vertex_buffer: ImmutableBuffer<[shaders::basic_vertex::VertexInput]>, 
        index_buffer: ImmutableBuffer<[u32]>
    ) -> Self {
        Self { vertex_buffer, index_buffer, instance_buffer: CowData::null() }
    }

    pub fn new(vgpu: &VirtualGpu, vertices: &[shaders::basic_vertex::VertexInput], indices: &[u32]) -> Self {
        Self {
            vertex_buffer: ImmutableBuffer::new(vgpu, vertices, BufferUsages::VERTEX),
            index_buffer: ImmutableBuffer::new(vgpu, indices, BufferUsages::INDEX),
            instance_buffer: CowData::null()
        }
    }
}

impl Mesh for BasicMesh {
    fn create_pipeline<'a>(
        &'a self, 
        _vgpu: &VirtualGpu
    ) -> PipelineBuilder<'a> {
        Pipeline::builder("Normal Shader")
            .source(
                ShaderType::Vertex, 
                ShaderSource {
                    source: shaders::basic_vertex::SHADER_primary_vs_main.into(),
                    main_function: "primary_vs_main".into()
                }
            )
            .vertex(vertex_buffer_layout())
            .vertex(instance_buffer_layout())
    }

    fn draw<'a>(
        &'a self,
        vgpu: &VirtualGpu,
        pass: &mut SinglePass<'a>, 
        entity: &'a anarchy::Entity
    ) {
        // extract transform and mesh components
        let (mut comps, _ctx) = extract_comps_distributed(
            entity, 
            &[Transform::bit_mask()], 
            None
        );
        let transform: RefCastGuard<_, Transform> = comps.next().flatten()
            .expect("BasicMaterial requires Transform companion component").lock_cast_ref();

        // create instance matrix to draw
        let instances = [
            Mat4::from_scale_rotation_translation(
                transform.scale, 
                transform.rotation, 
                transform.translation
            )
        ];

        // create or update instance buffer
        if self.instance_buffer.is_null() {
            self.instance_buffer.set(MutableBuffer::new(
                vgpu, 
                &instances, 
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST
            ));
        } else {
            self.instance_buffer.get_ref().write(vgpu, &instances)
                .expect("Failed to update instance buffer");
        }

        pass.bind_instances(&*self.instance_buffer.get_ref());
        pass.draw(
            &self.vertex_buffer,
            &self.index_buffer,
            DrawSettings::default()
        )
    }
}

fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    use std::mem;
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<shaders::basic_vertex::VertexInput>() as wgpu::BufferAddress,
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
        ],
    }
}

fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    use std::mem;
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<shaders::basic_vertex::InstanceInput>() as wgpu::BufferAddress,
        // We need to switch from using a step mode of Vertex to Instance
        // This means that our shaders will only change to use the next
        // instance when the shader starts processing a new instance
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                // While our vertex shader only uses locations 0, and 1 now, in later tutorials we'll
                // be using 2, 3, and 4, for Vertex. We'll start at slot 5 not conflict with them later
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
            // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
            // for each vec4. We don't have to do this in code though.
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            }
        ],
    }
}
