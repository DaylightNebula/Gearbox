use std::sync::Arc;

use anarchy::macros::Resource;
use cell::{App, Plugin};
use derive_more::{Deref, DerefMut};
use mutual::{CowData, DashMap, RefCowData};

use crate::{Asset, AssetContent, AssetVault, Handle, Mesh};

pub struct MeshAssetPlugin;
impl Plugin for MeshAssetPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(MeshAssetVault::default())
    }
}

#[derive(Deref, DerefMut)]
pub struct MeshAsset(pub Box<dyn Mesh>);

impl Asset for MeshAsset {
    type Vault = MeshAssetVault;
    type HandleTracker = (u64, Arc<MeshAssetVaultInner>);

    fn unload_threshold() -> usize { 2 }

    fn unload(tracker: &Self::HandleTracker) {
        tracker.1.remove(tracker.0);
    }
}

#[derive(Resource, Default, Deref, DerefMut)]
pub struct MeshAssetVault(Arc<MeshAssetVaultInner>);

#[derive(Default)]
pub struct MeshAssetVaultInner {
    pub mesh: DashMap<u64, (Handle<MeshAsset>, CowData<MeshAsset>)>
}

impl MeshAssetVault {
    pub fn new() -> Self { Self::default() }

    pub fn has(&self, handle: &Handle<MeshAsset>) -> bool { self.mesh.contains_key(&handle.inner.0) }

    pub fn get_handle(&self, hash: u64) -> Option<Handle<MeshAsset>> {
        self.mesh.get(&hash)
            .map(|a| a.0.clone())
    }

    pub fn load_raw(&self, hash: u64, asset: MeshAsset) -> Handle<MeshAsset> {
        let handle = Handle::new((hash, Arc::clone(&self.0)));
        self.mesh.insert(hash, (handle.clone(), CowData::new(asset)));
        return handle;
    }
}

impl MeshAssetVaultInner {
    pub fn remove(&self, hash: u64) -> Option<(u64, (Handle<MeshAsset>, CowData<MeshAsset>))> {
        self.mesh.remove(&hash)
    }
}

impl AssetVault for MeshAssetVault {
    type Asset = MeshAsset;
    type LoadResult = Handle<MeshAsset>;
    type Lookup = Handle<MeshAsset>;
    type LookupResult = RefCowData<MeshAsset>;

    fn get(&self, handle: &Self::Lookup) -> Option<Self::LookupResult> {
        self.mesh.get(&handle.inner.0).map(|a| a.1.get_ref())
    }

    fn load(&self, _content: AssetContent) -> anarchy::anyhow::Result<Self::LoadResult> {
        todo!("Load obj files from asset content")
    }
}
