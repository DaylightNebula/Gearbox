use std::any::{Any, TypeId};

use anarchy::{Entity, World, macros::Component};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{PipelineBuilder, SinglePass, VirtualGpu};

pub mod basic;

pub use basic::*;
use mutual::AsAny;

/// Standard trait for any drawable mesh type. `id` identifies the concrete mesh type
/// (shared by all instances of that type, used to key material/mesh pipelines), and
/// `draw` binds the mesh's vertex/index/instance buffers and issues the draw call.
pub trait Mesh: Any + AsAny {
    fn id(&self) -> TypeId { TypeId::of::<Self>() }

    fn create_pipeline<'a>(
        &'a self,
        vgpu: &VirtualGpu
    ) -> PipelineBuilder<'a>;

    fn draw(
        &self,
        vgpu: &VirtualGpu,
        pass: &mut SinglePass,
        world: &World,
        entity: &Entity
    );
}

/// A [`Component`](anarchy::Component) wrapping a type-erased [`Mesh`], attached to
/// an entity alongside a [`MaterialRef`](crate::MaterialRef) to make it renderable.
#[derive(Deref, DerefMut, Component)]
pub struct MeshRef(pub Box<dyn Mesh>);

impl MeshRef {
    /// Wraps `mesh` in a `MeshRef` component.
    pub fn new<M: Mesh + 'static>(mesh: M) -> Self {
        Self(Box::new(mesh))
    }
}
