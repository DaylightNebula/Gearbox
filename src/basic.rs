use anarchy::{ComponentMeta, extract_comps_distributed, macros::Getters};
use magician_vgpu::{BindGroupProvider, BindableObject, DrawSettings, MutableBuffer, Pipeline, ShaderSource, ShaderType, SinglePass, VirtualGpu, WritableBuffer, glam::Mat4, rust::Vec4};
use mutual::{CastableSharedData, CowData, RefCastGuard};
use wgpu::{BufferUsages, ShaderStages};

use crate::{Camera, Material, Mesh, Transform, shaders};

pub type BasicMesh = Mesh<shaders::basic_shader::VertexInput>;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct BasicMaterial {
    buffers: CowData<BasicMaterialBuffers>,
    color: Vec4
}

#[derive(Getters)]
pub struct BasicMaterialBuffers {
    instance_buffer: MutableBuffer<[Mat4]>,
    material_bind: BindableObject<shaders::common::Material>
}

impl BasicMaterial {
    pub fn new(color: Vec4) -> Self {
        Self { buffers: CowData::null(), color }
    }
}

impl Material for BasicMaterial {
    fn create_pipeline<'a>(&'a self, vgpu: &magician_vgpu::VirtualGpu) -> magician_vgpu::PipelineBuilder<'a> {
        Pipeline::builder("Normal Shader")
            .source(
                ShaderType::Vertex, 
                ShaderSource {
                    source: shaders::basic_shader::SHADER_primary_vs_main.into(),
                    main_function: "primary_vs_main".into()
                }
            )
            .source(
                ShaderType::Fragment, 
                ShaderSource {
                    source: shaders::basic_shader::SHADER_primary_fs_main.into(),
                    main_function: "primary_fs_main".into()
                }
            )
            .depth_format(DEPTH_FORMAT)
            .vertex(vertex_buffer_layout())
            .vertex(instance_buffer_layout())
            .layout_raw::<shaders::common::Material>(shaders::common::Material::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
            .layout_raw::<shaders::common::CameraInput>(shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
            // .build(&vgpu)
    }

    fn render_entity<'a>(
        &'a self,
        vgpu: &VirtualGpu, 
        pass: &mut SinglePass<'a>, 
        camera: &Camera, 
        entity: &'a anarchy::Entity
    ) {
        // get camera bindable or fail
        let Some(bindable) = camera.bindable()
            else { return };

        // extract transform and mesh components
        let (mut comps, _ctx) = extract_comps_distributed(
            entity, 
            &[Transform::bit_mask(), BasicMesh::bit_mask()], 
            None
        );
        let transform: RefCastGuard<_, Transform> = comps.next().flatten()
            .expect("BasicMaterial requires Transform companion component").lock_cast_ref();
        let mesh: RefCastGuard<_, BasicMesh> = comps.next().flatten()
            .expect("BasicMaterial requires Material companion component!").lock_cast_ref();

        // create instance matrix to draw
        let instance = Mat4::from_scale_rotation_translation(
            transform.scale, 
            transform.rotation, 
            transform.translation
        );

        // create or update buffers
        if !self.buffers.is_null() {
            self.buffers
                .get_ref()
                .instance_buffer()
                .write(vgpu, &[instance])
                .expect("Failed to update instance buffer");
        } else {
            let instance_buffer = MutableBuffer::<[Mat4]>::new(
                vgpu, &[instance], 
                BufferUsages::VERTEX | BufferUsages::COPY_DST
            );
            
            let material_buffer = MutableBuffer::new(vgpu, &self.color, BufferUsages::UNIFORM);
            let material_bind = BindableObject::<shaders::common::Material>::from_inputs(vgpu, &material_buffer);

            self.buffers.set(BasicMaterialBuffers { instance_buffer, material_bind });
        }
    
        // draw buffers
        let buffers = self.buffers.get_ref();
        pass.bind_instances(&buffers.instance_buffer);
        pass.bind(bindable);
        pass.bind(&buffers.material_bind);
        pass.draw(
            &mesh.vertex_buffer,
            &mesh.index_buffer,
            DrawSettings::default()
        )
    }
}

fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    use std::mem;
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<shaders::basic_shader::VertexInput>() as wgpu::BufferAddress,
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
        array_stride: mem::size_of::<shaders::basic_shader::InstanceInput>() as wgpu::BufferAddress,
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
