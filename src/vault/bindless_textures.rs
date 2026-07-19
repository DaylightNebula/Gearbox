//! An [`AssetVault`] that loads textures into a single bindless texture array bind
//! group, letting a shader index into many textures via one binding.

use std::{hash::{Hash, Hasher}, marker::PhantomData, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use ahash::AHasher;
use anarchy::{anyhow, macros::{Getters, Resource}};
use anarchy::anyhow::bail;
use derive_more::{Deref, DerefMut};
use image::{GenericImageView, ImageBuffer, Rgba};
use magician_vgpu::{SinglePass, StaticTexture, Texture, VirtualGpu, glam::UVec2};
use mutual::{CowData, DashMap, Ref, RefGuard, RelaxedMutex, SharedData};

use crate::{Asset, AssetContent, AssetVault, BindableAssetVault, Handle, HandleInner};

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

/// The [`AssetVault`] resource for [`BindlessArrayTextureAsset`]s.
///
/// A cheaply-clonable handle to the shared vault state; register one instance of
/// this as an ECS [`Resource`](anarchy::Resource) and load textures through it via
/// [`AssetVault::load`].
#[derive(Resource, Default, Deref, DerefMut)]
pub struct BindlessArrayTextureVault(Arc<BindlessArrayTextureVaultInner>);

/// Shared state backing a [`BindlessArrayTextureVault`].
///
/// Newly loaded images are staged in `unloaded_textures` (decoded, but not yet on
/// the GPU) and are only uploaded into `texture_arr` and moved into `texture_map`
/// the next time [`BindableAssetVault::bind`] runs, at which point the bind group
/// is rebuilt if `dirty`.
pub struct BindlessArrayTextureVaultInner {
    texture_map: DashMap<u64, (Handle<BindlessArrayTextureAsset>, usize)>,
    texture_arr: RelaxedMutex<Vec<BindlessArrayTextureAsset>>,
    unloaded_textures: DashMap<u64, (Handle<BindlessArrayTextureAsset>, ImageBuffer<Rgba<u8>, Vec<u8>>, UVec2)>,
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
            bind_group: CowData::null(),
            dirty: AtomicBool::new(false)
        }
    }
}

impl AssetVault for BindlessArrayTextureVault {
    type Asset = BindlessArrayTextureAsset;

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

    fn load(&self, content: AssetContent) -> anyhow::Result<Handle<Self::Asset>> {
        // compute content hash
        let mut hasher = AHasher::default();
        content.hash(&mut hasher);
        let hash = hasher.finish();
        
        // get previous handle
        if let Some(value) = self.texture_map.get(&hash) { return Ok(value.0.clone()) };
        if let Some(value) = self.unloaded_textures.get(&hash) { return Ok(value.0.clone()) };

        // load rgba content
        let AssetContent::Binary(bytes) = content else { bail!("Only binary images supported right now!") };
        let img = image::load_from_memory(&bytes)?;
        let dimensions = img.dimensions();
        let rgba = img.to_rgba8();

        // create and save new handle
        let handle = HandleInner { inner: (hash, Arc::clone(&self.0)), _phantom: PhantomData::default() };
        let handle = Handle(Arc::new(handle));
        self.unloaded_textures.insert(hash, (handle.clone(), rgba, dimensions.into()));

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
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: std::num::NonZeroU32::new(128),
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None
                        }
                    ]
                }
            );
    }

    fn bind(&self, vgpu: &VirtualGpu, pass: &mut SinglePass, bind_group: u32) {
        // get all unloaded keys
        let unloaded_keys: Vec<u64> = self.unloaded_textures.iter()
            .map(|a| *a.key())
            .collect();

        // loop through all unloaded keys, then mark dirty
        if !unloaded_keys.is_empty() {
            for key in unloaded_keys.into_iter() {
                // remove from unloaded cache
                let Some((_hash, (handle, rgba, dimensions))) = 
                    self.unloaded_textures.remove(&key) else { continue };

                // create new texture
                let texture = StaticTexture::from_raw(
                    vgpu, 
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
                let texture_idx = self.texture_arr.lock_ref().len();
                self.texture_arr.lock_mut().push(BindlessArrayTextureAsset { texture, texture_idx });
                self.texture_map.insert(key, (handle, texture_idx));
            }
            self.dirty.store(true, Ordering::Release);
        }

        // check if bind group needs rebuilding
        if self.dirty.swap(false, Ordering::AcqRel) || self.bind_group.is_null() {
            let binding = self.texture_arr.lock_ref();
            let views = binding
                .iter()
                .map(|a| a.texture.view())
                .collect::<Vec<_>>();

            let bgl = self.bind_group_layout(vgpu);

            let sampler = vgpu.device().create_sampler(&wgpu::SamplerDescriptor {
                label: Some("binding_array_textures_sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });

            let bind_group = vgpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
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

            self.bind_group.set(bind_group);
        }

        pass.bind_raw(bind_group, &self.bind_group.get_ref());
    }
}
