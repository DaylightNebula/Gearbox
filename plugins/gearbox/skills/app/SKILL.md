---
name: app
description: Use when writing a Rust application or game on the gearbox crate - adding the dependency, building an App, adding plugins, resources and systems to the startup/update/render graphs, ordering systems with pins, update and render rates, delta time, reading window and input events, spawning entities, background tasks with run_async, egui debug UI, the component editor, and building for the web. For drawing meshes, materials, textures and cameras see the gearbox rendering skill; for UI the ui-layout, ui-entity-widgets and ui-immediate-widgets skills; for skinned models the skeletal skill; for physics the physics skill; for ECS basics (queries, resources, hierarchy) see the anarchy skills.
---

# gearbox: apps, systems and plugins

gearbox runs an [anarchy](https://github.com/DaylightNebula/anarchy) ECS `World` inside a
`winit` window, renders with `wgpu`, and includes an `egui` integration. It is early, work
in progress, and the API changes. If this skill and the compiler disagree, trust the
compiler and `cargo doc -p gearbox --open`.

## Setup

gearbox and its sibling crates are referenced by relative path, so an app must sit next to
them (`../gearbox`, `../anarchy`, `../shader_magician`):

```toml
[dependencies]
gearbox = { path = "../gearbox" }
anarchy = { path = "../anarchy" }   # only needed for #[derive(Component)] / #[derive(Resource)]
winit = "0.30"                      # only needed to match on WindowEvent / DeviceEvent contents
```

gearbox re-exports `anarchy`, `anyhow`, `magician_vgpu`, `glam` and `shaders`, so most apps
need nothing else. The exception is the derives: their generated code names `::anarchy`,
so an app that derives `Component` or `Resource` must depend on `anarchy` directly.

Optional modules are behind features: `skeletal` (skinned models and animation, see the
gearbox skeletal skill), `rapier` (physics, see the gearbox physics skill), and `editor`,
`auto-egui` and `dbg-schedules` (below). The `ui` module is always included.

## A minimal app

```rust
use anarchy::Resource;
use gearbox::anarchy::ResMut;
use gearbox::App;

#[derive(Debug, Resource)]
struct Counter(u32);

fn tick(mut counter: ResMut<Counter>) {
    counter.0 += 1;
}

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_resource(Counter(0))
        .on_update(tick)
        .run()
}
```

- Components and resources must implement `Debug` and `Send`.
- `run` blocks until the window closes. It returns an error if appending a system failed
  (a cycle or a contradictory pin), or if the event loop fails.

## The four graphs

An `App` owns four anarchy `SystemGraph`s:

| Builder method | Runs | Thread |
| --- | --- | --- |
| `on_startup` | once, when `run` starts | the update thread |
| `on_update` | `UpdateRate` times per second (default 60) | the update thread, on `with_update_threads` threads (default 4) |
| `on_render_startup` | once, before the first frame, after `Graphics` exists | the event loop thread |
| `on_render_update` | once per frame | the event loop thread |

- **GPU work goes in render systems.** `Graphics` (the `VirtualGpu`: `device()`,
  `queue()`, `config()`, `window()`) is inserted once the window exists. Startup and
  update systems that take `Res<Graphics>` fail until then. Create meshes, materials and
  textures in `on_render_startup`.
- **Update and render run at the same time** on different threads. They share data
  through component and resource locks, so keep guards short.
- On the web there are no threads: the update graph is driven from the render loop.
- Every builder method has a `_with` variant (`on_update_with`, `on_render_update_with`,
  ...) taking an anarchy `SystemInstruction` to order it.

## Systems

Systems are plain functions whose parameters are all anarchy system params:

| Parameter | Gives |
| --- | --- |
| `World` | the whole world (cheap clone) |
| `Res<R>` / `ResMut<R>` | read or write a resource |
| `Query<(&'static A, &'static mut B)>` | every entity with those components. Terms need `'static` here |
| `Event<E>` | send and read events of type `E` |
| `Hierarchy` | parent and child links |

```rust
use gearbox::anarchy::{Query, Res};
use gearbox::{Transform, UpdateDeltaTime, glam::{EulerRot, Quat}};

fn spin(dt: Res<UpdateDeltaTime>, mut query: Query<&'static mut Transform>) {
    while let Some((_id, mut transform)) = query.next().unwrap() {
        transform.rotate_by(Quat::from_euler(EulerRot::XYZ, 0.0, dt.0, 0.0));
    }
}
```

- **Systems return `()`, not `Result`.** Use `unwrap`/`expect`, `let ... else { return }`,
  or log with `log::error!`.
- **A param that can't be built skips the system.** For example `Res<R>` with no `R`
  inserted. The app logs the error once and keeps running.
- **A system's identity is its function type.** Adding the same function twice to one
  graph replaces it.
- **Deadlocks.** Locks are not reentrant. Never take `Res<R>` and `ResMut<R>` for the same
  `R` in one system, or lock a resource through `World` that a param already holds.
- `Query::next` is not an `Iterator`: loop with `while let`.

## Ordering

```rust
use gearbox::anarchy::{SystemPin, after, before, pin, and};

App::new()
    .on_update(read_input)
    .on_update_with(movement, after(read_input))
    .on_render_update_with(draw_hud, pin(SystemPin::End));
```

- `pin(SystemPin::Start | Normal | End)` sets the phase. Each pin finishes before the next
  starts, and `Normal` is the default.
- `before(f)` / `after(f)` order against another system function. `and(a, b)` combines two
  instructions.
- Unordered update systems may run at the same time on different threads.
- In the render graph, gearbox and egui use pins:
  - `Start`: the egui frame opens, cameras update and asset uploads happen.
  - `Normal`: the main render pass is drawn.
  - `End`: egui is drawn on top.
  
  Put your own render systems in `Normal`, the default.

## Rates and time

| Resource | Meaning |
| --- | --- |
| `UpdateRate(u32)` | update passes per second, 0 = as fast as possible. Set with `with_update_rate`, mutable at runtime |
| `RenderRate(u32)` | frame cap, 0 = uncapped (default). Set with `with_render_rate`. Ignored on the web |
| `UpdateDeltaTime(f32)` / `RenderDeltaTime(f32)` | seconds between the starts of the last two passes / frames |
| `WindowDimensions` | window size in pixels, derefs to `UVec2` |

`with_vsync(true)` turns vsync on (off by default). `FrameCount(u64)` holds the number of
the frame being rendered (1 during the first frame).

## Headless runs, screenshots and frame limits

Use these to see what an app draws, in tests and in sessions without a display:

```rust
use std::time::Duration;

App::new()
    .headless(640, 480)                       // offscreen, no window or input events
    .with_frame_limit(60)                     // run returns after frame 60
    .with_fixed_timestep(Duration::from_millis(16))
    .with_screenshot(60, "out/frame60.png")   // saved as PNG when frame 60 is submitted
    .add_plugin(GearboxRenderPlugin)
    .run()
```

- **Fixed time step.** `with_fixed_timestep(step)` runs one update pass per frame, on the
  render thread, with both delta times equal to `step`. It ignores `UpdateRate`.
  - Before each frame, it waits for all `run_async` work (asset loads) to finish, so assets
    arrive on the same frame every run. A texture requested in frame 1's render startup
    draws in frame 2.
  - Headless apps default to a 1/60 s step.
- **Screenshots from systems.** Take `ResMut<Screenshots>` and call `capture(path)` (the
  frame being drawn, when called from a render system) or `capture_at(frame, path)`.
- **Checking pixels.** Open a saved PNG with `image::open(path)?.to_rgba8()` and check
  pixels. The headless target is sRGB, so a `ClearColor(Vec4::new(1.0, 0.0, 0.0, 1.0))`
  reads back as `[255, 0, 0, 255]`.
- **Environment variables.** These override the builder for any app, with no code
  changes. Prefer them for running existing examples or apps:

  | Variable | Effect |
  | --- | --- |
  | `GEARBOX_HEADLESS` | `WIDTHxHEIGHT`, or `1` for 800x600 |
  | `GEARBOX_FRAMES` | frame limit |
  | `GEARBOX_FIXED_TIMESTEP` | step in seconds |
  | `GEARBOX_SCREENSHOT` | save the last frame here (implies a 60 frame limit if `GEARBOX_FRAMES` isn't set) |

  ```sh
  GEARBOX_HEADLESS=640x480 GEARBOX_SCREENSHOT=out.png cargo run --example shapes
  ```

- **egui works headless** (`EguiCtx::state` is `None`, as there's no window or input).
- **A test can skip without a GPU.** `run` returns an error mentioning "No GPU adapter"
  when there's no GPU, so a test can check for that and return.
- Headless mode isn't supported on the web.

## Entities

anarchy never picks entity ids. Use gearbox's `spawn`, which hands out unique ids:

```rust
use gearbox::{Camera, Transform, spawn, glam::{Quat, Vec3}};

fn setup(world: gearbox::anarchy::World) {
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 0.0, 6.0), Quat::IDENTITY, Vec3::ONE)),
        Box::new(Camera::default()),
    ]);
}
```

`spawn` returns the `EntityID`. `next_entity_id(&world)` allocates an id without inserting
anything. An entity needs at least one component, and its component set is fixed once
inserted (see the anarchy recipes skill to add or remove components).

## Window and input events

Every winit event is sent into the world as `gearbox::WindowEvent` or
`gearbox::DeviceEvent`, which deref to the winit type:

```rust
use gearbox::{WindowEvent, anarchy::Event};
use winit::event::{ElementState, WindowEvent as W};

fn input(events: Event<WindowEvent>) {
    for event in events.read() {
        if let W::KeyboardInput { event, .. } = &*event
            && event.state == ElementState::Pressed {
            log::info!("pressed {:?}", event.logical_key);
        }
    }
}
```

- Each system has its own read cursor and sees every event once.
- Events expire after one second, so a system that runs less often misses them.

## Plugins

```rust
use gearbox::{App, Plugin};

pub struct PhysicsPlugin;
impl Plugin for PhysicsPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(Gravity(9.8))
            .on_update(gravity)
    }
}

App::new().add_plugin(PhysicsPlugin).run()
```

Plugins are deduplicated by type: adding one twice builds it once.

## Background work

`gearbox::run_async(future)` runs a future off the current graph: on a new thread
natively, and on the browser's event loop on the web. The asset vaults use it to load files.
Hand results back through a resource or an atomic, and do GPU uploads in a render system.

## egui

```rust
use gearbox::{App, EguiCtx, EguiPlugin, egui::egui, anarchy::{Res, ResMut}};

fn debug_ui(egui: Res<EguiCtx>, mut settings: ResMut<Settings>) {
    egui::Window::new("Debug").show(&egui.context, |ui| {
        ui.add(egui::Slider::new(&mut settings.speed, 0.0..=10.0).text("Speed"));
    });
}

App::new().add_plugin(EguiPlugin).on_render_update(debug_ui).run()
```

- Draw UI from render systems in the `Normal` pin.
- The `auto-egui` cargo feature adds `EguiPlugin` automatically. `dbg-schedules` adds a
  window showing the update and render FPS.

## Editor

The `editor` cargo feature adds `EditorPlugin` in `App::new()`: an egui entity list and an
inspector that views and edits components through anarchy reflection.

```rust
use gearbox::anarchy::{Component, Reflect};

/// Spins an entity around its Y axis.
#[derive(Debug, Component, Reflect)]
struct Spin {
    /// Radians per update, shown as hover text.
    #[reflect(hint(min = 0.0, max = 0.2))] // a slider instead of a drag value
    speed: f32,
}
```

- A component appears in the inspector once it derives `Reflect`; others are listed as "not
  reflected". Generic components must be registered with `world.register_component::<T>()`.
- Users of the derive need a direct `anarchy` dependency, as with `Component`.
- `Transform` and `Camera` are reflected; glam vectors show as `x y z`, and `Quat` as Euler
  angles in degrees.
- Edits use `World::set_component_path`, which sends `ReflectEdit` events.
- `EditorState` (a resource) holds the selected entity.

## Web

Builds for `wasm32` with [Trunk](https://trunkrs.dev). The page needs a
`<canvas id="canvas">` (see the repo's `index.html`), and `Trunk.toml` picks the example to
build. `AssetContent::LocalPath` paths are fetched relative to the page URL, and
`AssetContent::Url` isn't supported on the web.
