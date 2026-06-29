use std::{any::{Any, TypeId}, sync::OnceLock};

use ahash::AHashMap;
use anarchy::{Component, ComponentID, ComponentMeta, Entity, macros::Resource};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{Pipeline, PipelineBuilder, SinglePass, VirtualGpu};
use mutual::AsAny;

use crate::Camera;


/// Central storage for all pipelines in use by `Material`s.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct MaterialPipelineStorage {
    pipelines: AHashMap<(TypeId, TypeId), Pipeline>
}

/// Standard trait for any `Material` type.  All implemenator
/// of `Material` given a `Component` implementation but all
/// will have the same ID.
pub trait Material: Any {
    fn id(&self) -> TypeId { TypeId::of::<Self>() }
    fn create_pipeline<'a>(&'a self, vgpu: &VirtualGpu) -> PipelineBuilder<'a>;
    fn prep_render_entity<'a>(&'a self, vgpu: &VirtualGpu, pass: &mut SinglePass<'a>, camera: &Camera, entity: &'a Entity);
}


// MAKE MATERIAL A COMPONENT BY DEFAULT THAT HAVE THE SAME ID'S

#[derive(Deref, DerefMut)]
pub struct MaterialRef<A>(pub A);

static MATERIAL_COMPONENT_ID: OnceLock<ComponentID> = OnceLock::new();

impl <A: Material + 'static>  AsAny for MaterialRef<A> {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

impl <A: Material + 'static> ComponentMeta for MaterialRef<A> {
    fn bit_mask() -> ComponentID {
        *MATERIAL_COMPONENT_ID.get_or_init(|| {
            anarchy::ecs::components::NEXT_BIT_MASK
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        })
    }
}

impl <A: Material + 'static> Component for MaterialRef<A> {
    fn get_bit_mask(&self) -> ComponentID {
        Self::bit_mask()
    }
}
