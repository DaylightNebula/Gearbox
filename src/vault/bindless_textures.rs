//! An [`AssetVault`] that loads textures into a single bindless texture array bind
//! group, letting a shader index into many textures via one binding.

use std::{hash::{Hash, Hasher}, marker::PhantomData, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use ahash::AHasher;
use anarchy::{Res, Scheduler, World, anyhow, macros::{Getters, Resource, system}};
use anarchy::anyhow::bail;
use cell::{App, Graphics, Plugin};
use derive_more::{Deref, DerefMut};
use image::{GenericImageView, ImageBuffer, Rgba};
use magician_vgpu::{SinglePass, StaticTexture, Texture, VirtualGpu, glam::UVec2};
use mutual::{CowData, DashMap, Ref, RefGuard, RelaxedMutex, SharedData};

use crate::{Asset, AssetContent, AssetVault, BindableAssetVault, Handle, HandleInner, LoadableAssetVault};

/// Plugin that adds the [`BindlessArrayTextureVault`] asset vault resource to the
/// app as well as the needed upkeep systems.
pub struct BindlessTexturesPlugin;
impl Plugin for BindlessTexturesPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(BindlessArrayTextureVault::default())
            .on_render_update(update_bindless_textures)
    }
}

/// A single texture uploaded into a [`BindlessArrayTextureVault`]'s bindless array.
#[derive(Getters)]
pub struct BindlessArrayTextureAsset {
    texture: StaticTexture,
    texture_idx: usize
}

impl Asset for BindlessArrayTextureAsset {
    type Vault = BindlessArrayTextureVault;
    type HandleTracker = (u64, Arc<BindlessArrayTextureVaultInner>);

    fn unload_threshold() -> usize { 2 }

    fn unload(tracker: &Self::HandleTracker) {
        tracker.1.texture_map.remove(&tracker.0);
        tracker.1.unloaded_textures.remove(&tracker.0);
    }
}

/// The encoding of texture bytes passed to [`LoadableAssetVault::load`].
///
/// This selects the decoder explicitly rather than guessing from the file 
/// signature, since some formats (e.g. TGA) have no magic bytes to sniff.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextureType {
    PNG,
    JPG,
    TGA,
    BMP,
    DDS,
    FARBFELD,
    HDR,
    ICO,
    OPENEXR,
    PNM,
    QOI,
    TIFF,
    WEBP
}

/// The [`AssetVault`] resource for [`BindlessArrayTextureAsset`]s.
///
/// A cheaply-clonable handle to the shared vault state; register one instance of
/// this as an ECS [`Resource`](anarchy::Resource) and load textures through it via
/// [`AssetVault::load`].
#[derive(Resource, Default, Clone, Deref, DerefMut)]
pub struct BindlessArrayTextureVault(Arc<BindlessArrayTextureVaultInner>);

/// Shared state backing a [`BindlessArrayTextureVault`].
///
/// A hash's handle lives in exactly one of three maps at a time, moving left to right:
/// `pending_loads` (content fetch/decode in flight) -> `unloaded_textures` (decoded,
/// not yet on the GPU) -> `texture_map` (uploaded). `unloaded_textures` is drained and
/// uploaded into `texture_arr` the next time [`BindableAssetVault::bind`] runs, at
/// which point the bind group is rebuilt if `dirty`.
pub struct BindlessArrayTextureVaultInner {
    texture_map: DashMap<u64, (Handle<BindlessArrayTextureAsset>, usize)>,
    texture_arr: RelaxedMutex<Vec<BindlessArrayTextureAsset>>,
    unloaded_textures: DashMap<u64, (Handle<BindlessArrayTextureAsset>, ImageBuffer<Rgba<u8>, Vec<u8>>, UVec2)>,
    pending_loads: DashMap<u64, Handle<BindlessArrayTextureAsset>>,
    bind_group: CowData<wgpu::BindGroup>,
    dirty: AtomicBool
}

impl PartialEq for BindlessArrayTextureVaultInner {
    fn eq(&self, _other: &Self) -> bool {
        true // Resources are inheirently singletons
    }
}

impl Default for BindlessArrayTextureVaultInner {
    fn default() -> Self {
        Self {
            texture_map: DashMap::default(),
            texture_arr: RelaxedMutex::new(Vec::with_capacity(16)),
            unloaded_textures: DashMap::default(),
            pending_loads: DashMap::default(),
            bind_group: CowData::null(),
            dirty: AtomicBool::new(false)
        }
    }
}

