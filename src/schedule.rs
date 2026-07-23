use std::collections::LinkedList;

use ahash::AHashMap;
use anarchy::{ComponentMeta, MaskBuilder, Query, Res, ResMut, Schedule, ScheduleID, ScheduleTile, System, World, anyhow, execute_schedule_sync, extract_comps, macros::{Getters, Resource, system}};
use cell::{App, Graphics, Plugin};
use mutual::{CastableSharedData, CowData, RefCastGuard};

use crate::{Camera, MainPassPassthrough, MaterialPipelineStorage, MaterialRef, MeshRef};

/// This plugin defines the schedule that runs in the main render pass.
pub struct MainRenderPassPlugin;
impl Plugin for MainRenderPassPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(MainRenderPassSchedule::new())
            .on_render_startup(setup)
    }
}

/// The resource that contains the `Schedule` that will run during the 
/// main render pass.
#[derive(Resource, Getters)]
pub struct MainRenderPassSchedule {
    schedule: CowData<Schedule<(), ()>>,
    schedule_id: ScheduleID
}

impl MainRenderPassSchedule {
    /// Create a new `MainRenderPassSchedule`.
    pub fn new() -> Self {
        Self {
            schedule: CowData::new(Schedule::new_empty()),
            schedule_id: ScheduleID { id: "MAIN_PASS", tick_rate: 0, max_threads: 0 }
        }
    }

    /// Add a system to run on startup of the main pass schedule.
    pub fn on_startup<S>(&self, system: S) 
        where S: System<(), anyhow::Result<()>> + 'static
    {
        let tile = ScheduleTile::new(vec![Box::new(system)]);
        self.schedule.get_ref().add_startup(tile);
    }

    /// Add a system to run on update of the main pass schedule.
    pub fn on_update<S>(&self, system: S) 
        where S: System<(), anyhow::Result<()>> + 'static
    {
        let tile = ScheduleTile::new(vec![Box::new(system)]);
        self.schedule.get_ref().add_new(tile);
    }

    /// Internal function to execute all items in the given schedule.
    pub(crate) fn execute(&self, world: &World) {
        // swap schedules then execute the previous one
        let prev_schedule = CowData::new(Schedule::new_empty());
        self.schedule.swap(&prev_schedule);
        execute_schedule_sync(
            &prev_schedule.get_ref(), 
            &*self.schedule.get_ref(), 
            self.schedule_id, 
            world, 
            &()
        );
    }
}

/// Registers [`render_mesh_material`] to run on every update of the main render pass schedule.
#[system(1)]
pub fn setup(
    schedule: Res<MainRenderPassSchedule>
) {
    schedule.on_update(render_mesh_material);
}

#[system]
fn render_mesh_material(
    graphics: Res<Graphics>,
    pipelines: ResMut<MaterialPipelineStorage>,
    pass: ResMut<MainPassPassthrough>,
    camera: Query<&Camera>
) {
    // get primary (first) camera
    let Some(camera) = camera.as_iter().next() else { return Ok(()) };

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
                let pipeline = material.create_pipeline(&*graphics)?
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
            let mat_result = material.prep_render_entity(
                &*graphics, 
                &mut pass, 
                world,
                &camera, 
                &*entity
            );
            if mat_result.is_err() { continue }

            mesh.draw(&*graphics, &mut pass, world, entity)?;
        }
    }
}
