# Gearbox

A simple ECS based renderer and app framework for `Anarchy`, built on top of `winit` and
`wgpu` via `magician-vgpu`. It includes what used to be the separate `Cell` crate.

> **Status: work in progress.** This project is early and little of the API is
> set in stone. Expect breaking changes.

## What it does

`gearbox` provides:
- `App` — a builder that owns an `anarchy` `World` and the `winit` window/event loop. It
  runs a primary update graph at a fixed tick rate (`UpdateRate`), independent of rendering
  and on its own threads natively, and a render graph once per frame. Window and device
  events are sent into the ECS as `WindowEvent` and `DeviceEvent`, `UpdateDeltaTime` and
  `RenderDeltaTime` are kept up to date, and `spawn` hands out entity ids.
- `Plugin`s for packaging and sharing functionality, and `EguiPlugin` for `egui` debug UI.
- `GearboxRenderPlugin` — an `App` plugin that adds a main render pass and
  draws every entity carrying a `Transform`, `MaterialRef`, and `MeshRef`,
  as seen through the first entity found with a `Camera` component.
- `Transform` / `Camera` components.
- `BasicMesh`, plus `ShapeBuilder` for generating primitive meshes (cube,
  sphere, cylinder, cone, capsule).
- `BasicMaterial` (flat color) and `SimpleTexturedMaterial` (single albedo
  texture) materials.
- `vault` — a generic, not-yet-integrated asset-loading system for
  reference-counted GPU resources such as textures.

This crate is part of a small workspace of sibling crates (`anarchy`,
`shader_magician/magician-vgpu`) referenced by relative path in `Cargo.toml`, so it is
not currently usable as a standalone dependency outside that workspace layout.

`gearbox` re-exports `anarchy`, `anyhow`, `magician_vgpu`, `glam` and `shaders`, so an app
only needs `gearbox` itself. The exception is `#[derive(Component)]` and
`#[derive(Resource)]`: their generated code names `::anarchy`, so an app that uses them
must also depend on `anarchy` directly.

## Features

| Feature | Enables |
|---|---|
| `auto-egui` | Automatically adds `EguiPlugin` when `App::new()` is called. |
| `dbg-schedules` | Implies `auto-egui`. Adds an egui window showing the update and render FPS. |

## Usage

### A rotating triangle

```rust
use gearbox::anarchy::{Query, Res, World};
use gearbox::{App, BasicMaterial, BasicMesh, Camera, GearboxRenderPlugin, Graphics, MaterialRef, MeshRef, Transform, spawn};
use gearbox::magician_vgpu::{glam::{self, Quat}, rust::{Vec2, Vec3}};
use gearbox::shaders::basic_vertex;

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

fn setup(world: World, graphics: Res<Graphics>) {
    let vertices: [basic_vertex::VertexInput; 3] = [
        basic_vertex::VertexInput { position: Vec3::new(0.0, 0.5, 0.0), uvs: Vec2::new(0.5, 0.0), normals: Vec3::default() },
        basic_vertex::VertexInput { position: Vec3::new(-0.5, -0.5, 0.0), uvs: Vec2::new(0.0, 1.0), normals: Vec3::default() },
        basic_vertex::VertexInput { position: Vec3::new(0.5, -0.5, 0.0), uvs: Vec2::new(1.0, 1.0), normals: Vec3::default() },
    ];

    let mesh = BasicMesh::new(&*graphics, &vertices, &[0, 1, 2]);

    // renderable entity: needs a Transform + MaterialRef + MeshRef
    spawn(&world, vec![
        Box::new(Transform::identity()),
        Box::new(MaterialRef::new(BasicMaterial::new(glam::Vec4::new(0.1, 0.8, 0.2, 1.0)))),
        Box::new(MeshRef::new(mesh)),
    ]);

    // camera entity: needs a Transform + Camera
    spawn(&world, vec![
        Box::new(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE)),
        Box::new(Camera::default()),
    ]);
}

fn update(mut query: Query<(&'static MeshRef, &'static mut Transform)>) {
    while let Some((_id, (_mesh, mut transform))) = query.next().unwrap() {
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

spawn(&world, vec![
    Box::new(Transform::new(Vec3::new(2.5, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE)),
    Box::new(MaterialRef::new(SimpleTexturedMaterial::from_png(&graphics, include_bytes!("./cobblestone.png"))?)),
    Box::new(MeshRef::new(mesh)),
]);
```

`ShapeBuilder` also supports `sphere`, `sphere_subdivided`, `capsule`,
`capsule_with_draw_instructions`, `cylinder`, `cylinder_segmented`, `cone`, and
`cone_segmented`, and can combine multiple shapes into a single mesh by
chaining calls before `build_mesh`.

See `examples/shapes.rs` and `examples/textured.rs` for full runnable versions.

### An egui debug window

```rust
use anarchy::{Res, ResMut, Resource};
use gearbox::{App, EguiCtx, EguiPlugin, egui::egui};

#[derive(Debug, Resource)]
pub struct TestData {
    speed: f32
}

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_plugin(EguiPlugin)
        .on_render_update(on_render_update)
        .add_resource(TestData { speed: 0.5 })
        .run()
}

fn on_render_update(egui: Res<EguiCtx>, mut test_data: ResMut<TestData>) {
    egui::Window::new("Debug").show(&egui.context, |ui| {
        ui.label("Hello from egui!");
        ui.add(egui::Slider::new(&mut test_data.speed, 0.0..=1.0).text("Speed"));
    });
}
```

### Systems and plugins

Systems are plain functions whose parameters are `anarchy` system params (`World`, `Query`,
`Res`, `ResMut`, `Event`, ...). Add them with `on_startup`, `on_update`, `on_render_startup`
and `on_render_update`, and use the `_with` variants (`on_update_with`,
`on_render_update_with`, ...) to order them with `before`, `after` or `pin`. The egui pass is
open for every render system in the default `Normal` pin.

```rust
use gearbox::{App, Plugin};

struct MyPlugin;

impl Plugin for MyPlugin {
    fn build(self, app: App) -> App {
        app.on_startup(my_startup_system)
            .on_update(my_update_system)
    }
}
```

Plugins are deduplicated by type: calling `add_plugin` twice with the same plugin type only
builds it once.

## AI assistant skills

This repository is also a Claude Code plugin marketplace. Its `gearbox` plugin teaches
Claude how to build apps with the crate, including its current limitations. To install it
for a project that depends on gearbox (install the `anarchy` plugin too, for the ECS
itself):

```sh
claude plugin marketplace add DaylightNebula/Gearbox
claude plugin install gearbox@gearbox
```

| Skill | Covers |
| --- | --- |
| `gearbox:app` | Setup, the `App` and its graphs, systems and ordering, rates and delta time, entities, events, plugins, background work, egui and the web |
| `gearbox:rendering` | `GearboxRenderPlugin`, cameras and transforms, meshes, materials, textures, drawing in the main pass and custom materials or meshes |

The skills are plain Markdown in [`plugins/gearbox/skills/`](plugins/gearbox/skills), so
other AI tools can read them too. Contributors working on the crate itself get a separate
skill in `.claude/skills/gearbox`.

## Running the examples

```sh
cargo run --example egui --features auto-egui
cargo run --example basic
cargo run --example shapes
cargo run --example textured
```
