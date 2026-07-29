use anarchy::{World, anyhow::{self, bail}, macros::AsAny};
use magician_vgpu::{BindGroupProvider, BindableObject, MutableBuffer, Pipeline, PipelineBuilder, ShaderSource, ShaderType, SinglePass, VirtualGpu, rust::Vec4};
use mutual::CowData;
use wgpu::ShaderStages;

use crate::{AssetVault, AtlasTextureVault, BindableAssetVault, BindlessArrayTextureVault, Camera, Material, TextureHandle, shaders};

/// Depth format used by Gearbox's main render pass depth buffer and by materials
/// that render into it (see [`Camera::get_or_compute_framebuffer`](crate::Camera::get_or_compute_framebuffer)).
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A material that samples a single albedo texture, with no other lighting inputs.
///
/// Works against either [`TextureVault`] backend (true bindless array or the atlas
/// fallback) transparently -- which one is used depends only on which variant `texture`
/// (a [`TextureHandle`]) was loaded as.
#[derive(Clone, AsAny)]
pub struct SimpleTexturedMaterial {
    buffers: CowData<SimpleTexturedBuffers>,
    texture: TextureHandle
}

/// The per-material uniform + bind group, one of two shapes depending on which
/// [`TextureVault`] backend `texture` came from.
enum SimpleTexturedBuffers {
    Bindless {
        #[allow(unused)] buffer: MutableBuffer<u32>,
        bindable: BindableObject<shaders::simple_textured::SimpleTexturedMaterial>
    },
    Atlas {
        #[allow(unused)] buffer: MutableBuffer<Vec4>,
        bindable: BindableObject<shaders::simple_textured_atlas::SimpleTexturedAtlasMaterial>
    }
}

impl SimpleTexturedMaterial {
    /// Creates a new `SimpleTexturedMaterial` from an already-loaded `texture`.
    pub fn new(texture: TextureHandle) -> Self {
        Self { buffers: CowData::null(), texture }
    }

    // /// Decodes `bytes` as a PNG and creates a `SimpleTexturedMaterial` from it.
    // pub fn from_png(vgpu: &VirtualGpu, bytes: &[u8]) -> anyhow::Result<Self> {
    //     let img = image::load_from_memory(bytes)?;
    //     let dimensions = img.dimensions();
    //     let rgba = img.to_rgba8();
    //     let texture = StaticTexture::from_raw(
    //         vgpu, 
    //         TextureDescriptor {
    //             format: wgpu::TextureFormat::Rgba8UnormSrgb,
    //             usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    //             ..Default::default()
    //         }, 
    //         &rgba, 
    //         dimensions.0, 
    //         dimensions.1
    //     );
    //     Ok(Self { texture, buffers: CowData::null() })
    // }
}

impl Material for SimpleTexturedMaterial {
    fn create_pipeline<'a>(&self, vgpu: &magician_vgpu::VirtualGpu, _world: &World) -> anyhow::Result<PipelineBuilder<'a>> {
        Ok(match &self.texture {
            TextureHandle::Bindless(_) => Pipeline::builder("Normal Shader")
                .source(
                    ShaderType::Fragment,
                    ShaderSource {
                        source: shaders::simple_textured::SHADER_simple_textured_main.into(),
                        main_function: "simple_textured_main".into()
                    }
                )
                .depth_format(DEPTH_FORMAT)
                .layout_raw::<shaders::simple_textured::SimpleTexturedMaterial>(0, shaders::simple_textured::SimpleTexturedMaterial::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
                .layout_raw::<shaders::common::CameraInput>(2, shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
                .layout_raw::<shaders::common::BindlessTextures>(1, shaders::common::BindlessTextures::layout(vgpu, ShaderStages::VERTEX_FRAGMENT)),
            TextureHandle::Atlas(_) => Pipeline::builder("Normal Shader (Atlas)")
                .source(
                    ShaderType::Fragment,
                    ShaderSource {
                        source: shaders::simple_textured_atlas::SHADER_simple_textured_atlas_main.into(),
                        main_function: "simple_textured_atlas_main".into()
                    }
                )
                .depth_format(DEPTH_FORMAT)
                .layout_raw::<shaders::simple_textured_atlas::SimpleTexturedAtlasMaterial>(0, shaders::simple_textured_atlas::SimpleTexturedAtlasMaterial::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
                .layout_raw::<shaders::common::CameraInput>(2, shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
                .layout_raw::<shaders::common::AtlasTextures>(1, shaders::common::AtlasTextures::layout(vgpu, ShaderStages::VERTEX_FRAGMENT)),
        })
    }

    fn prep_render_entity(
        &self,
        vgpu: &VirtualGpu,
        pass: &mut SinglePass,
        world: &World,
        camera: &Camera,
        _entity: &anarchy::Entity
    ) -> anyhow::Result<()> {
        // get camera bindable or fail
        let Some(bindable) = camera.bindable()
            else { return Ok(()) };

        // initialize buffers, then bind, using whichever concrete vault matches `texture`'s
        // own backend -- no separate "which backend is active" lookup needed
        match &self.texture {
            TextureHandle::Bindless(handle) => {
                let Some(vault) = world.get_resource_ref::<BindlessArrayTextureVault>()
                    else { bail!("Missing BindlessArrayTextureVault") };
                let Some(texture) = vault.get(handle)
                    else { bail!("SimpleTexturedMaterial internal handle not yet loaded") };

                if self.buffers.is_null() {
                    let buffer = MutableBuffer::<u32>
                        ::new(vgpu, &(*texture.texture_idx() as u32), wgpu::BufferUsages::UNIFORM);
                    let bindable = BindableObject
                        ::<shaders::simple_textured::SimpleTexturedMaterial>
                        ::from_inputs(vgpu, &buffer);
                    self.buffers.set(SimpleTexturedBuffers::Bindless { buffer, bindable });
                }

                match &*self.buffers.get_ref() {
                    SimpleTexturedBuffers::Bindless { bindable, .. } => pass.bind(bindable),
                    SimpleTexturedBuffers::Atlas { .. } => unreachable!(),
                };
                vault.bind(vgpu, pass, 1)?;
            }
            TextureHandle::Atlas(handle) => {
                let Some(vault) = world.get_resource_ref::<AtlasTextureVault>()
                    else { bail!("Missing AtlasTextureVault") };
                let Some(texture) = vault.get(handle)
                    else { bail!("SimpleTexturedMaterial internal handle not yet loaded") };

                if self.buffers.is_null() {
                    let rect = Vec4::new(
                        texture.offset_px().x as f32, texture.offset_px().y as f32,
                        texture.size_px().x as f32, texture.size_px().y as f32
                    );
                    let buffer = MutableBuffer::<Vec4>
                        ::new(vgpu, &rect, wgpu::BufferUsages::UNIFORM);
                    let bindable = BindableObject
                        ::<shaders::simple_textured_atlas::SimpleTexturedAtlasMaterial>
                        ::from_inputs(vgpu, &buffer);
                    self.buffers.set(SimpleTexturedBuffers::Atlas { buffer, bindable });
                }

                match &*self.buffers.get_ref() {
                    SimpleTexturedBuffers::Atlas { bindable, .. } => pass.bind(bindable),
                    SimpleTexturedBuffers::Bindless { .. } => unreachable!(),
                };
                vault.bind(vgpu, pass, 1)?;
            }
        }
        pass.bind(bindable);

        Ok(())
    }
}
