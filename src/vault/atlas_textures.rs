//! An [`AssetVault`] that packs textures into a single, ever-growing texture atlas bound
//! as one bind group, letting a shader sample many textures without a GPU binding array.
//! This is a fallback for [`BindlessArrayTextureVault`] on backends that don't support
//! `wgpu::Features::TEXTURE_BINDING_ARRAY` (e.g. WebGL2/GLES) -- see [`crate::TextureVault`]
//! for the facade that picks between the two automatically.

use std::{hash::{Hash, Hasher}, marker::PhantomData, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use ahash::AHasher;
use anarchy::{Res, Scheduler, World, anyhow::{self, bail}, macros::{Getters, Resource, system}};
use cell::{App, Graphics, Plugin};
use derive_more::{Deref, DerefMut};
use image::{GenericImageView, ImageBuffer, Rgba};
use magician_vgpu::{BindGroupProvider, Buffer, MutableBuffer, SinglePass, StaticTexture, Texture, VirtualGpu, WritableBuffer, glam::UVec2, rust::Vec4};
use mutual::{CowData, DashMap, Ref, RefGuard, RelaxedMutex, SharedData};

use crate::{Asset, AssetContent, AssetVault, BindableAssetVault, BindlessArrayTextureType, Handle, HandleInner, LoadableAssetVault, shaders};

/// Plugin that adds the [`AtlasTextureVault`] asset vault resource to the app as well
/// as the needed upkeep systems.
pub struct AtlasTexturesPlugin;
impl Plugin for AtlasTexturesPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(AtlasTextureVault::default())
            .on_render_update(update_atlas_textures)
    }
}

/// A minimal append-only bin packer: rects are placed left-to-right in horizontal shelves
/// and never moved once placed, growing the page (power-of-two doubling) when they don't
/// fit. Trades packing density for the simplicity and "never moves" guarantee a growable
/// atlas needs.
// ponytail: naive shelf packing wastes space when placed rects have very different heights;
// upgrade to skyline/guillotine packing if atlas memory pressure becomes a real problem.
struct ShelfPacker {
    width: u32,
    height: u32,
    shelf_y: u32,
    shelf_height: u32,
    cursor_x: u32
}

impl ShelfPacker {
    fn new(width: u32, height: u32) -> Self {
        Self { width, height, shelf_y: 0, shelf_height: 0, cursor_x: 0 }
    }

    /// Places a `w x h` rect, growing the page (power-of-two doubling, capped at `max_dim`)
    /// if it doesn't fit. Returns the pixel offset it was placed at, and the page's new
    /// size if placing it required growing.
    fn place(&mut self, w: u32, h: u32, max_dim: u32) -> anyhow::Result<(UVec2, Option<UVec2>)> {
        if w > max_dim || h > max_dim {
            bail!("Texture {w}x{h} exceeds the max atlas page dimension ({max_dim})");
        }
        // defensive clamp in case a prior page size (e.g. the 1024x1024 default) exceeds
        // this adapter's actual limit
        self.width = self.width.min(max_dim);
        self.height = self.height.min(max_dim);

        if self.cursor_x + w > self.width {
            self.shelf_y += self.shelf_height;
            self.cursor_x = 0;
            self.shelf_height = 0;
        }

        let mut grew = None;
        while self.cursor_x + w > self.width || self.shelf_y + h > self.height {
            let (new_width, new_height) = ((self.width * 2).min(max_dim), (self.height * 2).min(max_dim));
            if new_width == self.width && new_height == self.height {
                bail!("Atlas page is full at the maximum size ({max_dim}x{max_dim}) and cannot fit a {w}x{h} texture");
            }
            self.width = new_width;
            self.height = new_height;
            grew = Some(UVec2::new(self.width, self.height));
        }

        let offset = UVec2::new(self.cursor_x, self.shelf_y);
        self.cursor_x += w;
        self.shelf_height = self.shelf_height.max(h);
        Ok((offset, grew))
    }
}

/// A single texture placed into an [`AtlasTextureVault`]'s atlas page.
#[derive(Getters, Clone, Copy)]
pub struct AtlasTextureAsset {
    offset_px: UVec2,
    size_px: UVec2
}

impl Asset for AtlasTextureAsset {
    type Vault = AtlasTextureVault;
    type HandleTracker = (u64, Arc<AtlasTextureVaultInner>);

    fn unload_threshold() -> usize { 2 }

    fn unload(tracker: &Self::HandleTracker) {
        tracker.1.texture_map.remove(&tracker.0);
        tracker.1.unloaded_textures.remove(&tracker.0);
    }
}

