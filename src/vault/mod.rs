//! `vault` is a generic asset-loading and lifecycle system: [`Asset`] defines a loadable
//! asset type, [`AssetVault`] stores and reference-counts loaded instances of that type
//! behind cheap-to-clone [`Handle`]s, and [`BindableAssetVault`] extends a vault with the
//! ability to bind its assets to a GPU render pass. See [`bindless_textures`] for a concrete
//! example vault implementation.

use anarchy::{Resource, anyhow};
use magician_vgpu::{SinglePass, VirtualGpu};

pub mod bindless_textures;
pub mod handles;
pub mod mesh;

pub use bindless_textures::*;
pub use handles::*;
pub use mesh::*;

/// A type of asset that can be loaded into and unloaded from an [`AssetVault`].
///
/// Implementors define how many outstanding [`Handle`]s are needed to keep the
/// asset alive (`unload_threshold`) and what happens when that threshold is
/// crossed (`unload`).
pub trait Asset: 'static {
    type Vault: AssetVault;
    type HandleTracker: Send + Sync + 'static;

    fn unload_threshold() -> usize;
    fn unload(tracker: &Self::HandleTracker);
}

/// The raw source data used to load an [`Asset`], as passed to [`AssetVault::load`].
///
/// Not every vault is required to support every variant; unsupported variants
/// should be rejected by returning an `Err` from `load`.
/// 
/// Beware when using raw `Binary` and `Content` variants as they will be hashed so
/// large inpust are not suggested unless absolutely necessary.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetContent {
    /// Raw, already-decoded binary content (e.g. the bytes of an image file).
    Binary(Box<[u8]>),
    /// Textual content, such as inline source for a text-based asset format.
    Content(String),
    /// A path to the asset's content on the local filesystem.
    LocalPath(String),
    /// A URL to fetch the asset's content from over the network.
    Url(String)
}

impl AssetContent {
    /// Resolves this content into raw bytes through one uniform async pipeline.
    ///
    /// `Binary`/`Content` resolve immediately with the data they already hold;
    /// `LocalPath`/`Url` perform blocking file/network I/O. Callers should drive
    /// this future off the calling thread (e.g. [`anarchy::Scheduler::run_async`])
    /// since the I/O variants block whichever thread polls them.
    pub async fn into_bytes(self) -> anyhow::Result<Box<[u8]>> {
        Ok(match self {
            AssetContent::Binary(bytes) => bytes,
            AssetContent::Content(text) => text.into_bytes().into_boxed_slice(),
            AssetContent::LocalPath(path) => std::fs::read(path)?.into_boxed_slice(),
            AssetContent::Url(url) => ureq::get(&url).call()?.body_mut().read_to_vec()?.into_boxed_slice(),
        })
    }
}

/// An ECS [`Resource`] that owns and manages the lifecycle of a single [`Asset`] type.
///
/// A vault deduplicates loads of identical content and hands out reference-counted
/// [`Handle`]s; assets are unloaded automatically once their handle count drops to
/// or below [`Asset::unload_threshold`].
pub trait AssetVault: Resource + 'static {
    type Asset: Asset;
    type LoadType;
    type LoadResult;
    type Lookup;
    type LookupResult;

    /// Returns a reference to the loaded asset data for `handle`, if it is currently loaded.
    fn get(&self, handle: &Self::Lookup) -> Option<Self::LookupResult>;

    /// Loads (or looks up an existing handle for) the asset described by `content`.
    fn load(&self, content: AssetContent, ty: Self::LoadType) -> anyhow::Result<Self::LoadResult>;
}

/// An [`AssetVault`] whose loaded assets can be bound to a GPU render pass.
pub trait BindableAssetVault: AssetVault {
    /// Builds the bind group layout expected by [`BindableAssetVault::bind`].
    fn bind_group_layout(&self, vgpu: &VirtualGpu) -> wgpu::BindGroupLayout;

    /// Binds this vault's assets to `bind_group` on `pass`, uploading any
    /// pending assets to the GPU first if needed.
    fn bind(&self, vgpu: &VirtualGpu, pass: &mut SinglePass, bind_group: u32) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn into_bytes_binary_returns_bytes_unchanged() {
        let bytes = pollster::block_on(AssetContent::Binary(Box::new([1, 2, 3])).into_bytes()).unwrap();
        assert_eq!(&*bytes, &[1, 2, 3]);
    }

    #[test]
    fn into_bytes_content_returns_utf8_bytes() {
        let bytes = pollster::block_on(AssetContent::Content("hello".to_string()).into_bytes()).unwrap();
        assert_eq!(&*bytes, b"hello");
    }

    #[test]
    fn into_bytes_local_path_reads_file_contents() {
        let path = std::env::temp_dir().join(format!("gearbox_asset_content_test_{:?}", std::thread::current().id()));
        std::fs::write(&path, b"from disk").unwrap();

        let bytes = pollster::block_on(AssetContent::LocalPath(path.to_string_lossy().into_owned()).into_bytes()).unwrap();

        std::fs::remove_file(&path).unwrap();
        assert_eq!(&*bytes, b"from disk");
    }

    #[test]
    fn into_bytes_url_fetches_content_over_http() {
        // Minimal loopback HTTP server so this test needs neither network access nor mocking crates.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = b"from the network";

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });

        let bytes = pollster::block_on(AssetContent::Url(format!("http://{addr}/asset")).into_bytes()).unwrap();

        server.join().unwrap();
        assert_eq!(&*bytes, body);
    }
}
