use std::{any::{Any, TypeId}, sync::OnceLock};

use anarchy::{Component, ComponentID, ComponentMeta};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{PipelineBuilder, SinglePass, VirtualGpu};
use mutual::AsAny;

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

    fn draw<'a>(
        &'a self,
        vgpu: &VirtualGpu,
        pass: &mut SinglePass<'a>, 
        entity: &'a anarchy::Entity
    );
}


// MAKE MATERIAL A COMPONENT BY DEFAULT THAT HAVE THE SAME ID'S

#[derive(Deref, DerefMut)]
pub struct MeshRef<A>(pub A);

static MESH_COMPONENT_ID: OnceLock<ComponentID> = OnceLock::new();

impl <A: Mesh + 'static>  AsAny for MeshRef<A> {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

impl <A: Mesh + 'static> ComponentMeta for MeshRef<A> {
    fn bit_mask() -> ComponentID {
        *MESH_COMPONENT_ID.get_or_init(|| {
            anarchy::ecs::components::NEXT_BIT_MASK
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        })
    }
}

impl <A: Mesh + 'static> Component for MeshRef<A> {
    fn get_bit_mask(&self) -> ComponentID {
        Self::bit_mask()
    }
}
