use std::{any::TypeId, hash::{Hash, Hasher}, io::{BufReader, Cursor}, sync::Arc};

use ahash::AHasher;
use anarchy::{Res, Scheduler, World, anyhow::{self, Context}, macros::{AsAny, Resource, system}};
use cell::{App, Graphics, Plugin};
use derive_more::{Deref, DerefMut};
use mutual::{AsAny, CowData, DashMap, RefCowData};

use crate::{Asset, AssetContent, AssetVault, BasicMesh, Handle, LoadableAssetVault, Mesh, ShapeMeshData, glam::{Vec2, Vec3}, shaders};

pub struct MeshAssetPlugin;
impl Plugin for MeshAssetPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(MeshAssetVault::default())
            .on_render_update(finalize_mesh_loads)
    }
}

#[derive(Deref, DerefMut, AsAny)]
pub struct MeshAsset(pub Box<dyn Mesh>);

impl Asset for MeshAsset {
    type Vault = MeshAssetVault;
    type HandleTracker = (u64, Arc<MeshAssetVaultInner>, CowData<TypeId>);

    fn unload_threshold() -> usize { 2 }

    fn unload(tracker: &Self::HandleTracker) {
        tracker.1.remove(tracker.0);
    }
}

/// The file format used to interpret an asset's content in [`MeshAssetVault::load`].
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeshLoadType {
    /// Wavefront `.obj` geometry.
    OBJ
}

#[derive(Resource, Default, Deref, DerefMut)]
pub struct MeshAssetVault(Arc<MeshAssetVaultInner>);

/// Shared state backing a [`MeshAssetVault`].
///
/// A hash's handle lives in exactly one of three maps at a time, moving left to right:
/// `pending_loads` (file fetch/parse in flight) -> `unloaded_meshes` (parsed into a
/// [`ShapeMeshData`] intermediary, not yet on the GPU) -> `mesh` (uploaded).
/// `unloaded_meshes` is drained and uploaded into GPU buffers by [`finalize_mesh_loads`],
/// which runs on the render schedule since buffer creation must happen on the render thread.
#[derive(Default)]
pub struct MeshAssetVaultInner {
    pub mesh: DashMap<u64, (Handle<MeshAsset>, CowData<MeshAsset>)>,
    unloaded_meshes: DashMap<u64, (Handle<MeshAsset>, ShapeMeshData)>,
    pending_loads: DashMap<u64, Handle<MeshAsset>>
}

impl MeshAssetVault {
    pub fn new() -> Self { Self::default() }

    pub fn has(&self, handle: &Handle<MeshAsset>) -> bool { self.mesh.contains_key(&handle.inner.0) }

    pub fn get_handle(&self, hash: u64) -> Option<Handle<MeshAsset>> {
        self.mesh.get(&hash)
            .map(|a| a.0.clone())
    }

    pub fn load_raw(&self, hash: u64, asset: MeshAsset) -> Handle<MeshAsset> {
        let handle = Handle::new((hash, Arc::clone(&self.0), CowData::new(asset.id())));
        self.mesh.insert(hash, (handle.clone(), CowData::new(asset)));
        return handle;
    }
}

impl MeshAssetVaultInner {
    pub fn remove(&self, hash: u64) -> Option<(u64, (Handle<MeshAsset>, CowData<MeshAsset>))> {
        self.pending_loads.remove(&hash);
        self.unloaded_meshes.remove(&hash);
        self.mesh.remove(&hash)
    }
}

impl AssetVault for MeshAssetVault {
    type Asset = MeshAsset;
    type Lookup = Handle<MeshAsset>;
    type LookupResult = RefCowData<MeshAsset>;

    fn get(&self, handle: &Self::Lookup) -> Option<Self::LookupResult> {
        self.mesh.get(&handle.inner.0).map(|a| a.1.get_ref())
    }
}

impl AsAny for Handle<MeshAsset> {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

impl Mesh for Handle<MeshAsset> {
    fn id(&self) -> std::any::TypeId {
        *self.inner().2.get_ref()
    }

    fn create_pipeline<'a>(
        &self,
        vgpu: &magician_vgpu::VirtualGpu,
        world: &World
    ) -> anyhow::Result<magician_vgpu::PipelineBuilder<'a>> {
        let vault = world.get_resource_ref::<MeshAssetVault>()
            .context("Failed to find MeshAssetVault")?;
        let mesh = vault.get(&self)
            .context("Failed to get loaded MeshAsset")?;
        mesh.create_pipeline(vgpu, world)
    }

    fn draw(
        &self,
        vgpu: &magician_vgpu::VirtualGpu,
        pass: &mut magician_vgpu::SinglePass,
        world: &World,
        entity: &anarchy::Entity
    ) -> anyhow::Result<()> {
        let vault = world.get_resource_ref::<MeshAssetVault>()
            .context("Failed to find MeshAssetVault")?;
        let mesh = vault.get(&self)
            .context("Failed to get loaded MeshAsset")?;
        mesh.draw(vgpu, pass, world, entity)
    }
}

impl LoadableAssetVault for MeshAssetVault {
    type LoadType = MeshLoadType;
    type LoadResult = Handle<MeshAsset>;

