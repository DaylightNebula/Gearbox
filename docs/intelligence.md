# Making Gearbox agent-friendly

An AI agent can only build what it can observe and verify. While upgrading Gearbox to
Anarchy v2, an agent hit three problems that came from Gearbox itself rather than the task:
- **It couldn't see output.** The window was hidden and screen capture was blocked, so it
  had no way to see what was drawn.
- **It needed temporary logging.** Debug logging went in and out four times just to tell
  whether anything rendered.
- **A crash went unnoticed.** The texture vaults crashed on the first frame, but nothing
  showed it until frames were actually drawn.

This document lists the changes that would let agents work with Gearbox and build things
in it confidently, roughly in order of value.

## 1. Let an agent see the output

The highest priority. Without this, every visual change is a guess.

*Done:* headless mode, screenshots and frame limits with a fixed time step are in. See
"Headless runs, screenshots and frame limits" in the README.

- **Headless mode.** Run an `App` with no window, rendering into an offscreen texture,
  enabled by something like `App::headless(width, height)` or an environment variable.
  This works in CI, over SSH and in sandboxed agent sessions.
- **Screenshots to file.** Read a frame back from the GPU and save it as a PNG, for
  example "write frame 60 to `out.png`". An agent can then open the image and check it.
- **Stop after N frames.** Exit cleanly after a set number of frames, ideally with a
  fixed time step, so each run gives the same result.
- **Golden-image tests.** Combine the three: render each example headless, compare it
  with a saved reference image within a tolerance, and fail the test on a mismatch.

## 2. Make problems show up instead of failing silently

- **Frame stats.** A resource that counts draws per frame, entities skipped and why
  (material or mesh not loaded yet), textures pending and uploaded, and pipelines built.
  Print it on exit or on request. This replaces ad-hoc logging for "did anything render?".
- **World dump.** Every component and resource already implements `Debug`, so dumping
  all entities to text or JSON is cheap. It lets an agent check the world's state without
  guessing.
- **Errors that name their type.** Anarchy's missing-resource error just says "No resource
  found". Including the resource's type name turns a guessing game into a one-line fix.
- **Fallible systems.** Systems can't return errors in Anarchy v2, so agents end up
  writing `unwrap()` everywhere. Accepting systems that return `anyhow::Result` (an
  Anarchy change) would let errors flow to the executor and get logged.

## 3. Teach the API

- **A Gearbox skill.** *Done:* the `app` and `rendering` skills are in
  `plugins/gearbox/skills`, and the contributor skill is in `.claude/skills/gearbox`. An
  agent plugin modelled on Anarchy's `plugins/anarchy/skills`, covering:
  - the `App` lifecycle (startup, update, render startup, render)
  - which `SystemPin` to use for what
  - `spawn` and entity ids
  - asset loading through `TextureVault::current` and the async vaults
  - writing materials and meshes
- **Gotchas the skill should spell out:**
  - `#[derive(Component)]` and `#[derive(Resource)]` generate code that names
    `::anarchy`, so an app using them must depend on `anarchy` directly.
  - Systems running inside the main render pass must not take
    `Res<MainRenderPassSchedule>` or `ResMut<MainRenderPassSchedule>`: the schedule is
    locked while it runs, so they deadlock.
  - The egui pass is only open for render systems in the `Normal` pin.
  - Texture vaults bind nothing until their first texture has uploaded, so textured
    entities are skipped for the first few frames.
- **A `CLAUDE.md` for the repo.** How to build, test and run the examples, and which
  checks to pass before calling a change done.
- **A `gearbox::prelude`.** One import with everything commonly needed, so agents don't
  have to guess module paths.
- **Doc examples that compile.** One doctest per main type, run by `cargo test`, so the
  docs can't fall out of date with the code.

## 4. Describe scenes as data

- **Scene files.** Load entities, meshes, materials and the camera from a RON or JSON
  file, with hot reload. An agent could build and adjust scenes without recompiling, and
  check each change with a screenshot.
- **Live control.** A more ambitious follow-up: a dev-mode command channel (a local
  socket or an MCP server) for querying the world, spawning or changing entities, and
  taking screenshots while the app runs.

## 5. Make the examples dependable smoke tests

- **Self-contained examples.** Every example should run from a clean checkout. `obj`
  currently needs `examples/SM_Prop_Bonsai_01.obj`, which isn't committed because
  `.gitignore` excludes `*.obj`.
- **One smoke-test command.** A script that runs every example headless for N frames
  and fails on any panic or logged error. It would have caught the texture vault crash
  immediately.

## Suggested order

1. ~~Headless mode, screenshots and exiting after N frames.~~ Done.
2. The smoke-test script.
3. `CLAUDE.md` and `gearbox::prelude` (the skills are done).
4. Diagnostics: frame stats, world dump and better error messages.
5. Scene files.
6. The live control channel.
