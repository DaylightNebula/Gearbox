# Gearbox

A simple ECS based renderer for `Cell` and `Anarchy`, built on top of `wgpu` via
`magician-vgpu`.

> **Status: work in progress.** This project is early and little of the API is
> set in stone. Expect breaking changes.

## What it does

`gearbox` provides:
- `GearboxRenderPlugin` — a `cell::App` plugin that adds a main render pass and
  draws every entity carrying a `Transform`, `MaterialRef`, and `MeshRef`,
  as seen through the first entity found with a `Camera` component.
- `Transform` / `Camera` components.
- `BasicMesh`, plus `ShapeBuilder` for generating primitive meshes (cube,
  sphere, cylinder, cone, capsule).
- `BasicMaterial` (flat color) and `SimpleTexturedMaterial` (single albedo
  texture) materials.
- `vault` — a generic, not-yet-integrated asset-loading system for
  reference-counted GPU resources such as textures.

This crate is part of a small workspace of sibling crates (`anarchy`, `cell`,
`mutual`, `shader_magician/magician-vgpu`) referenced by relative path in
`Cargo.toml`, so it is not currently usable as a standalone dependency outside
that workspace layout.

## Usage

### A rotating triangle

```rust
use anarchy::{EntityBuilder, Query, Res, WorldDatabase, macros::system};
use cell::{App, Graphics};
use gearbox::{BasicMaterial, BasicMesh, Camera, GearboxRenderPlugin, MaterialRef, MeshRef, Transform};
use magician_vgpu::{glam::{self, Quat}, rust::{Vec2, Vec3}};
use shaders::basic_vertex;

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

#[system]
fn setup(graphics: Res<Graphics>) {
    let vertices: [basic_vertex::VertexInput; 3] = [
        basic_vertex::VertexInput { position: Vec3::new(0.0, 0.5, 0.0), uvs: Vec2::new(0.5, 0.0), normals: Vec3::default() },
        basic_vertex::VertexInput { position: Vec3::new(-0.5, -0.5, 0.0), uvs: Vec2::new(0.0, 1.0), normals: Vec3::default() },
        basic_vertex::VertexInput { position: Vec3::new(0.5, -0.5, 0.0), uvs: Vec2::new(1.0, 1.0), normals: Vec3::default() },
    ];

    let mesh = BasicMesh::new(&*graphics, &vertices, &[0, 1, 2]);

    // renderable entity: needs a Transform + MaterialRef + MeshRef
    world.insert(
        EntityBuilder::default()
            .add(Transform::identity())
            .add(MaterialRef::new(BasicMaterial::new(glam::Vec4::new(0.1, 0.8, 0.2, 1.0))))
            .add(MeshRef::new(mesh))
            .build()
    );

    // camera entity: needs a Transform + Camera
    world.insert(
        EntityBuilder::default()
            .add(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .add(Camera::default())
            .build()
    );
}

#[system]
fn update(query: Query<(&MeshRef, &mut Transform)>) {
    for (_mesh, mut transform) in query.as_iter() {
        transform.rotate_by(Quat::from_euler(glam::EulerRot::XYZ, 0.01, 0.01, 0.01));
    }
}
```

See `examples/basic.rs` for the full runnable version.

### Built-in shapes and a textured material

`ShapeBuilder` generates a `BasicMesh` from one or more primitive shapes:

```rust
use gearbox::{MaterialRef, MeshRef, ShapeBuilder, SimpleTexturedMaterial, Transform, glam::*};

let mesh = ShapeBuilder::new()
    .cube(Vec3::ZERO, Quat::IDENTITY, 1.0, 1.0, 1.0)
    .build_mesh(&graphics)?;

world.insert(
    EntityBuilder::default()
        .add(Transform::new(Vec3::new(2.5, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE))
        .add(MaterialRef::new(SimpleTexturedMaterial::from_png(&graphics, include_bytes!("./cobblestone.png"))?))
        .add(MeshRef::new(mesh))
        .build()
);
```

`ShapeBuilder` also supports `sphere`, `sphere_subdivided`, `capsule`,
`capsule_with_draw_instructions`, `cylinder`, `cylinder_segmented`, `cone`, and
`cone_segmented`, and can combine multiple shapes into a single mesh by
chaining calls before `build_mesh`.

See `examples/shapes.rs` and `examples/textured.rs` for full runnable versions.

## Running the examples

```sh
cargo run --example basic
cargo run --example shapes
cargo run --example textured
```
