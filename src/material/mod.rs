use std::any::{Any, TypeId};

use ahash::AHashMap;
use anarchy::{Entity, macros::{Component, Resource}};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{Pipeline, PipelineBuilder, SinglePass, VirtualGpu};
use mutual::{AsAny, CowData};

use crate::Camera;

pub mod basic;
pub mod simple_textured;

pub use basic::*;
pub use simple_textured::*;

/// Central storage for all pipelines in use by `Material`s.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct MaterialPipelineStorage {
    pipelines: AHashMap<(TypeId, TypeId), CowData<Pipeline>>
}

/// Standard trait for any `Material` type. `id` identifies the concrete material type
/// (shared by all instances of that type, used to key material/mesh pipelines in
/// [`MaterialPipelineStorage`]), `create_pipeline` builds the render pipeline for
/// this material, and `prep_render_entity` binds per-entity buffers before drawing.
pub trait Material: Any + AsAny {
    fn id(&self) -> TypeId { TypeId::of::<Self>() }
    fn create_pipeline<'a>(&'a self, vgpu: &VirtualGpu) -> PipelineBuilder<'a>;
    fn prep_render_entity(&self, vgpu: &VirtualGpu, pass: &mut SinglePass, camera: &Camera, entity: &Entity);
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
