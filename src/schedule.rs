use std::collections::LinkedList;

use ahash::AHashMap;
use anarchy::{EntityID, IntoSystem, Query, Res, ResMut, Resource, SingleThreadedExecutor, SystemGraph, SystemInstruction, World};
use anyhow::bail;
use cell::{App, Graphics, Plugin};
use mutual::CowData;

use crate::{Camera, MainPassPassthrough, MaterialPipelineStorage, MaterialRef, MeshRef};

/// This plugin defines the schedule that runs in the main render pass.
pub struct MainRenderPassPlugin;
impl Plugin for MainRenderPassPlugin {
    fn build(self, app: App) -> App {
        let mut schedule = MainRenderPassSchedule::new();
        schedule.on_update(render_mesh_material)
            .expect("an unordered system can't conflict with the graph");
        app.add_resource(schedule)
    }
}

/// The resource that contains the system graphs that run during the
/// main render pass, once per frame while [`MainPassPassthrough`] exists.
///
/// The graphs run while this resource is locked, so their systems can't
/// take `Res<MainRenderPassSchedule>` or `ResMut<MainRenderPassSchedule>`.
#[derive(Resource, Default)]
pub struct MainRenderPassSchedule {
    /// Runs once before the first update, taken when it runs.
    startup: Option<SystemGraph>,
    /// Runs every frame.
    update: SystemGraph,
    exec: SingleThreadedExecutor
}

impl std::fmt::Debug for MainRenderPassSchedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainRenderPassSchedule")
            .field("systems", &self.update.nodes().count())
            .finish_non_exhaustive()
    }
}

impl MainRenderPassSchedule {
    /// Create a new `MainRenderPassSchedule`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a system to run on startup of the main pass schedule.
    ///
    /// # Errors
    ///
    /// Never fails, kept for symmetry with [`Self::on_startup_with`].
    pub fn on_startup<S, M>(&mut self, system: S) -> anyhow::Result<()>
        where S: IntoSystem<(), (), M> + 'static
    {
        self.on_startup_with(system, SystemInstruction::default())
    }

    /// Add a system to run on startup of the main pass schedule, ordered by `instruction`.
    ///
    /// # Errors
    ///
    /// Fails if `instruction` creates a cycle or contradicts a pin.
    pub fn on_startup_with<S, M>(&mut self, system: S, instruction: SystemInstruction) -> anyhow::Result<()>
        where S: IntoSystem<(), (), M> + 'static
    {
        self.startup.get_or_insert_default().append_system(system, instruction, std::iter::empty())
    }

    /// Add a system to run on update of the main pass schedule.
    ///
    /// # Errors
    ///
    /// Never fails, kept for symmetry with [`Self::on_update_with`].
    pub fn on_update<S, M>(&mut self, system: S) -> anyhow::Result<()>
        where S: IntoSystem<(), (), M> + 'static
    {
        self.on_update_with(system, SystemInstruction::default())
    }

    /// Add a system to run on update of the main pass schedule, ordered by `instruction`.
    ///
    /// # Errors
    ///
    /// Fails if `instruction` creates a cycle or contradicts a pin.
    pub fn on_update_with<S, M>(&mut self, system: S, instruction: SystemInstruction) -> anyhow::Result<()>
        where S: IntoSystem<(), (), M> + 'static
    {
        self.update.append_system(system, instruction, std::iter::empty())
    }

    /// Internal function to run the startup graph if it hasn't run yet, then the update graph.
    pub(crate) fn execute(&mut self, world: &World) -> anyhow::Result<()> {
        if let Some(startup) = self.startup.take() {
            self.exec.run(world, &startup)?;
        }
        self.exec.run(world, &self.update)
    }
}

/// Draws every entity with a [`MaterialRef`] and [`MeshRef`] through the first camera.
fn render_mesh_material(
    world: World,
    graphics: Res<Graphics>,
    mut pipelines: ResMut<MaterialPipelineStorage>,
    mut pass: ResMut<MainPassPassthrough>,
    mut cameras: Query<&'static Camera>,
    renderables: Query<(&'static MaterialRef, &'static MeshRef)>
) {
    // get primary (first) camera
    let Ok(Some((_, camera))) = cameras.next() else { return };

    if let Err(err) = draw_renderables(&world, &graphics, &mut pipelines, &mut pass, &camera, renderables) {
        log::error!("Failed to render meshes: {err:#}");
    }
}

fn draw_renderables(
    world: &World,
    graphics: &Graphics,
    pipelines: &mut MaterialPipelineStorage,
    pass: &mut MainPassPassthrough,
    camera: &Camera,
    mut renderables: Query<(&'static MaterialRef, &'static MeshRef)>
) -> anyhow::Result<()> {
    // group all renderable materials by there material's ID
    let mut groups = AHashMap::new();
    while let Some((entity, (material, mesh))) = renderables.next()? {
        // ensure pipeline exists for material, skipping entities whose material or mesh is
        // still loading
        let mesh_mat_key = (mesh.id(), material.id());
        if !pipelines.contains_key(&mesh_mat_key) {
            let (Ok(material_pipeline), Ok(mesh_pipeline)) = (
                material.create_pipeline(graphics, world),
                mesh.create_pipeline(graphics, world)
            ) else { continue };
            let pipeline = material_pipeline.merge(mesh_pipeline).build(graphics);
            pipelines.insert(mesh_mat_key, CowData::new(pipeline));
        }

        // create material group if needed, then save material and entity
        let mat_list = groups.entry(mesh_mat_key)
            .or_insert_with(LinkedList::new);
        mat_list.push_back((material, mesh, entity));
    }

    // set material pipeline
    for (mesh_mat_key, material_list) in groups.iter() {
        let Some(pipeline) = pipelines.get(mesh_mat_key) else { continue };

        pass.use_pipeline(pipeline.get_ref());
        for (material, mesh, entity) in material_list {
            let entity: EntityID = *entity;
            let mat_result = material.prep_render_entity(
                graphics,
                pass,
                world,
                camera,
                entity
            );
            if mat_result.is_err() { continue }

            // validate mesh mat key, this has to be done after prep_render_entity to protect against material swapping
            if material.id() != mesh_mat_key.1 { bail!("Material mismatch") }
            if mesh.id() != mesh_mat_key.0 { bail!("Mesh mismatch") }

            mesh.draw(graphics, pass, world, entity)?;
        }
    }

    Ok(())
}