/// The [`AssetVault`] resource for [`AtlasTextureAsset`]s.
///
/// A cheaply-clonable handle to the shared vault state; register one instance of this as
/// an ECS [`Resource`](anarchy::Resource) and load textures through it via [`AssetVault::load`].
#[derive(Resource, Default, Clone, Deref, DerefMut)]
pub struct AtlasTextureVault(Arc<AtlasTextureVaultInner>);

/// Shared state backing an [`AtlasTextureVault`].
///
/// Mirrors [`BindlessArrayTextureVaultInner`](crate::BindlessArrayTextureVaultInner)'s
/// load pipeline (`pending_loads` -> `unloaded_textures` -> `texture_map`) exactly; the only
/// difference is what happens once a texture is ready to go on the GPU: instead of pushing
/// it into a `Vec` of standalone textures bound as a binding array, it's packed into a
/// single growable atlas `page` via `packer`, and its permanent pixel-space rect is recorded
/// in `texture_arr`.
pub struct AtlasTextureVaultInner {
    texture_map: DashMap<u64, (Handle<AtlasTextureAsset>, usize)>,
    texture_arr: RelaxedMutex<Vec<AtlasTextureAsset>>,
    unloaded_textures: DashMap<u64, (Handle<AtlasTextureAsset>, ImageBuffer<Rgba<u8>, Vec<u8>>, UVec2)>,
    pending_loads: DashMap<u64, Handle<AtlasTextureAsset>>,
    packer: RelaxedMutex<ShelfPacker>,
    page: CowData<StaticTexture>,
    atlas_size_buffer: CowData<MutableBuffer<Vec4>>,
    bind_group: CowData<wgpu::BindGroup>,
    dirty: AtomicBool
}

/// Starting size (both dimensions) of a vault's atlas page, before any growth.
const INITIAL_ATLAS_SIZE: u32 = 1024;

impl PartialEq for AtlasTextureVaultInner {
    fn eq(&self, _other: &Self) -> bool {
        true // Resources are inheirently singletons
    }
}

impl Default for AtlasTextureVaultInner {
    fn default() -> Self {
        Self {
            texture_map: DashMap::default(),
            texture_arr: RelaxedMutex::new(Vec::with_capacity(16)),
            unloaded_textures: DashMap::default(),
            pending_loads: DashMap::default(),
            packer: RelaxedMutex::new(ShelfPacker::new(INITIAL_ATLAS_SIZE, INITIAL_ATLAS_SIZE)),
            page: CowData::null(),
            atlas_size_buffer: CowData::null(),
            bind_group: CowData::null(),
            dirty: AtomicBool::new(false)
        }
    }
}

impl AssetVault for AtlasTextureVault {
    type Asset = AtlasTextureAsset;
    type Lookup = Handle<Self::Asset>;
    type LookupResult = Ref<Self::Asset>;

    fn get(&self, handle: &Handle<Self::Asset>) -> Option<Ref<Self::Asset>> {
        self.0.texture_map.get(&handle.inner().0)
            .map(|a| {
                Ref::new(
                    (self.texture_arr.lock_ref(), a.1),
                    |b| {
                        let a = b
                            .downcast_ref::<(RefGuard<Vec<AtlasTextureAsset>>, usize)>()
                            .unwrap();
                        &a.0[a.1]
                    }
                )
            })
    }
}

impl LoadableAssetVault for AtlasTextureVault {
    type LoadType = BindlessArrayTextureType;
    type LoadResult = Handle<Self::Asset>;

    fn load(&self, _world: &World, content: AssetContent, ty: BindlessArrayTextureType) -> anyhow::Result<Handle<Self::Asset>> {
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
        // once ready, where `update_atlas_textures` picks them up to do the (sync) GPU upload
        let inner = Arc::clone(&self.0);
        Scheduler::run_async(async move {
            let result = async {
                let bytes = content.into_bytes().await?;
                // formats like TGA have no magic-byte signature, so the format must be
                // supplied explicitly rather than guessed from the bytes
                let format = match ty {
                    BindlessArrayTextureType::PNG => image::ImageFormat::Png,
                    BindlessArrayTextureType::JPG => image::ImageFormat::Jpeg,
                    BindlessArrayTextureType::TGA => image::ImageFormat::Tga,
                    BindlessArrayTextureType::BMP => image::ImageFormat::Bmp,
                    BindlessArrayTextureType::DDS => image::ImageFormat::Dds,
                    BindlessArrayTextureType::FARBFELD => image::ImageFormat::Farbfeld,
                    BindlessArrayTextureType::HDR => image::ImageFormat::Hdr,
                    BindlessArrayTextureType::ICO => image::ImageFormat::Ico,
                    BindlessArrayTextureType::OPENEXR => image::ImageFormat::OpenExr,
                    BindlessArrayTextureType::PNM => image::ImageFormat::Pnm,
                    BindlessArrayTextureType::QOI => image::ImageFormat::Qoi,
                    BindlessArrayTextureType::TIFF => image::ImageFormat::Tiff,
                    BindlessArrayTextureType::WEBP => image::ImageFormat::WebP,
                };
                let img = image::load_from_memory_with_format(&bytes, format)?;
                anyhow::Ok((img.dimensions(), img.to_rgba8()))
            }.await;

            let Some((_, staged_handle)) = inner.pending_loads.remove(&hash) else { return };
            match result {
                Ok((dimensions, rgba)) => {
                    inner.unloaded_textures.insert(hash, (staged_handle, rgba, dimensions.into()));
                }
                Err(err) => eprintln!("Failed to load atlas texture: {err}"),
            }
        });

        Ok(handle)
    }
}

