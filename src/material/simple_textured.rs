use anarchy::macros::Getters;
use image::GenericImageView;
use magician_vgpu::{BindGroupProvider, BindableObject, Pipeline, ShaderSource, ShaderType, SinglePass, StaticTexture, TextureDescriptor, VirtualGpu};
use mutual::CowData;
use wgpu::ShaderStages;

use crate::{Camera, Material, shaders};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A basic material that defines only a color to draw with the
/// material with.
#[derive(Getters)]
pub struct SimpleTexturedMaterial {
    buffers: CowData<BindableObject<shaders::simple_textured::SimpleTexturedMaterial>>,
    texture: StaticTexture
}

impl SimpleTexturedMaterial {
    pub fn new(texture: StaticTexture) -> Self {
        Self { buffers: CowData::null(), texture }
    }

    pub fn from_png(vgpu: &VirtualGpu, bytes: &[u8]) -> anyhow::Result<Self> {
        let img = image::load_from_memory(bytes)?;
        let dimensions = img.dimensions();
        let rgba = img.to_rgba8();
        let texture = StaticTexture::from_raw(
            vgpu, 
            TextureDescriptor {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                ..Default::default()
            }, 
            &rgba, 
            dimensions.0, 
            dimensions.1
        );
        Ok(Self { texture, buffers: CowData::null() })
    }
}

impl Material for SimpleTexturedMaterial {
    fn create_pipeline<'a>(&'a self, vgpu: &magician_vgpu::VirtualGpu) -> magician_vgpu::PipelineBuilder<'a> {
        Pipeline::builder("Normal Shader")
            .source(
                ShaderType::Fragment, 
                ShaderSource {
                    source: shaders::simple_textured::SHADER_simple_textured_main.into(),
                    main_function: "simple_textured_main".into()
                }
            )
            .depth_format(DEPTH_FORMAT)
            .layout_raw::<shaders::simple_textured::SimpleTexturedMaterial>(0, shaders::simple_textured::SimpleTexturedMaterial::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
            .layout_raw::<shaders::common::CameraInput>(1, shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
    }

    fn prep_render_entity(
        &self,
        vgpu: &VirtualGpu, 
        pass: &mut SinglePass, 
        camera: &Camera, 
        _entity: &anarchy::Entity
    ) {
        // get camera bindable or fail
        let Some(bindable) = camera.bindable()
            else { return };

        if self.buffers.is_null() {
            self.buffers.set(
                BindableObject
                    ::<shaders::simple_textured::SimpleTexturedMaterial>
                    ::from_inputs(vgpu, &(
                        self.texture.view.clone(), 
                        self.texture.sampler.clone()
                    ))
            );
        }
    
        // draw buffers
        pass.bind(bindable);
        pass.bind(&self.buffers.get_ref());
    }
}
