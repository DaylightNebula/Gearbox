---
name: gearbox
description: How to work on the gearbox crate itself (Rust renderer and app framework on anarchy, winit and wgpu, with its SDF ui module, skeletal animation and rapier physics) - where things live, the sibling repos it depends on, how to build, test and run the examples, internal gotchas, conventions, and keeping the README and user-facing skills in sync. Use when changing, testing or documenting code in this repository.
---

# Working on gearbox

gearbox is an ECS-based renderer and app framework: an anarchy `World` runs inside a winit
window and is drawn with wgpu through `magician_vgpu`. It absorbed the former `cell` crate
(the `App`, plugins, egui and event loop), and later `iso_ui` (now `src/ui`), `skeletal`
(`src/skeletal`, `skeletal` feature) and `anarchy_rapier` (`src/rapier`, `rapier` feature).
The API is early and changes freely. When this
skill disagrees with the code, trust the code.

## Sibling repos

`Cargo.toml` uses path dependencies, so these must sit next to this repo:

| Path | What it is |
| --- | --- |
| `../anarchy` | The ECS, used with its `glam` feature (`Reflect` for glam types). It has its own repo skill and user skills (`plugins/anarchy/skills`). Read those for ECS behaviour |
| `../shader_magician` | `magician-vgpu` (GPU wrapper: `VirtualGpu`, `RenderFrame`, `SinglePass`, buffers, textures), `magician-rust` and `magician-macros` (Rust-to-WGSL shaders) |
| `../Clarity` | Depends on gearbox (not the reverse): loads wasm modules into an `App` through `App::add_system_raw`. Changing `App`'s raw system or `report_error` API breaks it, so build it after such changes |

A bug can live in a sibling. Fix it there, and build gearbox against it.

## Map of the crate

