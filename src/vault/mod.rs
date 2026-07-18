//! `vault` is a generic asset-loading and lifecycle system: [`Asset`] defines a loadable
//! asset type, [`AssetVault`] stores and reference-counts loaded instances of that type
//! behind cheap-to-clone [`Handle`]s, and [`BindableAssetVault`] extends a vault with the
//! ability to bind its assets to a GPU render pass. See [`bindless_textures`] for a concrete
//! example vault implementation.

use anarchy::Resource;
use magician_vgpu::{SinglePass, VirtualGpu};
use mutual::Ref;

pub mod handles;
pub mod bindless_textures;

pub use handles::*;
pub use bindless_textures::*;

/// A type of asset that can be loaded into and unloaded from an [`AssetVault`].
///
/// Implementors define how many outstanding [`Handle`]s are needed to keep the
/// asset alive (`unload_threshold`) and what happens when that threshold is
/// crossed (`unload`).
pub trait Asset: 'static {
    type Vault: AssetVault;
    type HandleTracker: 'static;

    fn unload_threshold() -> usize;
    fn unload(tracker: &Self::HandleTracker);
}

/// The raw source data used to load an [`Asset`], as passed to [`AssetVault::load`].
///
/// Not every vault is required to support every variant; unsupported variants
/// should be rejected by returning an `Err` from `load`.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetContent {
    /// Raw, already-decoded binary content (e.g. the bytes of an image file).
    Binary(Box<[u8]>),
    /// Textual content, such as inline source for a text-based asset format.
    Content(String),
    /// A path to the asset's content on the local filesystem.
    LocalPath(String)
}

/// An ECS [`Resource`] that owns and manages the lifecycle of a single [`Asset`] type.
///
/// A vault deduplicates loads of identical content and hands out reference-counted
/// [`Handle`]s; assets are unloaded automatically once their handle count drops to
/// or below [`Asset::unload_threshold`].
pub trait AssetVault: Resource + 'static {
    type Asset: Asset;

    /// Returns a reference to the loaded asset data for `handle`, if it is currently loaded.
    fn get(&self, handle: &Handle<Self::Asset>) -> Option<Ref<Self::Asset>>;

    /// Loads (or looks up an existing handle for) the asset described by `content`.
    fn load(&self, content: AssetContent) -> anyhow::Result<Handle<Self::Asset>>;
}

/// An [`AssetVault`] whose loaded assets can be bound to a GPU render pass.
pub trait BindableAssetVault: AssetVault {
    /// Builds the bind group layout expected by [`BindableAssetVault::bind`].
    fn bind_group_layout(&self, vgpu: &VirtualGpu) -> wgpu::BindGroupLayout;

    /// Binds this vault's assets to `bind_group` on `pass`, uploading any
    /// pending assets to the GPU first if needed.
    fn bind(&self, vgpu: &VirtualGpu, pass: &mut SinglePass, bind_group: u32);
}
