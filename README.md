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
- `ui` — signed-distance-field UI drawn in one fullscreen pass: flexbox node trees, entity
  widgets and immediate mode widgets (formerly the `iso_ui` crate).
- `skeletal` (`skeletal` feature) — skinned glTF/FBX model loading, animation and GPU
  skinning (formerly the `skeletal` crate).
- `rapier` (`rapier` feature) — rapier3d physics driving and driven by `Transform`s
  (formerly the `anarchy_rapier` crate).

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
| `editor` | Adds `EditorPlugin`: an egui entity list and inspector for viewing and editing reflected components. |
| `skeletal` | Adds the `skeletal` module: skinned mesh loading (glTF/GLB, and FBX on desktop) and animation. |
| `rapier` | Adds the `rapier` module: rapier3d physics bodies, characters and queries. |

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

### Editing components (`editor` feature)

With the `editor` feature, `App::new()` adds `EditorPlugin`: an "Entities" window lists every
entity, and selecting one opens an inspector for its components. It works through anarchy's
reflection, so a component shows up once it derives `Reflect` (gearbox's `Transform` and
`Camera` already do). Field docs become hover text, and `#[reflect(hint(min, max))]` turns a
number into a slider. Edits go through `World::set_component_path`, so they send
`ReflectEdit` events.

```rust
use anarchy::{Component, Reflect};

/// Spins an entity around its Y axis.
#[derive(Debug, Component, Reflect)]
struct Spin {
    /// Radians per update.
    #[reflect(hint(min = 0.0, max = 0.2))]
    speed: f32,
}
```

See `examples/editor.rs` (`cargo run --example editor --features editor`).

### UI

