use anarchy::macros::Getters;
use magician_vgpu::{BindGroupProvider, BindableObject, Pipeline, ShaderSource, ShaderType, SinglePass, StaticTexture, VirtualGpu};
use mutual::CowData;
use wgpu::ShaderStages;

use crate::{Camera, Material, shaders};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A basic material that defines only a color to draw with the
/// material with.
#[derive(Getters)]
pub struct SimpleTexturedMaterial {
    buffers: CowData<BindableObject<shaders::basic_material::BasicMaterial>>,
    texture: StaticTexture
}

impl SimpleTexturedMaterial {
    pub fn new(texture: StaticTexture) -> Self {
        Self { buffers: CowData::null(), texture }
    }
}

impl Material for SimpleTexturedMaterial {
    fn create_pipeline<'a>(&'a self, vgpu: &magician_vgpu::VirtualGpu) -> magician_vgpu::PipelineBuilder<'a> {
        Pipeline::builder("Normal Shader")
            .source(
                ShaderType::Fragment, 
                ShaderSource {
                    source: shaders::basic_material::SHADER_primary_fs_main.into(),
                    main_function: "primary_fs_main".into()
                }
            )
            .depth_format(DEPTH_FORMAT)
            .layout_raw::<shaders::basic_material::BasicMaterial>(shaders::basic_material::BasicMaterial::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
            .layout_raw::<shaders::common::CameraInput>(shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
    }

    fn prep_render_entity<'a>(
        &'a self,
        _vgpu: &VirtualGpu, 
        _pass: &mut SinglePass<'a>, 
        _camera: &Camera, 
        _entity: &'a anarchy::Entity
    ) {
        todo!()
        // // get camera bindable or fail
        // let Some(bindable) = camera.bindable()
        //     else { return };

        // if self.buffers.is_null() {
        //     let material_buffer = MutableBuffer::new(vgpu, &self.color.into(), BufferUsages::UNIFORM);
        //     let material_bind = BindableObject::<shaders::basic_material::BasicMaterial>::from_inputs(vgpu, &material_buffer);

        //     self.buffers.set(material_bind);
        // }
    
        // // draw buffers
        // pass.bind(bindable);
        // pass.bind(&self.buffers.get_ref());
    }
}
