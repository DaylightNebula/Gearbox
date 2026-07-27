use std::{any::TypeId, sync::Arc};

use anarchy::{Entity, World, anyhow::{self, bail}, macros::Resource};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{PipelineBuilder, SinglePass, VirtualGpu};
use mutual::{CowData, DashMap, RefCowData};

use crate::{Asset, AssetVault, Camera, Handle, LazyAssetVault, Material};

#[derive(Resource, Deref, DerefMut, Default)]
pub struct MaterialVault(Arc<MaterialVaultInner>);

#[derive(Default)]
pub struct MaterialVaultInner {
    storage: DashMap<u64, CowData<Box<dyn Material>>>
}

impl Asset for Box<dyn Material> {
    type Vault = MaterialVault;
    type HandleTracker = (u64, Arc<MaterialVaultInner>, CowData<TypeId>);

    fn unload_threshold() -> usize { 1 }
    fn unload(tracker: &Self::HandleTracker) {
        tracker.1.storage.remove(&tracker.0);
    }
}

impl AssetVault for MaterialVault {
    type Asset = Box<dyn Material>;
    type Lookup = Handle<Box<dyn Material>>;
    type LookupResult = RefCowData<Box<dyn Material>>;

    fn get(&self, handle: &Self::Lookup) -> Option<Self::LookupResult> {
        let Some(cow) = self.storage.get(&handle.inner().0) else { return None };
        if cow.is_null() { return None }
        return Some(cow.get_ref());
    }
}

impl LazyAssetVault for MaterialVault {
    type AllocTy = u64;
    type Store = Box<dyn Material>;

    fn allocate(&self, alloc: Self::AllocTy) -> anarchy::anyhow::Result<Self::Lookup> {
        let handle = Handle::new((alloc, Arc::clone(&self.0), CowData::new(TypeId::of::<Self::Store>())));
        self.storage.insert(alloc, CowData::null());
        return Ok(handle);
    }

    fn store(&self, _world: &anarchy::World, handle: Self::Lookup, store: Self::Store) {
        if let Some(cow) = self.storage.get(&handle.inner().0) {
            handle.inner().2.set(store.id());
            cow.set(store);
        } else {
            handle.inner().2.set(store.id());
            self.storage.insert(handle.inner().0, CowData::new(store));
        }
    }
}

impl Material for Handle<Box<dyn Material>> {
    fn id(&self) -> TypeId {
        *self.inner().2.get_ref()
    }

    fn create_pipeline<'a>(
        &self, 
        vgpu: &VirtualGpu,
        world: &World
    ) -> anyhow::Result<PipelineBuilder<'a>> {
        let Some(material) = world.get_resource_ref::<MaterialVault>()
            .and_then(|material| material.get(self))
            else { bail!("Failed to get material, either vault is missing or not loaded") };
        material.create_pipeline(vgpu, world)
    }

    fn prep_render_entity(
        &self, 
        vgpu: &VirtualGpu, 
        pass: &mut SinglePass, 
        world: &World,
        camera: &Camera, 
        entity: &Entity
    ) -> anyhow::Result<()> {
        let Some(material) = world.get_resource_ref::<MaterialVault>()
            .and_then(|material| material.get(self))
            else { bail!("Failed to get material, either vault is missing or not loaded") };
        material.prep_render_entity(vgpu, pass, world, camera, entity)
    }
}
