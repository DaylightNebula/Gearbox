use anarchy::{World, anyhow, macros::{AsAny, Getters}};
use magician_vgpu::{BindGroupProvider, BindableObject, MutableBuffer, Pipeline, PipelineBuilder, ShaderSource, ShaderType, SinglePass, VirtualGpu, glam::Vec4};
use mutual::CowData;
use wgpu::{BufferUsages, ShaderStages};

use crate::{Camera, Material, shaders};

/// A basic material that defines only a color to draw with the
/// material with.
#[derive(Clone, Getters, AsAny)]
pub struct BasicMaterial {
    buffers: CowData<(BindableObject<shaders::basic_material::BasicMaterial>, BindableObject<shaders::common::EmptyBindable>)>,
    color: Vec4
}

impl BasicMaterial {
    /// Creates a new `BasicMaterial` with the given flat RGBA `color`.
    pub fn new(color: Vec4) -> Self {
        Self { buffers: CowData::null(), color }
    }
}

impl Material for BasicMaterial {
    fn create_pipeline<'a>(&'a self, vgpu: &magician_vgpu::VirtualGpu) -> anyhow::Result<PipelineBuilder<'a>> {
        Ok(
            Pipeline::builder("Basic Material Shader")
                .source(
                    ShaderType::Fragment, 
                    ShaderSource {
                        source: shaders::basic_material::SHADER_primary_fs_main.into(),
                        main_function: "primary_fs_main".into()
                    }
                )
                .depth_format(wgpu::TextureFormat::Depth32Float)
                .layout_raw::<shaders::basic_material::BasicMaterial>(0, shaders::basic_material::BasicMaterial::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
                .layout_raw::<shaders::common::EmptyBindable>(1, shaders::common::EmptyBindable::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
                .layout_raw::<shaders::common::CameraInput>(2, shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
        )
    }

    fn prep_render_entity(
        &self,
        vgpu: &VirtualGpu, 
        pass: &mut SinglePass, 
        _world: &World,
        camera: &Camera, 
        _entity: &anarchy::Entity
    ) -> anyhow::Result<()> {
        // get camera bindable or fail
        let Some(bindable) = camera.bindable()
            else { return Ok(()) };

        if self.buffers.is_null() {
            let material_buffer = MutableBuffer::new(vgpu, &self.color.into(), BufferUsages::UNIFORM);
            let material_bind = BindableObject::<shaders::basic_material::BasicMaterial>::from_inputs(vgpu, &material_buffer);

            let empty = BindableObject::<shaders::common::EmptyBindable>::from_inputs(vgpu, &());

            self.buffers.set((material_bind, empty));
        }
    
        // draw buffers
        pass.bind(bindable);
        pass.bind(&self.buffers.get_ref().0);
        pass.bind(&self.buffers.get_ref().1);

        Ok(())
    }
}
