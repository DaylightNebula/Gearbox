use std::{hash::{Hash, Hasher}, path::PathBuf, sync::Arc};

use ahash::AHasher;
use anarchy::{Res, Resource, SystemPin, World, pin};
use anyhow::Context;
use derive_more::{Deref, DerefMut};
use gltf::Gltf;
use mutual::{CowData, DashMap, RefCowData};
use crate::{App, AssetContent, AssetVault, BasicMaterial, Graphics, Handle, LazyAssetVault, LoadableAssetVault, Material, MaterialVault, MeshAssetVault, Plugin, glam::Vec4, run_async};

use crate::skeletal::{AnimationSet, AnimationVault, MAX_SKELETAL_BONES, SkeletalMesh, SkeletalMeshHandle, advance_animators, loader};

/// Registers the `SkeletalMeshVault` resource and the render-schedule system
/// that finishes loading meshes queued by it. Required for `SkeletalMeshVault::load`
/// to ever resolve a queued glTF/FBX file into a usable `SkeletalMesh`.
pub struct SkeletalMeshVaultPlugin;
impl Plugin for SkeletalMeshVaultPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(SkeletalMeshVault::default())
            .on_render_update_with(load_inprogress, pin(SystemPin::End))
            .on_render_update_with(advance_animators, pin(SystemPin::Start))
    }
}

/// The source file format to parse a `SkeletalMeshVault::load` call's content as.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkeletalMeshLoadType {
    GLTF,
    #[cfg(not(target_arch = "wasm32"))]
    FBX
}

/// Resource that loads and caches `SkeletalMesh`es, deduplicated by content hash.
/// A cheap `Arc`-backed handle to `SkeletalMeshVaultInner`; cloning it shares the
/// same underlying cache.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct SkeletalMeshVault(Arc<SkeletalMeshVaultInner>);

impl std::fmt::Debug for SkeletalMeshVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkeletalMeshVault").field("meshes", &self.mesh.len()).finish_non_exhaustive()
    }
}

/// Tracks a mesh through its load pipeline, one map per stage: `preload` holds
/// a handle while its file bytes are being read asynchronously; once read, the
/// bytes are parsed (off the render thread) into `inprogress_gltf` or
/// `inprogress_fbx`; `load_inprogress` then converts that parsed scene into GPU
/// resources on the render thread and moves the result into `mesh`, the final
/// cache queried by `get`/`has`. A hash only ever lives in one map at a time.
#[derive(Default)]
pub struct SkeletalMeshVaultInner {
    pub mesh: DashMap<u64, ((SkeletalMeshHandle, Handle<Box<dyn Material>>, Handle<AnimationSet>), CowData<SkeletalMesh>)>,
    pub preload: DashMap<u64, (SkeletalMeshHandle, Handle<Box<dyn Material>>, Handle<AnimationSet>)>,
    pub inprogress_gltf: DashMap<u64, ((SkeletalMeshHandle, Handle<Box<dyn Material>>, Handle<AnimationSet>), Gltf)>,
    #[cfg(not(target_arch = "wasm32"))]
    pub inprogress_fbx: DashMap<u64, ((SkeletalMeshHandle, Handle<Box<dyn Material>>, Handle<AnimationSet>), ufbx::SceneRoot)>
}

unsafe impl Send for SkeletalMeshVaultInner {}
unsafe impl Sync for SkeletalMeshVaultInner {}

impl SkeletalMeshVault {
    pub fn new() -> Self { Self::default() }

    /// Whether a fully-loaded mesh exists in the cache for this handle.
    pub fn has(&self, handle: &SkeletalMeshHandle) -> bool { self.mesh.contains_key(&handle.handle().inner().0) }

    /// Look up an already-cached handle by content hash, if a mesh has finished loading for it.
    pub fn get_handle(&self, hash: u64) -> Option<SkeletalMeshHandle> {
        self.mesh.get(&hash)
            .map(|a| a.0.0.clone())
    }

