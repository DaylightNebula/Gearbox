use std::any::{Any, TypeId};

use ahash::AHashMap;
use anarchy::{Entity, World, anyhow::{self, bail}, macros::{Component, Resource}};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{Pipeline, PipelineBuilder, SinglePass, VirtualGpu};
use mutual::CowData;

pub mod basic;
pub mod simple_textured;

pub use basic::*;
pub use simple_textured::*;

use crate::Camera;

/// Central storage for all pipelines in use by `Material`s.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct MaterialPipelineStorage {
    pipelines: AHashMap<(TypeId, TypeId), CowData<Pipeline>>
}

/// Standard trait for any `Material` type. `id` identifies the concrete material type
/// (shared by all instances of that type, used to key material/mesh pipelines in
/// [`MaterialPipelineStorage`]), `create_pipeline` builds the render pipeline for
/// this material, and `prep_render_entity` binds per-entity buffers before drawing.
pub trait Material: Any {
    fn id(&self) -> TypeId { TypeId::of::<Self>() }

    fn create_pipeline<'a>(
        &self, 
        vgpu: &VirtualGpu,
        world: &World
    ) -> anyhow::Result<PipelineBuilder<'a>>;

    fn prep_render_entity(
        &self, 
        vgpu: &VirtualGpu, 
        pass: &mut SinglePass, 
        world: &World,
        camera: &Camera, 
        entity: &Entity
    ) -> anyhow::Result<()>;
}

/// A [`Component`](anarchy::Component) wrapping a type-erased [`Material`], attached to
/// an entity alongside a [`MeshRef`](crate::MeshRef) to make it renderable.
#[derive(Deref, DerefMut, Component)]
pub struct MaterialRef(pub Box<dyn Material>);

impl MaterialRef {
    /// Wraps `material` in a `MaterialRef` component.
    pub fn new<M: Material>(material: M) -> Self {
        MaterialRef(Box::new(material))
    }
}

/// This type allows for materials that can be swapped at runtime while
/// still be contained by a MaterialRef.  This is meant for lazy loading
/// of materials outside of traditional asset loading or when loaders
/// need to lazy load materials outside of traditional handles.
/// 
/// This maps to a `CowData`, so this can be cloned while referencing the
/// same underlying data.  While the `CowData` is unfilled, the type ID
/// will be a generic type ID, not the type ID of the data it will eventually
/// reference.  In addition, while the `CowData` is null, `create_pipeline`
/// and `prep_render_entity` will bail.
pub type HotSwapMaterial = CowData<Box<dyn Material>>;

impl Material for HotSwapMaterial {
    fn id(&self) -> TypeId { 
        if self.is_null() { return TypeId::of::<Self>() }
        self.get_ref().id() 
    }

    fn create_pipeline<'a>(
        &self, 
        vgpu: &VirtualGpu,
        world: &World
    ) -> anyhow::Result<PipelineBuilder<'a>> {
        if self.is_null() { bail!("Unfilled hotswap material") }
        self.get_ref().create_pipeline(vgpu, world)
    }

    fn prep_render_entity(
        &self, 
        vgpu: &VirtualGpu, 
        pass: &mut SinglePass, 
        world: &World,
        camera: &Camera, 
        entity: &Entity
    ) -> anyhow::Result<()> {
        if self.is_null() { bail!("Unfilled hotswap material") }
        self.get_ref().prep_render_entity(vgpu, pass, world, camera, entity)
    }
}