`ui` draws UI as signed distance fields (rectangles, circles, bezier strokes, font glyphs and
vector shapes)
in a single fullscreen pass over the scene, laid out like CSS flexbox with
[taffy](https://github.com/DioxusLabs/taffy). It offers three ways to build UI, all from
`UIPlugin`:

- **Node trees:** a `UINodeSDFRoot(UINode)` component holding a tree of `UINode`s styled with
  `Style` (sizes as `Val`, margin/padding/border, flex row/column/grid, `Anchor`s, absolute
  positioning, text through `SDFFont`).
- **Entity widgets:** every node is an entity with a `Node` component, built as a `UiBundle`
  and spawned with `world.spawn(bundle.into_entity_builder())`. Behavior-only widgets
  (`Button`, `Checkbox`, `Slider`, `TextInput`, ...) send `Activate`/`ValueChange`/`Submit`
  events, and `Node::shape` draws vector icons. **Prefer building your own `UiBundle`s for
  retained UI**; the `themed::` builders give the immediate mode look and suit tools and debug
  panels.
- **Immediate mode widgets:** with `WidgetPlugin`, describe windows every frame through the
  `Widget` resource.

```rust
use std::sync::Arc;
use gearbox::anarchy::ResMut;
use gearbox::{App, GearboxRenderPlugin, glam::Vec2, ui::*};

fn main() -> gearbox::anyhow::Result<()> {
    let font = Arc::new(SDFFont::new(include_bytes!("./LiberationSans-Regular.ttf"))?);
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        .add_plugin(WidgetPlugin::new(font))
        .on_render_update(settings_ui)
        .run()
}

fn settings_ui(mut widget: ResMut<Widget>) {
    widget.panel("Settings", Vec2::splat(10.0), 320.0, |ui| {
        if ui.button("Save") { log::info!("saved"); }
    });
}
```

See the `ui_*` examples, and the `gearbox:ui-*` skills below for the full API.

### Skinned meshes (`skeletal` feature)

`SkeletalMeshPlugin` adds `SkeletalMeshVault`, which loads `.gltf`/`.glb` and (on desktop)
`.fbx` models in the background and returns mesh, material and animation handles. Draw the
mesh like any other with `MeshRef`, and play its animations with an `Animator` component:

```rust
let (mesh, material, animations) = meshes.load(&world, AssetContent::LocalPath("model.glb".into()), SkeletalMeshLoadType::GLTF)?;
let mut animator = Animator::empty();
animator.load_animations(animations);
animator.play("Idle", true);
spawn(&world, vec![
    Box::new(Transform::identity()),
    Box::new(MaterialRef::new(material)),
    Box::new(animator),
    Box::new(MeshRef::new(mesh)),
]);
```

See the `skeletal_gltf`, `skeletal_fbx` and `skeletal_viewer` examples. Their model files
aren't committed: put them in `examples/skeletal_*/`, or pass one to the viewer.

### Physics (`rapier` feature)

`PhysicsPlugin` steps a rapier3d world in the update graph. A `PhysicsObject` (fixed or
movable, with a `Collider` and mass) on an entity with a `Transform` gets a rigidbody, and
physics writes its position back into the `Transform`. Add `PhysicsCharacter` for a
kinematic character controller, and use the `PhysicsQueries` resource for raycasts and shape
casts, answered on the next tick.

```rust
use gearbox::rapier::{Collider, PhysicsObject, PhysicsPlugin};

spawn(&world, vec![
    Box::new(Transform::new(Vec3::new(0.0, 10.0, 0.0), Quat::IDENTITY, Vec3::ONE)),
    Box::new(PhysicsObject::movable(Collider::Sphere { radius: 0.5 }, 1.0)),
]);
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

Systems built at runtime, which can't be told apart by their Rust type (such as ones loaded
from wasm modules), go in with `add_system_raw(AppSchedule::Update, key, system, instruction,
metadata)` under a `SystemKey` of your choosing. A plugin that fails while building can pass
the error to `report_error`, and `run` returns it.

### Wasm modules (Clarity)

Clarity, a separate project in `../Clarity` (next to this repo), loads wasm modules into an
app. Modules define their own components, resources, systems and plugins, and can use
gearbox's `Transform`. See its README.

## Headless runs, screenshots and frame limits

Any app can run without a window, stop after a set number of frames, and save frames as
PNGs. This is meant for tests, CI and AI agents that need to see what was drawn.

```rust
App::new()
    .headless(640, 480)                       // render offscreen, no window
    .with_frame_limit(60)                     // exit after 60 frames
    .with_fixed_timestep(Duration::from_secs_f64(1.0 / 60.0))
    .with_screenshot(60, "out/frame60.png")   // save frame 60
    .add_plugin(GearboxRenderPlugin)
    .run()
```

- **Fixed time step.** With `with_fixed_timestep`, the app runs one update pass per frame
  and sets both delta times to the step. It also waits for background asset loads before
  each frame, so every run produces the same frames. Headless apps use a 1/60 s step
  unless you set one.
- **Screenshots from systems.** A system can request a screenshot through the
  `Screenshots` resource (`capture(path)` or `capture_at(frame, path)`). `FrameCount` holds
  the current frame number.
- **No code changes needed.** These environment variables turn the same features on for
  any app, including the examples:

| Variable | Effect |
| --- | --- |
| `GEARBOX_HEADLESS` | `WIDTHxHEIGHT`, or `1` for 800x600: render offscreen with no window |
| `GEARBOX_FRAMES` | exit after this many frames |
| `GEARBOX_FIXED_TIMESTEP` | fixed time step, in seconds |
| `GEARBOX_SCREENSHOT` | save the last frame to this path (with a 60 frame limit unless `GEARBOX_FRAMES` is set) |

```sh
GEARBOX_HEADLESS=640x480 GEARBOX_SCREENSHOT=shapes.png cargo run --example shapes
```

## Testing the web build

With rustup's `wasm32-unknown-unknown` target installed (Homebrew's Rust can't build for
wasm32, so put `~/.cargo/bin` first on `PATH`), check the library, examples and tests:

```sh
cargo check --target wasm32-unknown-unknown --all-targets --all-features
```

`trunk serve` builds the example named in `Trunk.toml` into `index.html`'s canvas. FBX loading
is desktop-only, so `SkeletalMeshLoadType::FBX` doesn't exist on the web.

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
| `gearbox:ui-layout` | `UIPlugin`, `Style` and flexbox, `Val` sizes, the box model, anchors and absolute positioning, text, colors, and `UINodeSDFRoot` trees with their events |
| `gearbox:ui-entity-widgets` | Entity nodes built as `UiBundle`s (preferred for retained UI), shapes and icons, behavior-only widgets and their events, hover styling, `themed::` builders, theming and `ThemeOverride`, frame order |
| `gearbox:ui-immediate-widgets` | The `Widget` resource and `Ui` builder, windows, ids and state, theme and input |
| `gearbox:skeletal` | Loading skinned glTF/FBX models, `Animator`, hiding sub-meshes |
| `gearbox:physics` | `PhysicsPlugin`, `PhysicsObject` and `Collider`, forces, characters, raycasts and shape casts |

The skills are plain Markdown in [`plugins/gearbox/skills/`](plugins/gearbox/skills), so
other AI tools can read them too. Contributors working on the crate itself get a separate
skill in `.claude/skills/gearbox`.

## Running the examples

```sh
cargo run --example egui --features auto-egui
cargo run --example basic
cargo run --example shapes
cargo run --example textured
cargo run --example editor --features editor
cargo run --example ui_widgets          # also ui_basic, ui_layout, ui_bundles, ui_entity_widgets, ui_custom_slider, ui_shapes
cargo run --example skeletal_viewer --features skeletal -- path/to/model.glb
```