    // /// Insert an already-constructed `SkeletalMesh` directly into the cache under
    // /// `hash`, skipping the async file-load pipeline, and return its handle.
    // pub fn load_raw(&self, hash: u64, asset: SkeletalMesh) -> SkeletalMeshHandle {
    //     let handle = Handle::new((hash, Arc::clone(&self.0)));
    //     // let material: HotSwapMaterial = CowData::new(Box::new(asset.material.unwrap_or(|| BasicMaterial::new(Vec4::ONE))));
    //     // let material: HotSwapMaterial = 
    //     //     if let Some(mat) = &asset.material { CowData::new(Box::new(mat.clone())) }
    //     //     else { CowData::new(Box::new(BasicMaterial::new(Vec4::ONE))) };
    //     let handle = SkeletalMeshHandle::new(handle, material);
    //     self.mesh.insert(hash, (handle.clone(), CowData::new(asset)));
    //     return handle;
    // }
}

impl SkeletalMeshVaultInner {
    /// Drop a fully-loaded mesh from the cache. Called by `SkeletalMesh::unload`
    /// once its handle's reference count drops to the unload threshold.
    pub fn remove(&self, hash: u64) {
        self.preload.remove(&hash);
        #[cfg(not(target_arch = "wasm32"))]
        self.inprogress_fbx.remove(&hash);
        self.inprogress_gltf.remove(&hash);
        self.mesh.remove(&hash);
    }
}

impl AssetVault for SkeletalMeshVault {
    type Asset = SkeletalMesh;
    type Lookup = SkeletalMeshHandle;
    type LookupResult = RefCowData<SkeletalMesh>;

    fn get(&self, handle: &Self::Lookup) -> Option<Self::LookupResult> {
        self.mesh.get(&handle.handle().inner().0).map(|a| a.1.get_ref())
    }
}

impl LoadableAssetVault for SkeletalMeshVault {
    type LoadType = SkeletalMeshLoadType;
    type LoadResult = (SkeletalMeshHandle, Handle<Box<dyn Material>>, Handle<AnimationSet>);

    /// Queue `content` for loading as `ty` and return a handle immediately.
    /// Hashes `content` first and, if a handle already exists for that hash
    /// (loaded, or anywhere in the load pipeline), returns the existing handle
    /// instead of loading a duplicate. Otherwise spawns an async task that
    /// reads the content's bytes and parses them into a `Gltf`/`ufbx::SceneRoot`,
    /// stashing the result in `inprogress_gltf`/`inprogress_fbx` for the
    /// `load_inprogress` render-schedule system to finish (uploading GPU
    /// resources requires the render thread, so that part can't happen here).
    fn load(&self, world: &World, content: AssetContent, ty: SkeletalMeshLoadType) -> anyhow::Result<Self::LoadResult> {
        let mat_vault = world.resource::<MaterialVault>()
            .context("Missing material vault")?;
        let anim_vault = world.resource::<AnimationVault>()
            .context("Missing animation vault")?;

        // get content hash
        let mut hasher = AHasher::default();
        content.hash(&mut hasher);
        let hash = hasher.finish();

        // attempt to find previous handle with the same hash and return that
        if let Some(handle) = self.mesh.get(&hash) { return Ok(handle.0.clone()); }
        if let Some(handle) = self.inprogress_gltf.get(&hash) { return Ok(handle.0.clone()); }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(handle) = self.inprogress_fbx.get(&hash) { return Ok(handle.0.clone()); }
        if let Some(handle) = self.preload.get(&hash) { return Ok(handle.clone()); }

        // create new handle and store inprogress
        let handle: Handle<SkeletalMesh> = Handle::new((hash, Arc::clone(&self.0)));
        let material: Handle<Box<dyn Material + 'static>> = mat_vault.allocate(handle.inner().0)?;
        let animations: Handle<AnimationSet> = anim_vault.allocate(handle.inner().0)?;
        let handle = SkeletalMeshHandle::new(handle);
        self.preload.insert(hash, (handle.clone(), material.clone(), animations.clone()));

        // start load
        let inner = Arc::clone(&self.0);
        let handle2 = (handle.clone(), material.clone(), animations.clone());
        run_async(async move {
            let bytes = content.into_bytes()
                .await
                .expect("Failed to read skeletal mesh content");
            
            match ty {
                SkeletalMeshLoadType::GLTF => {
                    let gltf = Gltf::from_slice(&bytes)
                        .expect("Failed to read gltf from bytes");
                    inner.inprogress_gltf.insert(hash, (handle2, gltf));
                },

                #[cfg(not(target_arch = "wasm32"))]
                SkeletalMeshLoadType::FBX => {
                    let fbx = ufbx::load_memory(&bytes, loader::fbx::load_opts())
                        .map_err(|e| anyhow::anyhow!("Failed to load fbx: {}", e.description))
                        .expect("Failed to load FBX");
                    inner.inprogress_fbx.insert(hash, (handle2, fbx));
                },
            }

            inner.preload.remove(&hash);
        });

        Ok((handle, material, animations))
    }
}