| Path | What lives there |
| --- | --- |
| `src/lib.rs` | Crate docs, module list, re-exports (`anarchy`, `anyhow`, `magician_vgpu`, `glam`, `shaders`), `GearboxRenderPlugin` and the main-pass systems (`update_cameras`, `begin_main_pass`, ...) |
| `src/app.rs` | `App` builder: the four `SystemGraph`s, the update thread, the headless loop, `render()` (frame count, fixed step, screenshots, frame limit), `configure_surface` |
| `src/frames.rs` | `FrameCount` and `Screenshots` resources, PNG saving, and parsing of the `GEARBOX_*` environment variables |
| `src/wrapper.rs` | winit `ApplicationHandler`: window and GPU creation, sending events into the world, the render rate cap, retrying skipped frames |
| `src/schedule.rs` | `UpdateRate`/`RenderRate`/delta-time resources, `Pacer`, `ErrorLog`, and `MainRenderPassSchedule` with `render_mesh_material` |
| `src/gpu.rs` | `Graphics` (wraps `VirtualGpu`) and `Frame` (the in-progress `RenderFrame`) resources |
| `src/egui.rs`, `src/plugin.rs`, `src/utils.rs` | `EguiPlugin`/`EguiCtx` (with a windowless input path); the `Plugin` trait; `WindowEvent`/`DeviceEvent`, `spawn`, `next_entity_id`, `run_async` and its in-flight counter (`wait_for_async_tasks`) |
| `src/editor.rs` | `EditorPlugin`/`EditorState` (`editor` feature): egui entity list and component inspector built on anarchy reflection (`to_value` to read, `set_component_path` to write) |
| `src/camera.rs`, `src/transform.rs` | `Camera` and `Transform` components |
| `src/mesh/`, `src/material/` | `Mesh`/`MeshRef`, `BasicMesh`; `Material`/`MaterialRef`, `BasicMaterial`, `SimpleTexturedMaterial`, `MaterialPipelineStorage` |
| `src/shapes/` | `ShapeBuilder` and the primitive shapes |
| `src/vault/` | Asset system: `Handle`, `AssetContent`, the vault traits, the bindless and atlas texture vaults, `TextureVault` facade, `MaterialVault`, `MeshAssetVault` |
| `src/ui/` | SDF UI (always on), see [UI module](#ui-module) |
| `src/skeletal/` | `skeletal` feature: `SkeletalMeshPlugin`, `SkeletalMeshVault` (async glTF/FBX loads, finished by `load_inprogress` in a render system), `SkeletalMeshHandle` (a `Mesh` with its own skinning pipeline), `Animator`/`AnimationVault`, `loader::{gltf, fbx}` (fbx is native only) |
| `src/rapier/` | `rapier` feature: `PhysicsPlugin` and `update_engine_side` (update `End` pin), `PhysicsEngine` (wraps the rapier sets), `PhysicsObject`, `PhysicsCharacter`, `Collider`, `PhysicsQueries` |
| `shaders/` | Separate crate of Rust shaders. `build.rs` writes WGSL to `shaders/shader_out` (gitignored). `shaders::skeletal` (the skinning shader) is behind the crate's `skeletal` feature, which gearbox's `skeletal` feature turns on; its WGSL is generated either way |
| `examples/` | `basic`, `shapes`, `textured`, `obj`, `egui`, `editor`; `ui_*` (UI); `skeletal_*/` (need `--features skeletal`, model files not committed); test textures in every format and `LiberationSans-Regular.ttf` |
| `scripts/` | `check_skills.py` (compiles the user skills' snippets) and `pngdiff.py` (screenshot diffs) |
| `docs/intelligence.md` | Roadmap for making gearbox agent-friendly |
| `plugins/gearbox/skills/` | User-facing skills (`app`, `rendering`). Keep them in sync, see below |

## How a frame runs

1. `wrapper.rs` gets `RedrawRequested`, applies the `RenderRate` cap, and calls `App::render`.
   A headless app (`App::run_headless`) has no event loop and calls `render` in a loop instead.
2. `render` begins a `RenderFrame` and stores it in the `Frame` resource. On the first
   frame it runs the render startup graph, then the render graph, then submits.
   - If there is no `Graphics` yet, or the surface can't give a texture (hidden window,
     zero size), `render` returns `false`. The wrapper then retries after 100 ms natively,
     and right away on the web.
   - With a fixed time step, `render` first waits for `run_async` tasks, then runs one
     update pass after the frame begins.
   - After the render graph, frames due in `Screenshots` are captured with
     `RenderFrame::submit_and_capture` and saved. Reaching the frame limit sets
     `App::finished`, which ends the headless loop or exits the event loop.
3. The render graph's pins:
   - `Start`: egui frame start, `update_cameras`, vault uploads (`update_*_textures`,
     `finalize_mesh_loads`).
   - `Normal`: `begin_main_pass` → `execute_render_schedule` (runs `MainRenderPassSchedule`,
     which includes `render_mesh_material`) → `complete_main_pass`.
   - `End`: egui is drawn over the output.
4. The update graph runs separately: on its own thread natively (`MultiThreadedExecutor`,
   one pass per `UpdateRate` tick), or from `render` on the web.

## Build, test, run

```sh
cargo build --all-targets                  # no warnings expected
cargo build --all-targets --all-features   # also covers auto-egui / dbg-schedules / editor
cargo test --all-features                  # unit tests (vault, ui layout, picking and widget systems)
cargo run --example shapes                 # also basic, textured, egui; obj needs a model
cargo run --example editor --features editor
```

- Run examples from the repo root: asset paths like `./examples/cobblestone.png` are
  relative to the working directory.
- `obj` needs `examples/SM_Prop_Bonsai_01.obj`, which isn't committed (`.gitignore` has
  `*.obj`).
- Web: `trunk serve` builds the example named in `Trunk.toml` into `index.html`'s canvas.

## Testing the web build

Check the `wasm32` target after touching anything web-only (`cfg(target_arch = "wasm32")`
code, time, asset fetching, canvas setup), or any feature module:

```sh
PATH=~/.cargo/bin:$PATH cargo check --target wasm32-unknown-unknown --all-targets --all-features
PATH=~/.cargo/bin:$PATH cargo test --target wasm32-unknown-unknown --all-features --lib
```

- **Homebrew's Rust can't build for wasm32.** Put `~/.cargo/bin` (rustup) first on `PATH`.
- `cargo test` on wasm32 runs in Node through `wasm-bindgen-test-runner` (the cargo runner in
  `.cargo/config.toml`). Use `#[wasm_bindgen_test]` there, and keep tests to logic: Node has
  no window or GPU. Tests needing threads, files, sockets or `pollster` go under
  `#[cfg(all(test, not(target_arch = "wasm32")))]`, like the vault tests.
- **Desktop-only pieces:** FBX loading (`ufbx`; `SkeletalMeshLoadType::FBX` doesn't exist on
  wasm32), the `rfd` file dialog in `skeletal_viewer`, and `arboard` clipboard access.
  Examples using them still have to compile for wasm32: gate those parts, and give a
  native-only example a stub `main` (see `skeletal_fbx`).
