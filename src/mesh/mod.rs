use std::any::{Any, TypeId};

use anarchy::{Component, EntityID, World};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{PipelineBuilder, SinglePass, VirtualGpu};
use mutual::AsAny;

pub mod basic;

pub use basic::*;

/// Standard trait for any drawable mesh type. `id` identifies the concrete mesh type
/// (shared by all instances of that type, used to key material/mesh pipelines), and
/// `draw` binds the mesh's vertex/index/instance buffers and issues the draw call.
pub trait Mesh: Any + AsAny + Send {
    fn id(&self) -> TypeId { TypeId::of::<Self>() }

    fn create_pipeline<'a>(
        &self,
        vgpu: &VirtualGpu,
        world: &World
    ) -> anyhow::Result<PipelineBuilder<'a>>;

    fn draw(
        &self,
        vgpu: &VirtualGpu,
        pass: &mut SinglePass,
        world: &World,
        entity: EntityID
    ) -> anyhow::Result<()>;
}

/// A [`Component`](anarchy::Component) wrapping a type-erased [`Mesh`], attached to
/// an entity alongside a [`MaterialRef`](crate::MaterialRef) to make it renderable.
#[derive(Deref, DerefMut, Component)]
#[component(no_serialize)]
pub struct MeshRef(pub Box<dyn Mesh>);

impl std::fmt::Debug for MeshRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MeshRef").field(&self.0.id()).finish()
    }
}

impl MeshRef {
    /// Wraps `mesh` in a `MeshRef` component.
    pub fn new<M: Mesh + 'static>(mesh: M) -> Self {
        Self(Box::new(mesh))
    }
}
