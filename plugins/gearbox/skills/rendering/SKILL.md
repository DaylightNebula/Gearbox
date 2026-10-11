---
name: rendering
description: Use when drawing things with the gearbox crate - GearboxRenderPlugin, cameras and transforms, meshes (BasicMesh, ShapeBuilder primitives, OBJ files through MeshAssetVault), materials (BasicMaterial, SimpleTexturedMaterial, MaterialVault), loading textures with TextureVault, the clear color, drawing custom things in the main render pass, and writing custom Material or Mesh types. For the App, systems and plugins see the gearbox app skill.
---

# gearbox: rendering

All snippets assume the setup from the gearbox app skill, and these imports:

```rust
use gearbox::*;
use gearbox::anarchy::{Query, Res, World};
use gearbox::glam::{Quat, Vec3, Vec4};
```

## What gets drawn

Add `GearboxRenderPlugin` to the app. Each frame it draws every entity that has a
`Transform`, a `MaterialRef` and a `MeshRef`, as seen by the first entity found with a
`Transform` and a `Camera`.

```rust
fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(setup)   // GPU resources need Graphics, which only render systems are guaranteed to have
        .on_update(animate)
        .run()
}

fn setup(world: World, graphics: Res<Graphics>) {
    let mesh = ShapeBuilder::new()
        .cube(Vec3::ZERO, Quat::IDENTITY, 1.0, 1.0, 1.0)
        .build_mesh(&graphics)
        .unwrap();

    spawn(&world, vec![
        Box::new(Transform::identity()),
        Box::new(MaterialRef::new(BasicMaterial::new(Vec4::new(0.1, 0.8, 0.2, 1.0)))),
        Box::new(MeshRef::new(mesh)),
    ]);
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 0.0, 6.0), Quat::IDENTITY, Vec3::ONE)),
        Box::new(Camera::default()),
    ]);
}
```

- The clear color is the `ClearColor(Vec4)` resource, a dark blue-grey by default.
  Replace it with `add_resource` or change it through `ResMut<ClearColor>`.
- Entities whose mesh or material is still loading are skipped, and drawn once they're
  ready.

## Transform and Camera

`Transform` has public `translation: Vec3`, `rotation: Quat` and `scale: Vec3` fields:
- constructors: `Transform::identity()`, `Transform::new(t, r, s)`
- copy-with helpers: `with_translation`, `with_rotation`, `with_scale`
- in-place helpers: `translate_by`, `rotate_by`, `scale_by`

**Camera limitations (currently):**
- The camera always looks at the world origin from its `Transform::translation`, and
  ignores the transform's rotation.
- The projection is fixed at a 45° field of view with 0.1–10000 clip planes, so
  `Camera::new(fovy, znear, zfar)` values have no effect yet.
- Only the first camera found is used.

## Meshes

**Primitives:** `ShapeBuilder` combines any number of shapes into one `BasicMesh`. Each
shape takes an offset and rotation within the mesh:

```rust
let mesh = ShapeBuilder::new()
    .sphere(Vec3::ZERO, Quat::IDENTITY, 1.0)
    .cylinder(Vec3::new(0.0, -1.5, 0.0), Quat::IDENTITY, 0.25, 2.0)
    .build_mesh(&graphics)?;
```

The shapes are `cube(w, h, d)`, `sphere(r)`, `sphere_subdivided(r, n)`, `capsule(r, h)`,
`capsule_with_draw_instructions`, `cylinder(r, h)`, `cylinder_segmented(r, h, n)`,
`cone(r, h)` and `cone_segmented(r, h, n)`.

**Raw vertices:**

```rust
use gearbox::shaders::basic_vertex::VertexInput;
use gearbox::magician_vgpu::rust::{Vec2 as V2, Vec3 as V3};

let vertices = [
    VertexInput { position: V3::new(0.0, 0.5, 0.0), uvs: V2::new(0.5, 0.0), normals: V3::default() },
    VertexInput { position: V3::new(-0.5, -0.5, 0.0), uvs: V2::new(0.0, 1.0), normals: V3::default() },
    VertexInput { position: V3::new(0.5, -0.5, 0.0), uvs: V2::new(1.0, 1.0), normals: V3::default() },
];
let mesh = BasicMesh::new(&graphics, &vertices, &[0, 1, 2]);
```

Shader vertex types use `magician_vgpu::rust` vector types, not `glam`.

**OBJ files** load in the background through the `MeshAssetVault` resource. The handle
returned is itself a mesh:

```rust
fn setup(world: World, meshes: Res<MeshAssetVault>) {
    let mesh = meshes.load(&world, AssetContent::LocalPath("assets/tree.obj".into()), MeshLoadType::OBJ).unwrap();
    // MeshRef::new(mesh) on an entity
}
```