- `trunk serve` builds the example named in `Trunk.toml` to try it in a browser.

## Verifying visual changes

Render headless and look at the result. This works without a display, and the `Read`
tool shows PNGs:

```sh
GEARBOX_HEADLESS=640x480 GEARBOX_SCREENSHOT=/tmp/shapes.png cargo run --example shapes
```

- Headless runs use a fixed 1/60 s step and wait for asset loads, so the same command
  gives the same image every time.
- `GEARBOX_FRAMES` and `GEARBOX_FIXED_TIMESTEP` control the frame count and step.
- In code, build the scene with `.headless`, `.with_frame_limit` and `.with_screenshot`. The
  headless target is sRGB, so a clear color of `(1, 0, 0, 1)` reads back as
  `[255, 0, 0, 255]`. A GPU test must skip itself when `run` reports "No GPU adapter".
- Compare two screenshots with `python3 -I scripts/pngdiff.py before.png after.png`. Take the
  "before" one first, rather than stashing work to rebuild it.
- A windowed run proves less: a hidden or covered window reports `Occluded` and draws
  nothing. Smoke-test windowed mode with a frame limit:
  `GEARBOX_FRAMES=30 cargo run --example shapes` should exit on its own with status 0.

## UI module

`src/ui` draws UI as signed distance fields in one fullscreen fragment pass, laid out as CSS
flexbox through taffy. Three ways to build UI: `UINodeSDFRoot` node trees, entity nodes (with
behavior-only and themed widgets), and immediate mode (`Widget`/`Ui`).

| Path | What lives there |
| --- | --- |
| `mod.rs` | `UIPlugin` (resources and system ordering), GPU setup (`init_resources`), `ui_render_pass` (merges entity elements, `UINodeSDFRoot` trees and `UIRawElements`, uploads and draws) |
| `nodes/data.rs`, `nodes/render.rs`, `nodes/outline.rs` | `UINode`, `Style`, `Val`, `Anchor`, `Text`, `Background`; layout (`layout_tree` builds a taffy tree and emits `SDFElement`s in paint order); `Outline`, the vector shapes `Background::Shape` draws as `SDFShape::Glyph`. Layout and outline tests |
| `data.rs`, `shader.rs`, `shaders/*.wgsl` | `SDFElement`/`SDFShape`/`SDFStyle` (`Wave` is the squiggly underline), their GPU layouts, and the WGSL (`include_str!`'d) |
| `fonts/` | `SDFFont`: text measuring, cached glyph outlines as bezier SDFs, an ASCII advance table; `fonts/layout.rs` is `TextLayout` (lines, wrapping, hit testing, caret and range boxes), shared by measuring, drawing, carets and hovers |
| `edit.rs` | `TextEditor` (caret, selection, word/line moves, undo, clipboard shortcuts), `field_shows`, `follow_caret`, span remapping (`text_change`, `remap_spans`); shared by `TextInput` and immediate fields |
| `widgets.rs` | Immediate mode (`WidgetPlugin`, `Widget`, `Ui`), `Theme`, `srgb`, and the size constants themed widgets share |
| `ecs/` | Entity UI: `node.rs` (`Node`, `UiBundle`, `into_entity_builder`), `layout.rs` (also records each node's `TextPlacement` in `EntityUiElements::texts`), `picking.rs` (also `hover_text_spans`/`TextHover`; scrollbars hide what's under them), `scroll.rs` (`scroll_nodes`: wheel, thumb drags, track pages), `input.rs` (`UiInput`, `UiFocus`, `UiTheme`, `ThemeOverride`), `controls.rs` (behavior-only widgets, context menus in `run_menus`, system tests), `themed.rs` (`themed::` builders, `restyle_themed`) |

Frame order, by render pin:
1. **`Start`:** `read_ui_input` fills `UiInput`; `widgets_begin` hit tests last frame's
   windows; `pick_ui` uses last frame's `ComputedNode`s; then the `UiWidgetsPlugin` systems.
2. **`Normal`:** user systems and immediate mode descriptions.
3. **`End`:** `widgets_end`, `restyle_themed`, `layout_entity_ui`, then `ui_render_pass`
   (entity elements first, then node trees, so immediate windows draw on top).

UI gotchas:
- **`UiBundle`s first.** User docs and examples steer retained UI to hand-built `UiBundle`s;
  `themed::` is for the immediate mode look. Write new examples and skill snippets that way.
- **Parity.** `ui_entity_widgets` and `ui_widgets` must render identical pixels. When a widget
  changes, change both `Ui` and its `themed::` builder or restyle arm, then diff screenshots of
  the two with `python3 -I scripts/pngdiff.py a.png b.png`. Themed parts are found by child
  index, so changing a builder's children means updating its restyle arm too. The Greet
  button's context menu covers menus: right click it from a throwaway driver (see Input in
  headless runs) and diff both. Immediate windows opened while building another (submenus) are
  inserted after it, to stack like entity `Fixed` nodes.
