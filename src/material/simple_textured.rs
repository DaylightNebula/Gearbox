use anarchy::{anyhow, macros::Getters};
use image::GenericImageView;
use magician_vgpu::{BindGroupProvider, BindableObject, Pipeline, ShaderSource, ShaderType, SinglePass, StaticTexture, TextureDescriptor, VirtualGpu};
use mutual::CowData;
use wgpu::ShaderStages;

use crate::{Camera, Material, shaders};

/// Depth format used by Gearbox's main render pass depth buffer and by materials
/// that render into it (see [`Camera::get_or_compute_framebuffer`](crate::Camera::get_or_compute_framebuffer)).
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A material that samples a single albedo texture, with no other lighting inputs.
#[derive(Getters)]
pub struct SimpleTexturedMaterial {
    buffers: CowData<BindableObject<shaders::simple_textured::SimpleTexturedMaterial>>,
    texture: StaticTexture
}

impl SimpleTexturedMaterial {
    /// Creates a new `SimpleTexturedMaterial` from an already-loaded `texture`.
    pub fn new(texture: StaticTexture) -> Self {
        Self { buffers: CowData::null(), texture }
    }

    /// Decodes `bytes` as a PNG and creates a `SimpleTexturedMaterial` from it.
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
