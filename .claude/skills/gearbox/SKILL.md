---
name: gearbox
description: How to work on the gearbox crate itself (Rust renderer and app framework on anarchy, winit and wgpu) - where things live, the sibling repos it depends on, how to build, test and run the examples, internal gotchas, conventions, and keeping the README and user-facing skills in sync. Use when changing, testing or documenting code in this repository.
---

# Working on gearbox

gearbox is an ECS-based renderer and app framework: an anarchy `World` runs inside a winit
window and is drawn with wgpu through `magician_vgpu`. It absorbed the former `cell` crate
(the `App`, plugins, egui and event loop). The API is early and changes freely. When this
skill disagrees with the code, trust the code.

## Sibling repos

`Cargo.toml` uses path dependencies, so these must sit next to this repo:

| Path | What it is |
| --- | --- |
| `../anarchy` | The ECS. It has its own repo skill and user skills (`plugins/anarchy/skills`). Read those for ECS behaviour |
| `../shader_magician` | `magician-vgpu` (GPU wrapper: `VirtualGpu`, `RenderFrame`, `SinglePass`, buffers, textures), `magician-rust` and `magician-macros` (Rust-to-WGSL shaders) |

A bug can live in a sibling. Fix it there, and build gearbox against it.

## Map of the crate

| Path | What lives there |
| --- | --- |
| `src/lib.rs` | Crate docs, module list, re-exports (`anarchy`, `anyhow`, `magician_vgpu`, `glam`, `shaders`), `GearboxRenderPlugin` and the main-pass systems (`update_cameras`, `begin_main_pass`, ...) |
| `src/app.rs` | `App` builder: the four `SystemGraph`s, the update thread, `render()`, `configure_surface` |
| `src/wrapper.rs` | winit `ApplicationHandler`: window and GPU creation, sending events into the world, the render rate cap, retrying skipped frames |
| `src/schedule.rs` | `UpdateRate`/`RenderRate`/delta-time resources, `Pacer`, `ErrorLog`, and `MainRenderPassSchedule` with `render_mesh_material` |
| `src/gpu.rs` | `Graphics` (wraps `VirtualGpu`) and `Frame` (the in-progress `RenderFrame`) resources |
| `src/egui.rs`, `src/plugin.rs`, `src/utils.rs` | `EguiPlugin`/`EguiCtx`; the `Plugin` trait; `WindowEvent`/`DeviceEvent`, `spawn`, `next_entity_id`, `run_async` |
| `src/camera.rs`, `src/transform.rs` | `Camera` and `Transform` components |
| `src/mesh/`, `src/material/` | `Mesh`/`MeshRef`, `BasicMesh`; `Material`/`MaterialRef`, `BasicMaterial`, `SimpleTexturedMaterial`, `MaterialPipelineStorage` |
| `src/shapes/` | `ShapeBuilder` and the primitive shapes |
| `src/vault/` | Asset system: `Handle`, `AssetContent`, the vault traits, the bindless and atlas texture vaults, `TextureVault` facade, `MaterialVault`, `MeshAssetVault` |
| `shaders/` | Separate crate of Rust shaders. `build.rs` writes WGSL to `shaders/shader_out` (gitignored) |
| `examples/` | `basic`, `shapes`, `textured`, `obj`, `egui`, plus test textures in every format |
| `docs/intelligence.md` | Roadmap for making gearbox agent-friendly |
| `plugins/gearbox/skills/` | User-facing skills (`app`, `rendering`). Keep them in sync, see below |

## How a frame runs

1. `wrapper.rs` gets `RedrawRequested`, applies the `RenderRate` cap, and calls `App::render`.
2. `render` begins a `RenderFrame` and stores it in the `Frame` resource. On the first
   frame it runs the render startup graph, then the render graph, then submits.
   - If there is no `Graphics` yet, or the surface can't give a texture (hidden window,
     zero size), `render` returns `false`. The wrapper then retries after 100 ms natively,
     and right away on the web.
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
cargo build --all-targets --all-features   # also covers auto-egui / dbg-schedules
cargo test                                 # 33 tests, vault loading and asset parsing
cargo run --example shapes                 # also basic, textured, egui; obj needs a model
```

- Run examples from the repo root: asset paths like `./examples/cobblestone.png` are
  relative to the working directory.
- `obj` needs `examples/SM_Prop_Bonsai_01.obj`, which isn't committed (`.gitignore` has
  `*.obj`).
- Web: `trunk serve` builds the example named in `Trunk.toml` into `index.html`'s canvas.

## Verifying visual changes

There is no headless mode or screenshot capture yet (`docs/intelligence.md` covers what
to add).
- **A hidden window draws nothing.** If the window is hidden or covered, the surface
  reports `Occluded`, nothing is drawn, and render bugs stay hidden. A clean run of a
  hidden window proves nothing.
- **Smoke test:** run each example for a few seconds and watch stderr:
  `RUST_LOG=warn perl -e 'alarm 6; exec @ARGV' ./target/debug/examples/shapes`.
  wgpu validation errors panic.
- **Proving something drew:** temporarily count draws in
  `draw_renderables` (`src/schedule.rs`) and print them. Remove the counter afterwards.
- Ask the user to look at the window whenever appearance matters.

## Gotchas

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
- Update the matching user skill in `plugins/gearbox/skills/` (`app` for the
  App/systems/events side, `rendering` for meshes/materials/textures/camera).
- Check skill snippets compile by pasting them into a throwaway `examples/zz_check.rs`,
  running `cargo build --example zz_check`, then deleting the file.
- Run `claude plugin validate .` after editing any plugin file.