- **Defaults differ from CSS:** `flex_direction` is `Column`, `Auto` margins, padding, borders
  and gaps are 0, and borders take room inside the size. `Percent` is parent-relative,
  `PercentWidth`/`PercentHeight` window-relative.
- **Scaling:** entity styles are scaled by the window scale factor in `layout_entity_ui`;
  `UINodeSDFRoot` trees aren't, and immediate mode scales itself through `px()`.
- **Style setters** (`getset`) return `&mut Style`, so closures calling them need braces.
  `Node` deliberately doesn't deref to `Style`.
- **Clipping:** the shader skips an element's children wherever a pixel is outside the element,
  so children are always cut off at their parent's box (`Overflow::Clip`). Scrolling nodes wrap
  their children and text in an empty element the size of the view, then add the bars after
  it; `Text::clip` does the same with the content box. `Emit::element` passes the clip box
  down, records it in `LaidOut::clip` (picking checks it through `ComputedNode::contains`), and
  leaves out children and text lines entirely outside it.
- **Scrolling:** `Overflow::Auto` starts as taffy `Hidden` and becomes `Scroll` (reserving the
  gutter) once `add_auto_scrollbars` finds it overflowing, then layout runs again, at most three
  times. Bar geometry (`Scrolled`, `thumb_rect`, `offset_at`) is shared by drawing, entity input
  (`ecs/scroll.rs`) and immediate mode's `scroll_offset`.
- **`Fixed` nodes** are lifted out of their parents to the roots (`lift` in `nodes/render.rs`),
  before their own fixed descendants so submenus draw over menus, and dropped under hidden
  nodes. Context menus rely on this to escape scroll areas.
- **Text measuring:** taffy hands the measure function the node's whole width as `known.width`;
  `TextMeasure::inset` takes the padding and border off before wrapping.
- **Buffer limits:** every glyph is one element. Where `max_storage_buffers_per_shader_stage`
  allows, the SDF buffers are storage buffers (`MAX_STORAGE_ELEMENTS`, 16 bit pointers);
  otherwise (WebGL2) uniform arrays (`MAX_BUFFER_ELEMENTS`, less under WebGL). Test both: force
  `storage` false in `init_resources` and diff screenshots. Text culls lines outside the
  window or its clip box.
- **Thin lines:** SDF coverage is the distance inside the shape, so a 1px-wide shape peaks at
  half alpha and vanishes when centered between pixels. Carets and underlines are 2px wide
  centered on a pixel center.
- **Text parity:** themed `text_input` and immediate `text_field` both draw through `Text`
  with `cursor`/`clip`/`scroll` and the same `edit.rs` helpers; keep them in step.
- **Input in headless runs:** there is none. Send `WindowEvent(CursorMoved/MouseInput)` from a
  render system in a throwaway `examples/zz_check.rs`, keyed on `FrameCount` (presses a frame
  or two apart count as a double click). Key events can't be built; test typing through
  `UiInput.keys` in unit tests. Text widget tests put a `TextPlacement` in
  `EntityUiElements::texts` to stand in for last frame's layout.
- Widget systems can be tested without a GPU: build a `World`, insert a `UiInput`, and run the
  system through `SingleThreadedExecutor` (see `ecs/controls.rs`).

## Skeletal and rapier gotchas

- **Skeletal:**
  - `AnimationInfo` (in `shaders/src/skeletal.rs`) holds 128 bone and 128 node matrices: 16 KiB, the
    WebGL2 uniform limit. `MAX_SKELETAL_BONES` must match it.
  - `Animator` time is advanced by `advance_animators` from `RenderDeltaTime`, never the wall
    clock, so fixed-step headless runs are deterministic.
  - One `AnimationInfo` buffer per handle is rewritten for each root bone during a draw, so
    (like shared meshes) a model with several root bones draws them all with the last root's
    matrices.