impl AssetVault for BindlessArrayTextureVault {
    type Asset = BindlessArrayTextureAsset;
    type Lookup = Handle<Self::Asset>;
    type LookupResult = Ref<Self::Asset>;

    fn get(&self, handle: &Handle<Self::Asset>) -> Option<Ref<Self::Asset>> {
        self.0.texture_map.get(&handle.inner().0)
            .map(|a| {
                Ref::new(
                    (self.texture_arr.lock_ref(), a.1), 
                    |b| {
                        let a = b
                            .downcast_ref::<(RefGuard<Vec<BindlessArrayTextureAsset>>, usize)>()
                            .unwrap();
                        &a.0[a.1]
                    }
                )
            })
    }
}

impl LoadableAssetVault for BindlessArrayTextureVault {
    type LoadType = TextureType;
    type LoadResult = Handle<Self::Asset>;

    fn load(&self, _world: &World, content: AssetContent, ty: TextureType) -> anyhow::Result<Handle<Self::Asset>> {
        // compute content hash
        let mut hasher = AHasher::default();
        content.hash(&mut hasher);
        let hash = hasher.finish();

        // get previous handle
        if let Some(value) = self.texture_map.get(&hash) { return Ok(value.0.clone()) };
        if let Some(value) = self.unloaded_textures.get(&hash) { return Ok(value.0.clone()) };
        if let Some(value) = self.pending_loads.get(&hash) { return Ok(value.clone()) };

        // atomically reserve this hash so a concurrent `load` of identical content, racing
        // with the checks above, joins this in-flight decode instead of starting a duplicate
        let handle = match self.pending_loads.entry(hash) {
            mutual::Entry::Occupied(existing) => return Ok(existing.get().clone()),
            mutual::Entry::Vacant(slot) => {
                let handle = HandleInner { inner: (hash, Arc::clone(&self.0)), _phantom: PhantomData::default() };
                let handle = Handle(Arc::new(handle));
                slot.insert(handle.clone());
                handle
            }
        };

        // the actual bytes are fetched/decoded off-thread and moved into `unloaded_textures`
        // once ready, where `update_bindless_textures` picks them up to do the (sync) GPU upload
        let inner = Arc::clone(&self.0);
        Scheduler::run_async(async move {
            let result = async {
                let bytes = content.into_bytes().await?;
                // formats like TGA have no magic-byte signature, so the format must be
                // supplied explicitly rather than guessed from the bytes
                let format = match ty {
                    TextureType::PNG => image::ImageFormat::Png,
                    TextureType::JPG => image::ImageFormat::Jpeg,
                    TextureType::TGA => image::ImageFormat::Tga,
                    TextureType::BMP => image::ImageFormat::Bmp,
                    TextureType::DDS => image::ImageFormat::Dds,
                    TextureType::FARBFELD => image::ImageFormat::Farbfeld,
                    TextureType::HDR => image::ImageFormat::Hdr,
                    TextureType::ICO => image::ImageFormat::Ico,
                    TextureType::OPENEXR => image::ImageFormat::OpenExr,
                    TextureType::PNM => image::ImageFormat::Pnm,
                    TextureType::QOI => image::ImageFormat::Qoi,
                    TextureType::TIFF => image::ImageFormat::Tiff,
                    TextureType::WEBP => image::ImageFormat::WebP,
                };
                let img = image::load_from_memory_with_format(&bytes, format)?;
                anyhow::Ok((img.dimensions(), img.to_rgba8()))
            }.await;

            // `remove` moves the handle out in one step rather than `get` (holding a read
            // guard) followed by a separate `remove` (which needs a write lock on the same
            // shard -- holding both at once self-deadlocks). It also avoids transiently
            // cloning the handle, which would push its refcount up and back down across
            // `unload_threshold` and could trigger a spurious unload of the entry we're
            // about to insert.
            let Some((_, staged_handle)) = inner.pending_loads.remove(&hash) else { return };
            match result {
                Ok((dimensions, rgba)) => {
                    inner.unloaded_textures.insert(hash, (staged_handle, rgba, dimensions.into()));
                }
                Err(err) => eprintln!("Failed to load bindless texture: {err}"),
            }
        });

        Ok(handle)
    }
}

impl BindableAssetVault for BindlessArrayTextureVault {
    fn bind_group_layout(&self, vgpu: &VirtualGpu) -> wgpu::BindGroupLayout {
        return vgpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("binding_array_textures_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: std::num::NonZeroU32::new(128),
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None
                        }
                    ]
                }
            );
    }

    fn bind(
        &self, 
        _vgpu: &VirtualGpu, 
        pass: &mut SinglePass, 
        bind_group: u32
    ) -> anyhow::Result<()> {
        if self.texture_arr.lock_ref().len() < 1 { bail!("No loaded textures") }
        if self.bind_group.is_null() { bail!("Missing bind group for bindless textures") }
        pass.bind_raw(bind_group, &self.bind_group.get_ref());
        Ok(())
    }
}