**Give every entity its own mesh.** A mesh keeps one instance buffer, rewritten each time
it is drawn, so entities sharing one mesh (for example the same OBJ handle) all draw at
the last entity's transform. Build or load one mesh per entity for now.

## Materials

| Material | Draws |
| --- | --- |
| `BasicMaterial::new(Vec4)` | a flat, unlit color |
| `SimpleTexturedMaterial::new(TextureHandle)` | a single texture, unlit |

Wrap a material directly with `MaterialRef::new(material)`. To share one between entities,
store it in the `MaterialVault` resource and use the handle as the material:

```rust
fn setup(world: World, materials: Res<MaterialVault>) {
    let handle = materials.allocate(1).unwrap();   // 1 is your key; reusing a key replaces it
    materials.store(&world, handle.clone(), Box::new(BasicMaterial::new(Vec4::ONE)));
    // MaterialRef::new(handle.clone()) on each entity
}
```

`allocate` and `store` come from the `LazyAssetVault` trait, so import it (it's included
in `gearbox::*`).

## Textures

Load textures through `TextureVault::current`. It picks the backend the GPU supports
(bindless texture arrays, or a texture atlas on WebGL and other limited GPUs):

```rust
fn setup(world: World, graphics: Res<Graphics>, materials: Res<MaterialVault>) {
    let vault = TextureVault::current(&world, &graphics).unwrap();
    let texture = vault.load(&world, AssetContent::LocalPath("assets/stone.png".into()), TextureType::PNG).unwrap();

    let material = materials.allocate(1).unwrap();
    materials.store(&world, material.clone(), Box::new(SimpleTexturedMaterial::new(texture)));
}
```

- `load` returns a handle at once and decodes the file in the background. Entities using
  it are skipped until it has uploaded, usually a frame or two.
- Loading the same content twice returns the same handle.
- `AssetContent` is one of `Binary(Box<[u8]>)`, `Content(String)`, `LocalPath(PathBuf)`
  or `Url(String)` (`Url` is native only).
- Native paths are relative to the working directory, so run examples from the crate root.
- `TextureType` is one of `PNG`, `JPG`, `TGA`, `BMP`, `DDS`, `FARBFELD`, `HDR`, `ICO`,
  `OPENEXR`, `PNM`, `QOI`, `TIFF` or `WEBP`. Give it explicitly: some formats can't be
  detected from their bytes.
- `vault.empty_texture()` is a 1x1 white texture, available after the first frame or two.

## Drawing in the main render pass

To draw something that isn't a mesh-plus-material entity, add a system to the
`MainRenderPassSchedule` resource. Its systems run inside the main pass each frame, and
take `ResMut<MainPassPassthrough>` to reach the pass (a `magician_vgpu::SinglePass`):

```rust
struct OverlayPlugin;
impl Plugin for OverlayPlugin {
    fn build(self, app: App) -> App {
        app.world().resource_mut::<MainRenderPassSchedule>()
            .expect("add GearboxRenderPlugin first")
            .on_update(draw_overlay)
            .unwrap();
        app
    }
}

fn draw_overlay(graphics: Res<Graphics>, mut pass: gearbox::anarchy::ResMut<MainPassPassthrough>) {
    // pass.use_pipeline(...), pass.bind(...), pass.draw(...)
}
```

- Add `GearboxRenderPlugin` before such a plugin, so the schedule exists.
- These systems must not take `Res<MainRenderPassSchedule>` or
  `ResMut<MainRenderPassSchedule>`: it is locked while they run, so they deadlock.
- `on_startup` on the schedule runs once before its first update.

## Custom materials and meshes

Implement `Material` (or `Mesh`) and wrap the value in `MaterialRef::new` (or
`MeshRef::new`). Both traits need `Send + 'static`.

- **Pipelines.** `create_pipeline` returns a `magician_vgpu::PipelineBuilder`. The
  material's builder is merged with the mesh's, and the result is cached per
  (mesh type, material type) pair, keyed by `id()`.
- **`Material::prep_render_entity`** binds the material's groups before each entity is
  drawn:
  - Group 0 is the material.
  - Group 1 is textures, if any.
  - Group 2 is the camera (`camera.bindable()`).
- **`Mesh::draw`** binds vertex and instance buffers and issues the draw. It gets the
  entity's `EntityID`, so it can read other components with
  `world.get::<&Transform>(entity)` (import `gearbox::anarchy::WorldImpl` for `get`).
- **Shaders** are written in Rust in the `shaders` crate and compiled to WGSL by
  `shader_magician`. Each `#[shader]` function generates a `SHADER_<name>` source
  constant. Follow `shaders/src/basic_material.rs` and `src/material/basic.rs` as the
  pattern.
