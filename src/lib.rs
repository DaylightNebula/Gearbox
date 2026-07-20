//! `gearbox` is an ECS-based renderer built on [`anarchy`] (ECS) and [`cell`] (app/windowing),
//! using [`magician_vgpu`] for its GPU abstraction over `wgpu`.
//!
//! Register [`GearboxRenderPlugin`] with a `cell` [`App`] to get a main render pass that draws
//! every entity carrying a [`Transform`], [`MaterialRef`], and [`MeshRef`], as seen through the
//! first entity found with a [`Camera`] component. [`shapes`] provides a [`ShapeBuilder`] for
//! generating primitive meshes, and [`vault`] provides a generic, not-yet-integrated
//! asset-loading system for reference-counted GPU resources such as textures.
//!
//! This project is early and work in progress; little of the API is set in stone.

#![allow(ambiguous_glob_reexports)]

use anarchy::{DeltaTime, FlexLocalId, Query, Res, ResMut, macros::{Resource, system}};
use cell::{App, Frame, Graphics, Plugin, RENDER_SCHEDULE_ID, WindowDimensions};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{LoadOp, PassAttachment, PassTarget, SinglePass, StoreOp, glam::Vec4};
use mutual::SharedData;

pub mod camera;
pub mod material;
pub mod mesh;
pub mod schedule;
pub mod shapes;
pub mod transform;
pub mod vault;

pub use camera::*;
pub use material::*;
pub use mesh::*;
pub use schedule::*;
pub use shapes::*;
pub use transform::*;
pub use vault::*;

pub use shaders as shaders;
pub use magician_vgpu::glam as glam;

/// The primary plugin used by Gearbox renderer.
pub struct GearboxRenderPlugin;
impl Plugin for GearboxRenderPlugin {
    fn build(self, app: App) -> App {
        app.add_plugin(MainRenderPassPlugin)
            .add_plugin(BindlessTexturesPlugin)
            .add_resource(MaterialPipelineStorage::default())
            .on_render_update(update_cameras)
            .on_render_update(begin_main_pass)
            .on_render_update(execute_render_schedule)
            .on_render_update(complete_main_pass)
    }
}

/// A passthrough resource that contains the `SinglePass` used
/// for the rendering of Gearboxs main render pass.  This can
/// be used by schedules added to `MainRenderPassSchedule` to
/// render to the main render pass without needing a mesh + material
/// entity.
#[derive(Resource, Deref, DerefMut)]
pub struct MainPassPassthrough(SinglePass);

/// Used to update all cameras at the earlist possible moment in the render schedule.
/// This provides up-to-date camera information to anyone using the cameras for rendering.
#[system(std::i32::MIN)]
pub fn update_cameras(
    graphics: Res<Graphics>,
    camera: Query<(&Transform, &mut Camera)>
) {
    for (transform, mut camera) in camera.as_iter() {
        camera.update(&*graphics, &*transform)?;
    }
}

/// Initializes the main render pass for rendering.  This schedule adds the `MainPassPassthrough` resource.
/// The three phases of rendering (initialize, render, complete) are split into seperate passes to avoid
/// lock conflicts with systems running in the `MainRenderPassSchedule`.
#[system(0)]
pub fn begin_main_pass(
    graphics: Res<Graphics>,
    frame: ResMut<Frame>,
    pipelines: ResMut<MaterialPipelineStorage>,
    camera: Query<&mut Camera>,
    schedule: Res<MainRenderPassSchedule>,
    delta_time: Res<DeltaTime>,
    window_dimensions: Res<WindowDimensions>
) {
    // get primary (first) camera
    let Some(mut camera) = camera.as_iter().next() else { return Ok(()) };

    // get depth buffer
    let depth_attachment = 
        camera.get_or_compute_framebuffer(
            &*graphics, 
            FrameBufferKey::Depth, 
            DEPTH_FORMAT, 
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ***window_dimensions
        ).map(|depth_texture| {
            PassAttachment { 
                target: PassTarget::Texture(depth_texture),
                load_op: LoadOp::Clear(1.0), 
                store_op: StoreOp::Store
            }
        });
    
    // draw pass
    let pass = frame.init_pass(
        &[
            PassAttachment {
                target: PassTarget::PassOutput,
                load_op: LoadOp::Clear(Vec4::new(0.1, 0.2, 0.3, 1.0)),
                store_op: StoreOp::Store
            }
        ], 
        depth_attachment
    );

    // add pass passthrough to finish rendering
    world.insert_resource(MainPassPassthrough(pass));

    // update delta time using primary render schedule delta time (effectively the same)
    delta_time.set(FlexLocalId::Schedule(*schedule.schedule_id()), *delta_time.get(FlexLocalId::Schedule(RENDER_SCHEDULE_ID)).lock_ref());
}

/// Execute the main render pass schedules through `MainRenderPassSchedule`.
#[system(1)]
fn execute_render_schedule(schedule: Res<MainRenderPassSchedule>) {
    if world.get_resource_ref::<MainPassPassthrough>().is_none() { return Ok(()) }
    schedule.execute(world);
}

/// Complete the main render pass by removing and dropping the pass contained in `MainPassPassthrough`.
#[system(2)]
fn complete_main_pass() {
    world.remove_resource::<MainPassPassthrough>();
}
