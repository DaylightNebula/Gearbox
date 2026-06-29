use magician_vgpu::{BindGroupProvider, BindableObject, MutableBuffer, Pipeline, ShaderSource, ShaderType, SinglePass, VirtualGpu, rust::Vec4};
use mutual::CowData;
use wgpu::{BufferUsages, ShaderStages};

use crate::{Camera, Material, shaders};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A basic material that defines only a color to draw with the
/// material with.
pub struct BasicMaterial {
    buffers: CowData<BindableObject<shaders::common::Material>>,
    color: Vec4
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
                ShaderType::Fragment, 
                ShaderSource {
                    source: shaders::basic_shader::SHADER_primary_fs_main.into(),
                    main_function: "primary_fs_main".into()
                }
            )
            .depth_format(DEPTH_FORMAT)
            .layout_raw::<shaders::common::Material>(shaders::common::Material::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
            .layout_raw::<shaders::common::CameraInput>(shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
    }

    fn prep_render_entity<'a>(
        &'a self,
        vgpu: &VirtualGpu, 
        pass: &mut SinglePass<'a>, 
        camera: &Camera, 
        _entity: &'a anarchy::Entity
    ) {
        // get camera bindable or fail
        let Some(bindable) = camera.bindable()
            else { return };

        if self.buffers.is_null() {
            let material_buffer = MutableBuffer::new(vgpu, &self.color, BufferUsages::UNIFORM);
            let material_bind = BindableObject::<shaders::common::Material>::from_inputs(vgpu, &material_buffer);

            self.buffers.set(material_bind);
        }
    
        // draw buffers
        pass.bind(bindable);
        pass.bind(&self.buffers.get_ref());
    }
}