/// Finishes loading every mesh currently sitting in `inprogress_gltf`/`inprogress_fbx`:
/// turns each parsed scene into GPU resources (requires the render thread, hence
/// this being a render-schedule system) and moves the result into the vault's
/// `mesh` cache. Pinned to the end of the render schedule, after every other render
/// system this frame, so a mesh finished here becomes visible to the rest of the
/// render schedule starting next frame. Meshes that fail to load are logged and dropped.
pub fn load_inprogress(
    world: World,
    graphics: Res<Graphics>,
    vault: Res<SkeletalMeshVault>,
    meshes: Res<MeshAssetVault>,
    materials: Res<MaterialVault>,
    animations: Res<AnimationVault>
) {
    // take copy of all hashes in the inprogress maps
    let inprogress_gltf_hashes = vault.inprogress_gltf.iter()
        .map(|a| *a.key())
        .collect::<Vec<_>>();
    #[cfg(not(target_arch = "wasm32"))]
    let inprogress_fbx_hashes = vault.inprogress_fbx.iter()
        .map(|a| *a.key())
        .collect::<Vec<_>>();

    // saves a loaded mesh along with its material and animations
    let finish = |hash: u64, handle: (SkeletalMeshHandle, Handle<Box<dyn Material>>, Handle<AnimationSet>), mesh: SkeletalMesh, anims| {
        let joints = mesh.skin().as_ref().map_or(0, |skin| skin.len());
        let nodes = mesh.node_count();
        if joints > MAX_SKELETAL_BONES || nodes > MAX_SKELETAL_BONES {
            log::warn!("Skeletal mesh has {joints} joints and {nodes} nodes, but only {MAX_SKELETAL_BONES} of each are animated");
        }
        let material: Box<dyn Material> = mesh.material().clone()
            .map::<Box<dyn Material>, _>(|a| Box::new(a))
            .unwrap_or_else(|| Box::new(BasicMaterial::new(Vec4::ONE)));
        materials.store(&world, handle.1.clone(), material);
        animations.store(&world, handle.2.clone(), AnimationSet(anims));
        vault.mesh.insert(hash, (handle, CowData::new(mesh)));
    };

    for hash in inprogress_gltf_hashes.into_iter() {
        if let Some(content) = vault.inprogress_gltf.get(&hash) {
            let handle = content.0.clone();
            match loader::gltf::load(&content.1, &world, &graphics, &meshes, &PathBuf::new(), &PathBuf::new(), None, hash) {
                Ok((mesh, anims)) => finish(hash, handle, mesh, anims),
                Err(err) => log::error!("Failed to load gltf skeletal mesh: {err:#}"),
            }
        }

        vault.inprogress_gltf.remove(&hash);
    }

    #[cfg(not(target_arch = "wasm32"))]
    for hash in inprogress_fbx_hashes.into_iter() {
        if let Some(content) = vault.inprogress_fbx.get(&hash) {
            let handle = content.0.clone();
            match loader::fbx::load(&content.1, &world, &graphics, &meshes, None, None, hash) {
                Ok((mesh, anims)) => finish(hash, handle, mesh, anims),
                Err(err) => log::error!("Failed to load fbx skeletal mesh: {err:#}"),
            }
        }

        vault.inprogress_fbx.remove(&hash);
    }
}
