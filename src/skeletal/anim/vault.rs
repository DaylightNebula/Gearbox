use std::sync::Arc;

use ahash::AHashMap;
use anarchy::Resource;
use derive_more::{Deref, DerefMut};
use crate::{Asset, AssetVault, Handle, LazyAssetVault};
use mutual::{CowData, DashMap, RefCowData};

use crate::skeletal::PreProcessAnimation;

#[derive(Resource, Deref, DerefMut, Default)]
pub struct AnimationVault(Arc<AnimationVaultInner>);

impl std::fmt::Debug for AnimationVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnimationVault").field("animations", &self.storage.len()).finish()
    }
}

#[derive(Default)]
pub struct AnimationVaultInner {
    storage: DashMap<u64, CowData<AnimationSet>>
}

#[derive(Default, Deref, DerefMut, Debug)]
pub struct AnimationSet(pub AHashMap<String, PreProcessAnimation>);

impl Asset for AnimationSet {
    type Vault = AnimationVault;
    type HandleTracker = (u64, Arc<AnimationVaultInner>);

    fn unload_threshold() -> usize { 1 }
    fn unload(tracker: &Self::HandleTracker) {
        tracker.1.storage.remove(&tracker.0);
    }
}

impl AssetVault for AnimationVault {
    type Asset = AnimationSet;
    type Lookup = Handle<AnimationSet>;
    type LookupResult = RefCowData<AnimationSet>;

    fn get(&self, handle: &Self::Lookup) -> Option<Self::LookupResult> {
        let Some(cow) = self.storage.get(&handle.inner().0) else { return None };
        if cow.is_null() { return None }
        return Some(cow.get_ref());
    }
}

impl LazyAssetVault for AnimationVault {
    type AllocTy = u64;
    type Store = AnimationSet;

    fn allocate(&self, alloc: Self::AllocTy) -> anyhow::Result<Self::Lookup> {
        let handle = Handle::new((alloc, Arc::clone(&self.0)));
        self.storage.insert(alloc, CowData::null());
        return Ok(handle);
    }

    fn store(&self, _world: &anarchy::World, handle: Self::Lookup, store: Self::Store) {
        if let Some(cow) = self.storage.get(&handle.inner().0) {
            cow.set(store);
        } else {
            self.storage.insert(handle.inner().0, CowData::new(store));
        }
    }
}