- **Rapier:**
  - anarchy's `Query::new` requires every listed component, `Option` ones included, so
    `update_engine_side` walks characters and plain bodies with two queries. Don't merge them
    back into one with `Option<&mut PhysicsCharacter>`: plain bodies would stop simulating.
  - The engine steps by `UpdateDeltaTime` (capped at `MAX_STEP`).
  - In that system, use `continue` to skip an entity, never `return`: returning skips the
    query run and the step for every body.
  - Tests in `src/rapier/mod.rs` run the system on a bare `World` with no GPU.

## Gotchas

- **Shader struct names are global.** The shader generator resolves struct names across the
  whole `shaders` crate, not per module, so two shader modules with a `VertexOutput` produce
  broken WGSL ("Compose ... ComponentCount" at build time) or silently use the wrong layout.
  Give each shader's input/output structs a unique name (`SkeletalVertexInput`, ...).

- **Graphics timing.** `Graphics` exists only after the window does. Update and startup
  systems taking `Res<Graphics>` fail (logged once by `ErrorLog`) until then. GPU work
  belongs in render systems.
- **Surface setup.** Configure the surface when `Graphics` is created
  (`configure_surface`): macOS sends `RedrawRequested` before the first `Resized`.
- **Empty bind groups.** Both texture vaults run their update systems every frame,
  whichever backend is active. They must not build a bind group before they have a
  texture: wgpu rejects empty texture arrays, and the atlas page doesn't exist yet.
- **Shared meshes.** Mesh instance buffers are per mesh and rewritten with
  `queue.write_buffer` each draw. All writes land before the frame's commands run, so
  entities sharing one mesh draw at the last transform.
- **The camera** always looks at the origin. Its fov/near/far fields are unused (the
  projection is hardcoded in `Camera::update`).
- **Locks.** Nested graphs lock their owner: `MainRenderPassSchedule` is held mutably
  while its systems run. Locks aren't reentrant, so a system holding `ResMut<R>` must not
  lock `R` again through `World`.
- **Systems can't return `Result`** (anarchy v2). Library systems log with `log::error!`.
  A helper returning `anyhow::Result` that the system calls keeps `?` usable.
- **anarchy derives emit `::anarchy`**, which is why users of the derives need a direct
  `anarchy` dependency even though gearbox re-exports it.
- **Asset loading is async** (`run_async`): native uses a thread per load, the web uses
  `spawn_local`. GPU uploads happen later in render `Start` systems. Code must handle "not
  loaded yet" by skipping, not failing.
- **Headless:**
  - `VirtualGpu::window()` and `surface()` are `None` when headless. Use
    `vgpu.request_redraw()` and `vgpu.resize(w, h)`, which handle both cases.
  - `EguiCtx::state` is `None` without a window.
- **Async waits are global.** `wait_for_async_tasks` waits for every `run_async` task in
  the process, not just one app's.
- **Web:**
  - Use `web_time::Instant`, never `std::time::Instant`, which panics on wasm32.
  - wasm-only code paths can't be compiled without the wasm target installed. Say so when
    you couldn't check them.

## Conventions

- **Errors:** `anyhow::Result` with `bail!` / `.context(..)` in fallible functions;
  `log::error!` in systems. Don't `unwrap` in library code that runs every frame.
- **Debug:** every component and resource must implement `Debug` (anarchy requires it).
  Hand-write it with `finish_non_exhaustive()` when fields aren't `Debug` (GPU types).
- **Getters:** use `getset` with struct-level attributes:
  `#[derive(Getters)] #[getset(get = "pub")]`.
- **Docs:** every public item gets a short `///` doc saying what it does. Comments only
  for non-obvious reasons.
- **Pins:** new render systems pick a pin deliberately:
  - `Start`: uploads and prep.
  - `Normal`: drawing.
  - `End`: overlays.

## Keeping docs and skills in sync

When a public API or behaviour changes:
- Update `README.md`.
- Update the matching user skill in `plugins/gearbox/skills/`: `app` (App/systems/events),
  `rendering` (meshes/materials/textures/camera), `ui-layout`, `ui-entity-widgets`,
  `ui-immediate-widgets`, `skeletal` and `physics`.
- Check skill snippets compile:
  `python3 -I scripts/check_skills.py && cargo build --example zz_check --all-features; rm examples/zz_check.rs`.
  It skips `app` and `rendering`, whose snippets are fragments: paste those into
  `examples/zz_check.rs` by hand. Check README snippets the same way.
- Run `claude plugin validate .` after editing any plugin file.