#[system(std::i32::MIN)]
pub fn update_bindless_textures(
    graphics: Res<Graphics>,
    vault: Res<BindlessArrayTextureVault>
) {
    // get all unloaded keys
    let unloaded_keys: Vec<u64> = vault.unloaded_textures.iter()
        .map(|a| *a.key())
        .collect();

    // loop through all unloaded keys, then mark dirty
    if !unloaded_keys.is_empty() {
        for key in unloaded_keys.into_iter() {
            // remove from unloaded cache
            let Some((_hash, (handle, rgba, dimensions))) = 
                vault.unloaded_textures.remove(&key) else { continue };

            // create new texture
            let texture = StaticTexture::from_raw(
                &*graphics, 
                magician_vgpu::TextureDescriptor {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    ..Default::default()
                }, 
                &rgba, 
                dimensions.x, 
                dimensions.y
            );

            // save texture
            let texture_idx = vault.texture_arr.lock_ref().len();
            vault.texture_arr.lock_mut().push(BindlessArrayTextureAsset { texture, texture_idx });
            vault.texture_map.insert(key, (handle, texture_idx));
        }
        vault.dirty.store(true, Ordering::Release);
    }

    // check if bind group needs rebuilding
    if (vault.dirty.swap(false, Ordering::AcqRel) || vault.bind_group.is_null()) && !vault.texture_arr.lock_mut().is_empty() {
        let binding = vault.texture_arr.lock_ref();
        let views = binding
            .iter()
            .map(|a| a.texture.view())
            .collect::<Vec<_>>();

        let bgl = vault.bind_group_layout(&graphics);

        let sampler = graphics.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("binding_array_textures_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = graphics.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("binding_array_textures_bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&views),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        vault.bind_group.set(bind_group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    const COBBLESTONE_PNG: &[u8] = include_bytes!("../../examples/cobblestone.png");
    const COBBLESTONE_JPG: &[u8] = include_bytes!("../../examples/cobblestone.jpg");
    const COBBLESTONE_TGA: &[u8] = include_bytes!("../../examples/cobblestone.tga");
    const COBBLESTONE_BMP: &[u8] = include_bytes!("../../examples/cobblestone.bmp");
    const COBBLESTONE_DDS: &[u8] = include_bytes!("../../examples/cobblestone.dds");
    const COBBLESTONE_FARBFELD: &[u8] = include_bytes!("../../examples/cobblestone.ff");
    const COBBLESTONE_HDR: &[u8] = include_bytes!("../../examples/cobblestone.hdr");
    const COBBLESTONE_ICO: &[u8] = include_bytes!("../../examples/cobblestone.ico");
    const COBBLESTONE_EXR: &[u8] = include_bytes!("../../examples/cobblestone.exr");
    const COBBLESTONE_PNM: &[u8] = include_bytes!("../../examples/cobblestone.ppm");
    const COBBLESTONE_QOI: &[u8] = include_bytes!("../../examples/cobblestone.qoi");
    const COBBLESTONE_TIFF: &[u8] = include_bytes!("../../examples/cobblestone.tiff");
    const COBBLESTONE_WEBP: &[u8] = include_bytes!("../../examples/cobblestone.webp");

    fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if condition() { return true; }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Loads `bytes` as `ty` and asserts it gets decoded and staged with the
    /// dimensions `image` itself reports for that format.
    fn assert_decodes(bytes: &'static [u8], ty: TextureType, format: image::ImageFormat) {
        let world = World::default();
        let vault = BindlessArrayTextureVault::default();
        let handle = vault.load(&world, AssetContent::Binary(bytes.into()), ty).unwrap();
        let hash = handle.inner.0;

        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&hash)));

        let staged = vault.unloaded_textures.get(&hash).unwrap();
        let expected_dimensions = image::load_from_memory_with_format(bytes, format).unwrap().dimensions();
        assert_eq!(staged.2, expected_dimensions.into());
    }

    #[test]
    fn load_decodes_binary_content_asynchronously() {
        assert_decodes(COBBLESTONE_PNG, TextureType::PNG, image::ImageFormat::Png);
    }

    #[test]
    fn load_decodes_jpg_content_asynchronously() {
        assert_decodes(COBBLESTONE_JPG, TextureType::JPG, image::ImageFormat::Jpeg);
    }

    #[test]
    fn load_decodes_tga_content_asynchronously() {
        assert_decodes(COBBLESTONE_TGA, TextureType::TGA, image::ImageFormat::Tga);
    }

    #[test]
    fn load_decodes_bmp_content_asynchronously() {
        assert_decodes(COBBLESTONE_BMP, TextureType::BMP, image::ImageFormat::Bmp);
    }

    #[test]
    fn load_decodes_dds_content_asynchronously() {
        assert_decodes(COBBLESTONE_DDS, TextureType::DDS, image::ImageFormat::Dds);
    }

    #[test]
    fn load_decodes_farbfeld_content_asynchronously() {
        assert_decodes(COBBLESTONE_FARBFELD, TextureType::FARBFELD, image::ImageFormat::Farbfeld);
    }

    #[test]
    fn load_decodes_hdr_content_asynchronously() {
        assert_decodes(COBBLESTONE_HDR, TextureType::HDR, image::ImageFormat::Hdr);
    }

    #[test]
    fn load_decodes_ico_content_asynchronously() {
        assert_decodes(COBBLESTONE_ICO, TextureType::ICO, image::ImageFormat::Ico);
    }

    #[test]
    fn load_decodes_openexr_content_asynchronously() {
        assert_decodes(COBBLESTONE_EXR, TextureType::OPENEXR, image::ImageFormat::OpenExr);
    }

    #[test]
    fn load_decodes_pnm_content_asynchronously() {
        assert_decodes(COBBLESTONE_PNM, TextureType::PNM, image::ImageFormat::Pnm);
    }

    #[test]
    fn load_decodes_qoi_content_asynchronously() {
        assert_decodes(COBBLESTONE_QOI, TextureType::QOI, image::ImageFormat::Qoi);
    }

    #[test]
    fn load_decodes_tiff_content_asynchronously() {
        assert_decodes(COBBLESTONE_TIFF, TextureType::TIFF, image::ImageFormat::Tiff);
    }

    #[test]
    fn load_decodes_webp_content_asynchronously() {
        assert_decodes(COBBLESTONE_WEBP, TextureType::WEBP, image::ImageFormat::WebP);
    }

    #[test]
    fn load_deduplicates_identical_content_once_staged() {
        let world = World::default();
        let vault = BindlessArrayTextureVault::default();
        let first = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), TextureType::PNG).unwrap();
        let hash = first.inner.0;
        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&hash)));

        let second = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), TextureType::PNG).unwrap();

        assert_eq!(second.inner.0, hash);
        assert_eq!(vault.unloaded_textures.len(), 1);
    }

    #[test]
    fn load_before_decode_finishes_joins_in_flight_load() {
        let world = World::default();
        let vault = BindlessArrayTextureVault::default();
        let first = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), TextureType::PNG).unwrap();
        // this call is made before the first's background decode has had a chance to
        // finish, so it must join the in-flight load rather than starting a duplicate
        let second = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), TextureType::PNG).unwrap();

        assert_eq!(first.inner.0, second.inner.0);
        assert_eq!(vault.pending_loads.len() + vault.unloaded_textures.len(), 1);

        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&first.inner.0)));
        assert_eq!(vault.unloaded_textures.len(), 1);
    }

    #[test]
    fn concurrent_loads_of_identical_content_do_not_race() {
        let vault = Arc::new(BindlessArrayTextureVault::default());
        let barrier = Arc::new(std::sync::Barrier::new(4));

        let handles: Vec<_> = (0..4).map(|_| {
            let vault = Arc::clone(&vault);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let world = World::default();
                barrier.wait();
                vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), TextureType::PNG).unwrap()
            })
        }).collect();

        let hashes: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap().inner.0).collect();
        assert!(hashes.windows(2).all(|w| w[0] == w[1]));

        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&hashes[0])));
        assert_eq!(vault.unloaded_textures.len(), 1);
    }

    #[test]
    fn load_stages_content_read_from_a_local_path() {
        let world = World::default();
        let path = std::env::temp_dir().join(format!("gearbox_bindless_texture_test_{:?}.png", std::thread::current().id()));
        std::fs::write(&path, COBBLESTONE_PNG).unwrap();

        let vault = BindlessArrayTextureVault::default();
        let handle = vault.load(&world, AssetContent::LocalPath(path.to_string_lossy().into_owned().into()), TextureType::PNG).unwrap();
        let hash = handle.inner.0;

        let staged = wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&hash));
        std::fs::remove_file(&path).unwrap();
        assert!(staged);
    }
}
