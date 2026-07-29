//! A thin facade unifying [`BindlessArrayTextureVault`] and [`AtlasTextureVault`] behind
//! one API, so app code never needs to branch on which backend the current GPU supports.
//!
//! This is deliberately *not* a pre-selected `Resource`: `anarchy`'s render-startup
//! schedule has no priority ordering (it's a LIFO stack -- the most recently registered
//! startup system runs first), so a plugin-registered "pick the backend" system can never
//! be guaranteed to run before an app's own `on_render_startup` system that needs the
//! result. [`TextureVault::current`] sidesteps this by computing the answer fresh (a cheap
//! `Arc` clone) from `Graphics`, which -- unlike a plugin-populated resource -- is always
//! already present by the time any render-startup system runs.

use anarchy::{World, anyhow::{self, Context}};
use cell::Graphics;

use crate::{AssetContent, AtlasTextureAsset, AtlasTextureVault, BindlessArrayTextureAsset, BindlessArrayTextureType, BindlessArrayTextureVault, Handle, LoadableAssetVault};

/// A handle to a texture loaded through a [`TextureVault`], regardless of which concrete
/// backend loaded it.
#[derive(Clone)]
pub enum TextureHandle {
    Bindless(Handle<BindlessArrayTextureAsset>),
    Atlas(Handle<AtlasTextureAsset>)
}

/// Dispatches texture loading to whichever backend `graphics`'s GPU supports. App code
/// should load textures through this rather than a concrete vault, so it works unchanged
/// regardless of which backend is active.
#[derive(Clone)]
pub enum TextureVault {
    Bindless(BindlessArrayTextureVault),
    Atlas(AtlasTextureVault)
}

impl TextureVault {
    /// Looks up the vault backend matching what `graphics`'s GPU supports. Both
    /// `BindlessArrayTextureVault` and `AtlasTextureVault` are always registered by
    /// [`crate::GearboxRenderPlugin`] regardless of GPU support, so this only fails if
    /// the plugin wasn't added at all.
    pub fn current(world: &World, graphics: &Graphics) -> anyhow::Result<Self> {
        Ok(if *graphics.supports_bindless_arrays() {
            Self::Bindless(world.get_resource_ref::<BindlessArrayTextureVault>().context("Missing BindlessArrayTextureVault")?.clone())
        } else {
            Self::Atlas(world.get_resource_ref::<AtlasTextureVault>().context("Missing AtlasTextureVault")?.clone())
        })
    }

    /// Loads (or looks up an existing handle for) the texture described by `content`,
    /// through whichever backend is active.
    pub fn load(&self, world: &World, content: AssetContent, ty: BindlessArrayTextureType) -> anyhow::Result<TextureHandle> {
        match self {
            Self::Bindless(vault) => vault.load(world, content, ty).map(TextureHandle::Bindless),
            Self::Atlas(vault) => vault.load(world, content, ty).map(TextureHandle::Atlas),
        }
    }
}
