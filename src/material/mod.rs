use std::any::{Any, TypeId};

use ahash::AHashMap;
use anarchy::{Entity, macros::{Component, Resource}};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{Pipeline, PipelineBuilder, SinglePass, VirtualGpu};

use crate::Camera;

pub mod basic;

pub use basic::*;


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

#[derive(Deref, DerefMut, Component)]
pub struct MaterialRef(pub Box<dyn Material>);

impl MaterialRef {
    pub fn new<M: Material>(material: M) -> Self {
        MaterialRef(Box::new(material))
    }
}