impl BindableAssetVault for AtlasTextureVault {
    fn bind_group_layout(&self, vgpu: &VirtualGpu) -> wgpu::BindGroupLayout {
        shaders::common::AtlasTextures::layout(vgpu, wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX)
    }

    fn bind(
        &self,
        _vgpu: &VirtualGpu,
        pass: &mut SinglePass,
        bind_group: u32
    ) -> anyhow::Result<()> {
        if self.texture_arr.lock_ref().len() < 1 { bail!("No loaded textures") }
        if self.bind_group.is_null() { bail!("Missing bind group for atlas textures") }
        pass.bind_raw(bind_group, &self.bind_group.get_ref());
        Ok(())
    }
}

fn atlas_page_descriptor() -> magician_vgpu::TextureDescriptor {
    magician_vgpu::TextureDescriptor {
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        ..Default::default()
    }
}

#[system(std::i32::MIN)]
pub fn update_atlas_textures(
    graphics: Res<Graphics>,
    vault: Res<AtlasTextureVault>
) {
    let max_dim = graphics.device().limits().max_texture_dimension_2d;

    // get all unloaded keys
    let unloaded_keys: Vec<u64> = vault.unloaded_textures.iter()
        .map(|a| *a.key())
        .collect();

    for key in unloaded_keys.into_iter() {
        let Some((_hash, (handle, rgba, dimensions))) =
            vault.unloaded_textures.remove(&key) else { continue };

        let placement = vault.packer.lock_mut().place(dimensions.x, dimensions.y, max_dim);
        let (offset, grew) = match placement {
            Ok(placed) => placed,
            Err(err) => { eprintln!("Failed to place atlas texture: {err}"); continue; }
        };
        let (page_width, page_height) = { let packer = vault.packer.lock_ref(); (packer.width, packer.height) };

        if vault.page.is_null() {
            // first texture ever placed: create the page directly at its (possibly
            // already-grown) size, nothing to copy forward yet
            let page = StaticTexture::empty_texure(&*graphics, atlas_page_descriptor(), page_width, page_height);
            vault.page.set(page);
            vault.atlas_size_buffer.set(MutableBuffer::new(&*graphics, &Vec4::new(page_width as f32, page_height as f32, 0.0, 0.0), wgpu::BufferUsages::UNIFORM));
        } else if let Some(new_size) = grew {
            // grow the page, preserving existing content via a GPU-side copy so already
            // (permanently) placed textures don't need to be re-uploaded from the CPU
            let new_page = StaticTexture::empty_texure(&*graphics, atlas_page_descriptor(), new_size.x, new_size.y);
            {
                let old_page = vault.page.get_ref();
                let mut encoder = graphics.device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("atlas_grow_copy") });
                encoder.copy_texture_to_texture(
                    old_page.texture().as_image_copy(),
                    new_page.texture().as_image_copy(),
                    old_page.texture().size()
                );
                graphics.queue().submit(std::iter::once(encoder.finish()));
            }
            vault.page.set(new_page);
            vault.atlas_size_buffer.get_ref().write(&*graphics, &Vec4::new(new_size.x as f32, new_size.y as f32, 0.0, 0.0)).ok();
        }

        vault.page.get_ref().write_region(&*graphics, &rgba, offset, dimensions);

        let texture_idx = vault.texture_arr.lock_ref().len();
        vault.texture_arr.lock_mut().push(AtlasTextureAsset { offset_px: offset, size_px: dimensions });
        vault.texture_map.insert(key, (handle, texture_idx));
        vault.dirty.store(true, Ordering::Release);
    }

    // check if bind group needs rebuilding
    if (vault.dirty.swap(false, Ordering::AcqRel) || vault.bind_group.is_null()) && !vault.texture_arr.lock_mut().is_empty() {
        let sampler = graphics.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas_textures_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let layout = vault.bind_group_layout(&graphics);
        let bind_group = graphics.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas_textures_bg"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(vault.page.get_ref().view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: vault.atlas_size_buffer.get_ref().buffer(),
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
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

    fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if condition() { return true; }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn load_decodes_binary_content_asynchronously() {
        let world = World::default();
        let vault = AtlasTextureVault::default();
        let handle = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), BindlessArrayTextureType::PNG).unwrap();
        let hash = handle.inner.0;

        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&hash)));

        let staged = vault.unloaded_textures.get(&hash).unwrap();
        let expected_dimensions = image::load_from_memory_with_format(COBBLESTONE_PNG, image::ImageFormat::Png).unwrap().dimensions();
        assert_eq!(staged.2, expected_dimensions.into());
    }

    #[test]
    fn load_deduplicates_identical_content_once_staged() {
        let world = World::default();
        let vault = AtlasTextureVault::default();
        let first = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), BindlessArrayTextureType::PNG).unwrap();
        let hash = first.inner.0;
        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&hash)));

        let second = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), BindlessArrayTextureType::PNG).unwrap();

        assert_eq!(second.inner.0, hash);
        assert_eq!(vault.unloaded_textures.len(), 1);
    }

    #[test]
    fn load_before_decode_finishes_joins_in_flight_load() {
        let world = World::default();
        let vault = AtlasTextureVault::default();
        let first = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), BindlessArrayTextureType::PNG).unwrap();
        let second = vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), BindlessArrayTextureType::PNG).unwrap();

        assert_eq!(first.inner.0, second.inner.0);
        assert_eq!(vault.pending_loads.len() + vault.unloaded_textures.len(), 1);

        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&first.inner.0)));
        assert_eq!(vault.unloaded_textures.len(), 1);
    }

    #[test]
    fn concurrent_loads_of_identical_content_do_not_race() {
        let vault = Arc::new(AtlasTextureVault::default());
        let barrier = Arc::new(std::sync::Barrier::new(4));

        let handles: Vec<_> = (0..4).map(|_| {
            let vault = Arc::clone(&vault);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let world = World::default();
                barrier.wait();
                vault.load(&world, AssetContent::Binary(COBBLESTONE_PNG.into()), BindlessArrayTextureType::PNG).unwrap()
            })
        }).collect();

        let hashes: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap().inner.0).collect();
        assert!(hashes.windows(2).all(|w| w[0] == w[1]));

        assert!(wait_until(Duration::from_secs(5), || vault.unloaded_textures.contains_key(&hashes[0])));
        assert_eq!(vault.unloaded_textures.len(), 1);
    }

    #[test]
    fn place_fits_within_initial_page_without_growing() {
        let mut packer = ShelfPacker::new(1024, 1024);
        let (offset, grew) = packer.place(64, 32, 8192).unwrap();
        assert_eq!(offset, UVec2::new(0, 0));
        assert!(grew.is_none());
    }

    #[test]
    fn place_starts_a_new_shelf_when_the_current_row_is_full() {
        let mut packer = ShelfPacker::new(100, 100);
        let (first, _) = packer.place(80, 20, 1000).unwrap();
        let (second, _) = packer.place(80, 20, 1000).unwrap();
        assert_eq!(first, UVec2::new(0, 0));
        assert_eq!(second, UVec2::new(0, 20));
    }

    #[test]
    fn place_grows_the_page_when_a_texture_does_not_fit() {
        let mut packer = ShelfPacker::new(64, 64);
        let (offset, grew) = packer.place(100, 100, 8192).unwrap();
        assert_eq!(offset, UVec2::new(0, 0));
        assert_eq!(grew, Some(UVec2::new(128, 128)));
    }

    #[test]
    fn place_errors_when_a_texture_exceeds_the_max_page_dimension() {
        let mut packer = ShelfPacker::new(64, 64);
        assert!(packer.place(9000, 64, 8192).is_err());
    }

    #[test]
    fn place_errors_once_the_page_is_full_at_the_max_dimension() {
        // max_dim == current size, so no headroom to grow into for a rect that doesn't fit
        let mut packer = ShelfPacker::new(64, 64);
        packer.place(64, 64, 64).unwrap();
        assert!(packer.place(1, 1, 64).is_err());
    }

    #[test]
    fn placements_never_move_across_growth() {
        let mut packer = ShelfPacker::new(64, 64);
        let (first, _) = packer.place(32, 32, 8192).unwrap();
        // forces the page to grow past its current bounds
        let (_second, grew) = packer.place(64, 64, 8192).unwrap();
        assert!(grew.is_some());
        // the first placement's offset is still meaningful pixel-space coordinates in the
        // grown page -- growth never repacks/moves earlier placements
        assert_eq!(first, UVec2::new(0, 0));
    }
}
