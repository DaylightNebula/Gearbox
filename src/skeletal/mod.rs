use crate::{App, Plugin};

pub mod anim;
pub mod data;
pub mod loader;
pub mod mesh;
pub mod model;
pub mod vault;

pub use anim::*;
pub use data::*;
pub use loader::*;
pub use mesh::*;
pub use model::*;
pub use vault::*;

/// Adds skinned-mesh loading (glTF/FBX) and rendering to a `crate::App`.
/// Registers the `SkeletalMeshVault` resource; see `vault::SkeletalMeshVaultPlugin`.
pub struct SkeletalMeshPlugin;
impl Plugin for SkeletalMeshPlugin {
    fn build(self, app: App) -> App {
        app.add_plugin(SkeletalMeshVaultPlugin)
            .add_resource(AnimationVault::default())
    }
}
