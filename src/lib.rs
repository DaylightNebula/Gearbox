#![allow(ambiguous_glob_reexports)]

use std::collections::LinkedList;

use ahash::AHashMap;
use anarchy::{ComponentMeta, MaskBuilder, Query, Res, ResMut, Schedule, extract_comps, macros::{Getters, Resource, system}};
use cell::{App, Frame, Graphics, Plugin};
use magician_vgpu::{LoadOp, PassAttachment, PassTarget, StoreOp, glam::Vec4};

pub mod camera;
pub mod material;
pub mod mesh;
pub mod transform;

pub use camera::*;
pub use material::*;
pub use mesh::*;
pub use transform::*;

use mutual::{CastableSharedData, CowData, RefCastGuard};
pub use shaders as shaders;

pub type MainPassScheduleIn = ();
pub type MainPassScheduleOut = ();

pub struct RenderPlugin;
impl Plugin for RenderPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(MaterialPipelineStorage::default())
            .add_resource(MainRenderPassSchedules::default())
            .on_render_update(update_cameras)
            .on_render_update(render)
    }
}

#[derive(Resource, Getters)]
pub struct MainRenderPassSchedules {
    schedule: CowData<Schedule<MainPassScheduleIn, MainPassScheduleOut>>
}

impl Default for MainRenderPassSchedules {
    fn default() -> Self {
        Self {
            schedule: CowData::new(Schedule::new_empty())
        }
    }
}

#[system(std::i32::MIN)]
pub fn update_cameras(
    graphics: Res<Graphics>,
    camera: Query<(&Transform, &mut Camera)>
) {
    for (transform, mut camera) in camera.as_iter() {
        camera.update(&*graphics, &*transform)?;
    }
}


#[system]
pub fn render(
    graphics: Res<Graphics>,
    frame: ResMut<Frame>,
    pipelines: ResMut<MaterialPipelineStorage>,
    camera: Query<&mut Camera>
) {
    // get primary (first) camera
    let Some(mut camera) = camera.as_iter().next() else { return Ok(()) };

    // get depth buffer
    let depth_attachment = 
        camera.get_or_compute_framebuffer(
            &*graphics, 
            FrameBufferKey::Depth, 
            DEPTH_FORMAT, 
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
        ).map(|depth_texture| {
            PassAttachment { 
                target: PassTarget::Texture(depth_texture),
                load_op: LoadOp::Clear(1.0), 
                store_op: StoreOp::Store
            }
        });
    
    // draw pass
    let mut pass = frame.init_pass(
        &[
            PassAttachment {
                target: PassTarget::PassOutput,
                load_op: LoadOp::Clear(Vec4::new(0.1, 0.2, 0.3, 1.0)),
                store_op: StoreOp::Store
            }
        ], 
        depth_attachment
    );

    // group all renderable materials by there material's ID
    let mut groups = AHashMap::new();
    let mut builder = MaskBuilder::new();
    builder.insert::<MaterialRef>();
    builder.insert::<MeshRef>();
    let mask = builder.build();
    let search_ids = [MaterialRef::bit_mask(), MeshRef::bit_mask()];
    for chunk in world.query_raw(&mask) {
        let mut extract_ctx = None;
        for entity in chunk.iter() {
            // get material from entity
            let (material, mesh, new_ctx) = {
                let (mut iter, new_ctx) = extract_comps(&entity, &search_ids, &extract_ctx);
                let material: Option<RefCastGuard<_, MaterialRef>> = iter.next().flatten()
                    .map(|a| a.lock_cast_ref());
                let mesh: Option<RefCastGuard<_, MeshRef>> = iter.next().flatten()
                    .map(|a| a.lock_cast_ref());
                (material, mesh, new_ctx)
            };
            let Some(material) = material else { continue };
            let Some(mesh) = mesh else { continue };
            if let Some(new_ctx) = new_ctx { extract_ctx = Some(new_ctx); }

            // ensure pipeline exists for material
            let mesh_mat_key = (mesh.id(), material.id());
            if !pipelines.contains_key(&mesh_mat_key) {
                let pipeline = material.create_pipeline(&*graphics)
                    .merge(mesh.create_pipeline(&*graphics))
                    .build(&*graphics);
                pipelines.insert(mesh_mat_key, CowData::new(pipeline));
            }

            // create material group if needed, then save material and entity
            let mat_list = groups.entry(mesh_mat_key)
                .or_insert_with_key(|_| LinkedList::new());
            mat_list.push_back((material, mesh, entity));
        }
    }

    // set material pipeline
    for (mesh_mat_key, material_list) in groups.iter() {
        let Some(pipeline) = pipelines.get(mesh_mat_key) else { continue };

        pass.use_pipeline(pipeline.get_ref());
        for (material, mesh, entity) in material_list {
            material.prep_render_entity(
                &*graphics, 
                &mut pass, 
                &camera, 
                &*entity
            );

            mesh.draw(&*graphics, &mut pass, entity);
        }
    }
}
