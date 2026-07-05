use std::any::{Any, TypeId};

use anarchy::macros::Component;
use derive_more::{Deref, DerefMut};
use magician_vgpu::{PipelineBuilder, SinglePass, VirtualGpu};

pub mod basic;

pub use basic::*;

/// Standard trait for any `Material` type.  All implemenator
/// of `Material` given a `Component` implementation but all
/// will have the same ID.
pub trait Mesh: Any {
    fn id(&self) -> TypeId { TypeId::of::<Self>() }

    fn create_pipeline<'a>(
        &'a self, 
        vgpu: &VirtualGpu
    ) -> PipelineBuilder<'a>;

    fn draw(
        &self,
        vgpu: &VirtualGpu,
        pass: &mut SinglePass, 
        entity: &anarchy::Entity
    );
}


// MAKE MATERIAL A COMPONENT BY DEFAULT THAT HAVE THE SAME ID'S

#[derive(Deref, DerefMut, Component)]
pub struct MeshRef(pub Box<dyn Mesh>);

impl MeshRef {
    pub fn new<M: Mesh + 'static>(mesh: M) -> Self {
        Self(Box::new(mesh))
    }
}
