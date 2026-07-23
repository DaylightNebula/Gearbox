use std::{fmt::Debug, marker::PhantomData, sync::Arc};

use anarchy::macros::Getters;
use derive_more::{Deref, DerefMut};

use crate::Asset;

/// A cheap-to-clone, reference-counted handle to an asset loaded in its
/// [`AssetVault`](crate::AssetVault).
///
/// Cloning a handle increments its reference count; dropping the last clone that
/// crosses [`Asset::unload_threshold`] triggers [`Asset::unload`] for the underlying
/// asset. Use [`AssetVault::get`](crate::AssetVault::get) to access the loaded asset
/// data itself.
#[derive(Deref, DerefMut)]
pub struct Handle<T: Asset>(pub(crate) Arc<HandleInner<T>>);

impl <T: Asset> Handle<T> {
    pub fn new(inner: T::HandleTracker) -> Self {
        Self(Arc::new(HandleInner { inner, _phantom: PhantomData::default() }))
    }
}

impl <T: Asset> Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("ref_count", &Arc::strong_count(&self.0))
            .finish()
    }
}

impl <T: Asset> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

unsafe impl <T: Asset> Send for Handle<T> {}
unsafe impl <T: Asset> Sync for Handle<T> {}

/// The data tracked by a [`Handle`]: the vault-defined [`Asset::HandleTracker`] used
/// to locate and unload the asset, plus a marker tying the handle to its asset type.
#[derive(Getters)]
pub struct HandleInner<T: Asset> {
    pub(crate) inner: T::HandleTracker,
    pub(crate) _phantom: PhantomData<T>
}

impl <T: Asset> Drop for Handle<T> {
    fn drop(&mut self) {
        let remaining = Arc::strong_count(&self.0);
        if remaining <= T::unload_threshold() {
            T::unload(&self.0.inner);
        }
    }
}

impl <T: Asset<HandleTracker = H, Vault = V>, H: PartialEq, V> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner)
    }
}
