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
#![recursion_limit = "256"]

use anarchy::{Query, Res, ResMut, Resource, SystemPin, World, after, pin};
use cell::{App, Frame, Graphics, Plugin, WindowDimensions};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{LoadOp, PassAttachment, PassTarget, SinglePass, StoreOp, glam::Vec4};

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
            .add_plugin(AtlasTexturesPlugin)
            .add_plugin(MeshAssetPlugin)
            .add_resource(MaterialPipelineStorage::default())
            .add_resource(MaterialVault::default())
            .add_resource(ClearColor(Vec4::new(0.1, 0.2, 0.3, 1.0)))
            .on_render_update_with(update_cameras, pin(SystemPin::Start))
            .on_render_update(begin_main_pass)
            .on_render_update_with(execute_render_schedule, after(begin_main_pass))
            .on_render_update_with(complete_main_pass, after(execute_render_schedule))
    }
}

/// A passthrough resource that contains the `SinglePass` used
/// for the rendering of Gearboxs main render pass.  This can
/// be used by schedules added to `MainRenderPassSchedule` to
/// render to the main render pass without needing a mesh + material
/// entity.
#[derive(Resource, Deref, DerefMut)]
pub struct MainPassPassthrough(SinglePass);

impl std::fmt::Debug for MainPassPassthrough {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainPassPassthrough").finish_non_exhaustive()
    }
}

/// The color the main render pass's color attachment is cleared to before drawing.
/// Defaults to a dark blue-grey; mutate via `world.resource_mut::<ClearColor>()`.
#[derive(Debug, Resource, Deref, DerefMut, Clone, Copy)]
pub struct ClearColor(pub Vec4);

/// Used to update all cameras at the earlist possible moment in the render schedule.
/// This provides up-to-date camera information to anyone using the cameras for rendering.
pub fn update_cameras(
    graphics: Res<Graphics>,
    mut cameras: Query<(&'static Transform, &'static mut Camera)>
) {
    while let Ok(Some((_, (transform, mut camera)))) = cameras.next() {
        if let Err(err) = camera.update(&graphics, &transform) {
            log::error!("Failed to update camera: {err:#}");
        }
    }
}

/// Initializes the main render pass for rendering.  This schedule adds the `MainPassPassthrough` resource.
/// The three phases of rendering (initialize, render, complete) are split into seperate passes to avoid
/// lock conflicts with systems running in the `MainRenderPassSchedule`.
pub fn begin_main_pass(
    world: World,
    graphics: Res<Graphics>,
    mut frame: ResMut<Frame>,
    mut cameras: Query<&'static mut Camera>,
    window_dimensions: Res<WindowDimensions>,
    clear_color: Res<ClearColor>
) {
    // get primary (first) camera
    let Ok(Some((_, mut camera))) = cameras.next() else { return };

    // get depth buffer
    let depth_attachment = 
        camera.get_or_compute_framebuffer(
            &graphics, 
            FrameBufferKey::Depth, 
            DEPTH_FORMAT, 
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            **window_dimensions
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
                load_op: LoadOp::Clear(**clear_color),
                store_op: StoreOp::Store
            }
        ], 
        depth_attachment
    );

    // add pass passthrough to finish rendering
    world.insert_resource(MainPassPassthrough(pass));
}

/// Execute the main render pass schedules through `MainRenderPassSchedule`.
fn execute_render_schedule(world: World, mut schedule: ResMut<MainRenderPassSchedule>) {
    if !world.has_resource::<MainPassPassthrough>() { return }
    if let Err(err) = schedule.execute(&world) {
        log::error!("Main render pass system failed: {err:#}");
    }
}

/// Complete the main render pass by removing and dropping the pass contained in `MainPassPassthrough`.
fn complete_main_pass(world: World) {
    world.remove_resource::<MainPassPassthrough>();
}
