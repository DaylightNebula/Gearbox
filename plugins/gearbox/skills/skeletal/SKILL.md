---
name: skeletal
description: Use when loading or animating skinned 3D models with gearbox's skeletal module (the `skeletal` feature) - SkeletalMeshPlugin, loading glTF/GLB or FBX files through SkeletalMeshVault, the mesh, material and animation handles it returns, the Animator component (load_animations, play, stop), hiding sub-meshes by label with invisible_bones, and what works on the web. For cameras, transforms and plain meshes see the gearbox rendering skill.
---

# gearbox: skeletal meshes and animation

`gearbox::skeletal` loads skinned models (mesh, skeleton, skin and animations) from glTF/GLB
or FBX into one engine format, and draws them with GPU vertex skinning (up to 4 bones per
vertex). It's behind the `skeletal` feature:

```toml
[dependencies]
gearbox = { path = "../gearbox", features = ["skeletal"] }
```

Snippets assume the setup from the gearbox app skill and these imports:

```rust
use gearbox::anarchy::{Query, Res, World};
use gearbox::glam::{Quat, Vec3};
use gearbox::skeletal::{Animator, SkeletalMeshHandle, SkeletalMeshLoadType, SkeletalMeshPlugin, SkeletalMeshVault};
use gearbox::{App, AssetContent, Camera, GearboxRenderPlugin, LoadableAssetVault, MaterialRef, MeshRef, Transform, anyhow, spawn};
```

## Loading and drawing a model

```rust
fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(SkeletalMeshPlugin)   // SkeletalMeshVault, AnimationVault and the upload system
        .on_render_startup(setup)
        .run()
}

fn setup(world: World, meshes: Res<SkeletalMeshVault>) {
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 10.0, 30.0), Quat::from_rotation_x(-0.5), Vec3::ONE)),
        Box::new(Camera::default()),
    ]);

    let content = AssetContent::LocalPath("./assets/Barbarian.glb".into());
    let (mesh, material, animations) = match meshes.load(&world, content, SkeletalMeshLoadType::GLTF) {
        Ok(loaded) => loaded,
        Err(err) => return log::error!("Failed to load model: {err:#}"),
    };

    let mut animator = Animator::empty();
    animator.load_animations(animations);
    animator.play("Idle", true);

    spawn(&world, vec![
        Box::new(Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE)),
        Box::new(MaterialRef::new(material)),
        Box::new(animator),
        Box::new(MeshRef::new(mesh)),
    ]);
}
```

- `load` comes from the `LoadableAssetVault` trait, so import it. It returns right away with
  `(SkeletalMeshHandle, Handle<Box<dyn Material>>, Handle<AnimationSet>)`. Parsing runs in
  the background and the GPU upload happens in a later render `Start` pass; until then the
  entity just isn't drawn.
- Loads are deduplicated by content: loading the same path twice gives the same handles.
- The material is a `SimpleTexturedMaterial` using the first material's albedo texture, or a
  white `BasicMaterial` when there is none. Use your own `MaterialRef` to override it.
- A model animates at most `MAX_SKELETAL_BONES` (128) skin joints and 128 nodes, the most
  that fits a uniform buffer on WebGL2. Loading a bigger one logs a warning, and the extra
  bones keep their bind pose.
- An entity needs `Transform`, `MaterialRef`, `MeshRef(SkeletalMeshHandle)` and an `Animator`
  to be drawn. Use `Animator::empty()` for a model with no animations.

## Animator

- `load_animations(handle)` queues an `AnimationSet`. It's added on the first frame the set
  and the mesh are loaded. Animations from another file with the same skeleton can be loaded
  onto the same animator: load that file too and pass its third handle.
- `play(name, looping)` starts an animation by name from the beginning. A name that doesn't
  exist (or isn't loaded yet) plays nothing. A non-looping animation stops itself at its end.
- `stop()` stops playback, leaving the model in its bind pose. `animations()` lists what's loaded and `state()` what's
  playing.
- Playback advances by `RenderDeltaTime` each frame (in the render `Start` pin), so it
  follows the app's clock, and headless fixed-step screenshots are repeatable.

## Hiding sub-meshes

Each sub-mesh has a `label` (its node name). Hide one by inserting its label into the handle's
`invisible_bones()` set, for example to swap gear on a character:

```rust
fn hide_helmet(mut models: Query<&'static MeshRef>) {
    while let Some((_id, model)) = models.next().unwrap() {
        if let Some(handle) = model.0.as_any().downcast_ref::<SkeletalMeshHandle>() {
            handle.invisible_bones().insert("Helmet".to_string());
        }
    }
}
```

## Formats and the web

| Load type | Files | Web |
| --- | --- | --- |
| `SkeletalMeshLoadType::GLTF` | `.gltf` (embedded or base64 buffers), `.glb` | yes |
| `SkeletalMeshLoadType::FBX` | `.fbx` | no, the variant doesn't exist on wasm32 (ufbx is native only) |

Unskinned meshes attached to a bone (weapons, props) follow that bone rigidly. Runnable
examples: `skeletal_gltf`, `skeletal_fbx` and `skeletal_viewer` (`cargo run --example
skeletal_viewer --features skeletal -- path/to/model.glb`, an egui window for playing
animations and toggling sub-meshes).