    fn load(&self, _world: &World, content: AssetContent, ty: MeshLoadType) -> anarchy::anyhow::Result<Self::LoadResult> {
        // compute content hash
        let mut hasher = AHasher::default();
        content.hash(&mut hasher);
        let hash = hasher.finish();

        // return an existing or already in-flight handle for identical content
        if let Some(value) = self.mesh.get(&hash) { return Ok(value.0.clone()) }
        if let Some(value) = self.unloaded_meshes.get(&hash) { return Ok(value.0.clone()) }
        if let Some(value) = self.pending_loads.get(&hash) { return Ok(value.clone()) }

        // atomically reserve this hash so a concurrent `load` of identical content, racing
        // with the checks above, joins this in-flight parse instead of starting a duplicate
        let handle = match self.pending_loads.entry(hash) {
            mutual::Entry::Occupied(existing) => return Ok(existing.get().clone()),
            mutual::Entry::Vacant(slot) => {
                let handle = Handle::new((hash, Arc::clone(&self.0), CowData::new(TypeId::of::<BasicMesh>())));
                slot.insert(handle.clone());
                handle
            }
        };

        // the file is fetched and parsed off-thread into a `ShapeMeshData` intermediary,
        // moved into `unloaded_meshes` once ready; `finalize_mesh_loads` picks it up to
        // do the (render-thread-only) GPU buffer upload
        let inner = Arc::clone(&self.0);
        Scheduler::run_async(async move {
            let result = async {
                let bytes = content.into_bytes().await?;
                match ty {
                    MeshLoadType::OBJ => parse_obj(&bytes),
                }
            }.await;

            let Some(staged_handle) = inner.pending_loads.get(&hash) else { return };
            match result {
                Ok(mesh_data) => {
                    inner.unloaded_meshes.insert(hash, (staged_handle.clone(), mesh_data));
                    inner.pending_loads.remove(&hash);
                }
                Err(err) => eprintln!("Failed to load mesh: {err}"),
            }
        });

        Ok(handle)
    }
}

/// Parses Wavefront `.obj` bytes into a single [`ShapeMeshData`] of [`shaders::basic_vertex::VertexInput`]s,
/// triangulated and merged across every object/group in the file. Materials referenced by the
/// file are not resolved since [`MeshAsset`] only carries geometry.
fn parse_obj(bytes: &[u8]) -> anyhow::Result<ShapeMeshData> {
    let (models, _) = tobj::load_obj_buf(
        &mut BufReader::new(Cursor::new(bytes)),
        &tobj::GPU_LOAD_OPTIONS,
        |_| Ok(Default::default())
    )?;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for model in models {
        let mesh = model.mesh;
        let base = vertices.len() as u32;

        vertices.extend((0..mesh.positions.len() / 3).map(|i| shaders::basic_vertex::VertexInput {
            position: Vec3::new(mesh.positions[i * 3], mesh.positions[i * 3 + 1], mesh.positions[i * 3 + 2]).into(),
            uvs: (if mesh.texcoords.is_empty() { Vec2::ZERO } else { Vec2::new(mesh.texcoords[i * 2], mesh.texcoords[i * 2 + 1]) }).into(),
            normals: (if mesh.normals.is_empty() { Vec3::ZERO } else { Vec3::new(mesh.normals[i * 3], mesh.normals[i * 3 + 1], mesh.normals[i * 3 + 2]) }).into()
        }));
        indices.extend(mesh.indices.into_iter().map(|i| i + base));
    }

    Ok(ShapeMeshData { vertices, indices })
}

/// Uploads every mesh parsed by a background [`MeshAssetVault::load`] to the GPU, converting
/// its [`ShapeMeshData`] intermediary into a real [`MeshAsset`]. Runs on the render schedule
/// since GPU buffer creation must happen on the render thread.
#[system(std::i32::MIN)]
pub fn finalize_mesh_loads(
    graphics: Res<Graphics>,
    vault: Res<MeshAssetVault>
) {
    let ready_keys: Vec<u64> = vault.unloaded_meshes.iter()
        .map(|a| *a.key())
        .collect();

    for key in ready_keys {
        let Some((_hash, (handle, mesh_data))) = vault.unloaded_meshes.remove(&key) else { continue };
        let mesh = BasicMesh::new(&graphics, &mesh_data.vertices, &mesh_data.indices);
        vault.mesh.insert(key, (handle, CowData::new(MeshAsset(Box::new(mesh)))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    const TRIANGLE_OBJ: &str = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 0.0 1.0
vn 0.0 0.0 1.0
f 1/1/1 2/2/1 3/3/1
";

    fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if condition() { return true; }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn parse_obj_produces_one_vertex_per_face_corner() {
        let mesh_data = parse_obj(TRIANGLE_OBJ.as_bytes()).unwrap();
        assert_eq!(mesh_data.vertices.len(), 3);
        assert_eq!(mesh_data.indices, vec![0, 1, 2]);
    }

    #[test]
    fn load_parses_obj_content_asynchronously_into_unloaded_meshes() {
        let world = World::default();
        let vault = MeshAssetVault::default();
        let handle = vault.load(&world, AssetContent::Content(TRIANGLE_OBJ.to_string()), MeshLoadType::OBJ).unwrap();
        let hash = handle.inner.0;

        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_meshes.contains_key(&hash)));
        let staged = vault.unloaded_meshes.get(&hash).unwrap();
        assert_eq!(staged.1.vertices.len(), 3);
    }
}
