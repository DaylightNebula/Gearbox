//! The [`App`] builder, which runs an [`anarchy`] ECS `World` inside a `winit`/`wgpu`
//! application with a primary update graph and a render graph.

use std::{any::TypeId, collections::HashSet, path::PathBuf, sync::{Arc, atomic::AtomicBool}, time::Duration};

use anarchy::{IntoSystem, Res, Resource, SingleThreadedExecutor, SystemGraph, SystemInstruction, World};
use derive_more::{Deref, DerefMut};
use ::egui::Window;
use magician_vgpu::{RenderFrame, glam::UVec2};
use web_time::Instant;
use winit::event_loop::EventLoop;

use crate::{DEFAULT_FIXED_TIMESTEP, DEFAULT_SCREENSHOT_FRAME, EguiCtx, EnvOptions, ErrorLog, Frame, FrameCount, Graphics, Pacer, Plugin, RenderDeltaTime, RenderRate, Screenshots, UpdateDeltaTime, UpdateRate, save_capture};

/// Resource holding the current size of the app's window (or headless render target), in
/// pixels. Kept in sync automatically whenever the window is resized.
#[derive(Debug, Deref, DerefMut, Resource)]
pub struct WindowDimensions(UVec2);

/// Builder for a `winit`/`wgpu` application backed by an `anarchy` ECS `World`.
///
/// An `App` owns four [`SystemGraph`]s: a startup and an update graph for the primary
/// schedule (ticked at [`UpdateRate`], independent of rendering, on its own threads natively),
/// and a startup and an update graph for the render schedule (run once per `RedrawRequested`
/// on the event loop thread). Startup graphs run once before the first pass of their
/// schedule; render startup waits until [`Graphics`] exists. Plugins, resources, and systems
/// are attached via the builder methods below, then [`App::run`] hands control to the `winit`
/// event loop.
///
/// For tests, CI and AI agents, an app can also run without a window ([`App::headless`]),
/// stop after a set number of frames ([`App::with_frame_limit`]), step time by a fixed amount
/// ([`App::with_fixed_timestep`]) and save frames as PNGs ([`App::with_screenshot`]). The
/// `GEARBOX_HEADLESS`, `GEARBOX_FRAMES`, `GEARBOX_FIXED_TIMESTEP` and `GEARBOX_SCREENSHOT`
/// environment variables turn these on for any app, see [`App::run`].
pub struct App {
    /// Runs once before the first update pass.
    startup: SystemGraph,
    /// Runs every update pass.
    update: SystemGraph,
    /// Runs once before the first render frame.
    render_startup: SystemGraph,
    /// Runs every render frame.
    render: SystemGraph,
    render_exec: SingleThreadedExecutor,
    /// Whether `render_startup` has run.
    render_started: bool,
    render_errors: ErrorLog,
    /// Threads used to run the update graph natively.
    update_threads: usize,
    /// Paces the update graph when it runs from `render` (on web, which has no threads).
    update_pacer: Pacer,
    update_errors: ErrorLog,
    /// Render offscreen at this size, with no window.
    headless: Option<UVec2>,
    /// Stop after this many frames.
    frame_limit: Option<u64>,
    /// Run one update per frame, stepping both delta times by this much.
    fixed_timestep: Option<Duration>,
    /// Set once the frame limit has been rendered.
    pub(crate) finished: bool,
    /// Cleared when the event loop exits, stopping the update thread.
    pub(crate) running: Arc<AtomicBool>,
    added_plugins: HashSet<TypeId>,
    /// The first error from appending a system, returned by [`App::run`].
    build_error: Option<anyhow::Error>,
    /// Whether presentation waits for the display's vertical blank.
    pub(crate) vsync: bool,
    /// Start time of the previous frame, used for [`RenderDeltaTime`].
    last_render: Option<Instant>,
    pub(crate) world: World
}

impl App {
    /// Create a new `App` instance.
    pub fn new() -> Self {
        #[cfg(target_arch = "wasm32")]
        console_error_panic_hook::set_once();

        #[allow(unused_mut)] // used by non-default features
        let mut app = App {
            startup: SystemGraph::new(),
            update: SystemGraph::new(),
            render_startup: SystemGraph::new(),
            render: SystemGraph::new(),
            render_exec: SingleThreadedExecutor::new(),
            render_started: false,
            render_errors: ErrorLog::default(),
            update_threads: 4,
            update_pacer: Pacer::default(),
            update_errors: ErrorLog::default(),
            headless: None,
            frame_limit: None,
            fixed_timestep: None,
            finished: false,
            running: Arc::new(AtomicBool::new(true)),
            added_plugins: HashSet::new(),
            build_error: None,
            vsync: false,
            last_render: None,
            world: World::default()
        };

        app = app.add_resource(WindowDimensions(UVec2::ZERO))
            .add_resource(UpdateRate(60))
            .add_resource(RenderRate(0))
            .add_resource(UpdateDeltaTime::default())
            .add_resource(RenderDeltaTime::default())
            .add_resource(FrameCount(0))
            .add_resource(Screenshots::default());

        #[cfg(feature = "auto-egui")]
        {
            app = app.add_plugin(crate::EguiPlugin);
        }

        #[cfg(feature = "dbg-schedules")]
        {
            app = app.on_render_update(dbg_schedules);
        }

        return app;
    }

    /// The `World` this app runs its systems against.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Add a plugin to this `App`.  The plugins build function is called immeidately.
    /// If a plugin is added twice on accident, uniqueness is guaranteed, so the second
    /// and any future attempts will be ignored.
    pub fn add_plugin<P: Plugin + 'static>(mut self, plugin: P) -> Self {
        if !self.added_plugins.insert(TypeId::of::<P>()) { return self }
        return plugin.build(self);
    }

    /// Run the update graph `rate` times per second, 0 runs it as fast as possible (default 60).
    pub fn with_update_rate(self, rate: u32) -> Self {
        self.add_resource(UpdateRate(rate))
    }

    /// Run the update graph on `threads` threads natively (default 4). Web always uses one.
    pub fn with_update_threads(mut self, threads: usize) -> Self {
        self.update_threads = threads.max(1);
        return self;
    }

    /// Cap rendering to `rate` frames per second, 0 renders as fast as possible (the default).
    pub fn with_render_rate(self, rate: u32) -> Self {
        self.add_resource(RenderRate(rate))
    }

    /// Enable or disable vsync, off by default.
    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.vsync = vsync;
        return self;
    }

    /// Render into a `width` by `height` offscreen texture instead of opening a window. There
    /// are no window or input events. Unless [`with_fixed_timestep`](Self::with_fixed_timestep)
    /// sets another step, a headless app steps time by 1/60 s per frame. Not supported on web.
    pub fn headless(mut self, width: u32, height: u32) -> Self {
        self.headless = Some(UVec2::new(width.max(1), height.max(1)));
        return self;
    }

    /// Exit once `frames` frames have been rendered.
    pub fn with_frame_limit(mut self, frames: u64) -> Self {
        self.frame_limit = Some(frames);
        return self;
    }

    /// Run exactly one update pass per rendered frame, on the render thread, and set both
    /// [`UpdateDeltaTime`] and [`RenderDeltaTime`] to `step`, so every run of the app sees
    /// the same times. [`UpdateRate`] is ignored. Natively, each frame also waits for
    /// background work started with [`run_async`](crate::run_async) (such as asset loads),
    /// so assets arrive on the same frame every run.
    pub fn with_fixed_timestep(mut self, step: Duration) -> Self {
        self.fixed_timestep = Some(step);
        return self;
    }

    /// Save frame `frame` (1 is the first frame) to `path` as a PNG. Systems can request
    /// screenshots too, through the [`Screenshots`] resource.
    pub fn with_screenshot(self, frame: u64, path: impl Into<PathBuf>) -> Self {
        if let Ok(mut screenshots) = self.world.resource_mut::<Screenshots>() {
            screenshots.capture_at(frame, path);
        }
        return self;
    }

    /// Add a resource to this apps world.
    pub fn add_resource<R: Resource>(self, resource: R) -> Self {
        self.world.insert_resource(resource);
        return self;
    }

    /// Add a system to run once before the first update.
    pub fn on_startup<S, M>(self, system: S) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        self.on_startup_with(system, SystemInstruction::default())
    }

    /// Add a system to run once before the first update, ordered by `instruction`.
    pub fn on_startup_with<S, M>(mut self, system: S, instruction: SystemInstruction) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        let result = self.startup.append_system(system, instruction, std::iter::empty());
        self.record(result)
    }

    /// Add a system to run every update.
    pub fn on_update<S, M>(self, system: S) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        self.on_update_with(system, SystemInstruction::default())
    }

    /// Add a system to run every update, ordered by `instruction`. Systems without an order
    /// between them may run at the same time.
    pub fn on_update_with<S, M>(mut self, system: S, instruction: SystemInstruction) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        let result = self.update.append_system(system, instruction, std::iter::empty());
        self.record(result)
    }

    /// Add a system to run once before the first render frame, once [`Graphics`] exists.
    pub fn on_render_startup<S, M>(self, system: S) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        self.on_render_startup_with(system, SystemInstruction::default())
    }

    /// Add a system to run once before the first render frame, ordered by `instruction`.
    pub fn on_render_startup_with<S, M>(mut self, system: S, instruction: SystemInstruction) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        let result = self.render_startup.append_system(system, instruction, std::iter::empty());
        self.record(result)
    }

    /// Add a system to run every render frame.
    pub fn on_render_update<S, M>(self, system: S) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        self.on_render_update_with(system, SystemInstruction::default())
    }

    /// Add a system to run every render frame, ordered by `instruction`. The egui pass is open
    /// for every system in the `Normal` pin (the default).
    pub fn on_render_update_with<S, M>(mut self, system: S, instruction: SystemInstruction) -> Self
        where S: IntoSystem<(), (), M> + 'static
    {
        let result = self.render.append_system(system, instruction, std::iter::empty());
        self.record(result)
    }

    /// Keep the first error from appending a system, so `run` can return it.
    fn record(mut self, result: anyhow::Result<()>) -> Self {
        if let Err(err) = result { self.build_error.get_or_insert(err); }
        return self;
    }

    /// Start the primary schedule and hand control over to the `winit` event loop. This
    /// creates the window, drives resize/redraw events into the `App`, and blocks until the
    /// event loop exits, or the frame limit is reached. A headless app runs its own loop
    /// instead.
    ///
    /// These environment variables override the builder settings, so any app can be run
    /// headless or captured without code changes:
    ///
    /// | Variable | Effect |
    /// | --- | --- |
    /// | `GEARBOX_HEADLESS` | `WIDTHxHEIGHT` (or `1` for 800x600): run [`headless`](Self::headless) |
    /// | `GEARBOX_FRAMES` | [`with_frame_limit`](Self::with_frame_limit) |
    /// | `GEARBOX_FIXED_TIMESTEP` | seconds: [`with_fixed_timestep`](Self::with_fixed_timestep) |
    /// | `GEARBOX_SCREENSHOT` | save the last frame to this path, with a 60 frame limit unless `GEARBOX_FRAMES` is set |
    ///
    /// # Errors
    ///
    /// Returns the first error from appending a system (a cycle or a contradicting pin), an
    /// invalid `GEARBOX_*` variable, a headless app's GPU failing to start or render, or an
    /// error from the event loop.
    pub fn run(mut self) -> anyhow::Result<()> {
        if let Some(err) = self.build_error.take() { return Err(err) }
        self.apply_env(EnvOptions::from_env()?);
        if self.headless.is_some() && self.fixed_timestep.is_none() {
            self.fixed_timestep = Some(DEFAULT_FIXED_TIMESTEP);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            // `try_init`, as tests may run several apps in one process
            let _ = env_logger::try_init();
            if let Some(size) = self.headless {
                return self.run_headless(size);
            }
            if !self.update_in_render() {
                self.spawn_update_thread();
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = console_log::init_with_level(log::Level::Info);
            if self.headless.is_some() {
                anyhow::bail!("Headless apps aren't supported on the web");
            }
        }

        let event_loop = EventLoop::with_user_event().build()?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::wrapper::AppWrapper;

            let mut wrapper = AppWrapper { app: self, render_pacer: Pacer::default() };
            event_loop.run_app(&mut wrapper)?;
        }
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::EventLoopExtWebSys;
            use crate::wrapper::AppWrapper;

            let wrapper = AppWrapper { app: self, render_pacer: Pacer::default() };
            event_loop.spawn_app(wrapper);
        }

        Ok(())
    }

    /// Apply the `GEARBOX_*` environment variables over the builder settings.
    fn apply_env(&mut self, env: EnvOptions) {
        if let Some(size) = env.headless { self.headless = Some(size); }
        if let Some(frames) = env.frames { self.frame_limit = Some(frames); }
        if let Some(step) = env.fixed_timestep { self.fixed_timestep = Some(step); }
        if let Some(path) = env.screenshot {
            let frame = *self.frame_limit.get_or_insert(DEFAULT_SCREENSHOT_FRAME);
            if let Ok(mut screenshots) = self.world.resource_mut::<Screenshots>() {
                screenshots.capture_at(frame, path);
            }
        }
    }

    /// Whether the update graph runs from `render` rather than on its own thread: on web,
    /// which has no threads, and with a fixed time step, so updates stay in step with frames.
    fn update_in_render(&self) -> bool {
        cfg!(target_arch = "wasm32") || self.fixed_timestep.is_some()
    }

    /// Render offscreen at `size` until the frame limit, with no window or event loop.
    #[cfg(not(target_arch = "wasm32"))]
    fn run_headless(mut self, size: UVec2) -> anyhow::Result<()> {
        let vgpu = pollster::block_on(magician_vgpu::VirtualGpu::new_headless(size.x, size.y))?;
        self.world.insert_resource(Graphics(vgpu));
        self.world.insert_resource(Frame::default());
        configure_surface(&self.world, size.x, size.y);

        let mut pacer = Pacer::default();
        let result = loop {
            let rate = self.world.resource::<RenderRate>().map(|rate| rate.0).unwrap_or(0);
            if let Some(deadline) = pacer.too_early(rate) {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                continue;
            }
            match self.render() {
                Ok(_) if self.finished => break Ok(()),
                Ok(_) => {}
                Err(err) => break Err(err),
            }
        };
        self.running.store(false, std::sync::atomic::Ordering::Release);
        result
    }

    /// Move the startup and update graphs onto a thread that runs startup once, then runs
    /// update at [`UpdateRate`] until the app exits.
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn_update_thread(&mut self) {
        use std::sync::atomic::Ordering;
        use anarchy::MultiThreadedExecutor;

        let world = self.world.clone();
        let startup = std::mem::take(&mut self.startup);
        let update = std::mem::take(&mut self.update);
        let running = Arc::clone(&self.running);
        let threads = self.update_threads;

        std::thread::spawn(move || {
            let mut errors = ErrorLog::default();
            errors.report("Startup", SingleThreadedExecutor::new().run(&world, &startup));

            let exec = MultiThreadedExecutor::new();
            // cleared, so each `run` call is a single pass and the pacing below decides when
            // the next one starts
            let single_pass = AtomicBool::new(false);
            let mut pacer = Pacer::default();
            while running.load(Ordering::Acquire) {
                let rate = world.resource::<UpdateRate>().map(|rate| rate.0).unwrap_or(0);
                if let Some(deadline) = pacer.too_early(rate) {
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                    continue;
                }
                set_delta::<UpdateDeltaTime>(&world, pacer.start_pass());
                errors.report("Update", exec.run(&world, &update, threads, &single_pass));
            }
        });
    }

    /// Run one update pass from the render loop: every frame with a fixed time step,
    /// otherwise whenever [`UpdateRate`] says one is due.
    fn tick_update(&mut self) {
        let delta = match self.fixed_timestep {
            Some(step) => step.as_secs_f32(),
            None => {
                let rate = self.world.resource::<UpdateRate>().map(|rate| rate.0).unwrap_or(0);
                if self.update_pacer.too_early(rate).is_some() { return }
                self.update_pacer.start_pass()
            }
        };

        // startup is taken on the first pass, so it only runs once
        let startup = std::mem::take(&mut self.startup);
        let result = self.render_exec.run(&self.world, &startup);
        self.update_errors.report("Startup", result);

        set_delta::<UpdateDeltaTime>(&self.world, delta);
        let result = self.render_exec.run(&self.world, &self.update);
        self.update_errors.report("Update", result);
    }

    /// Reconfigure the GPU surface and [`WindowDimensions`] resource after a window resize.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        configure_surface(&self.world, width, height);
    }

    /// Run one frame: begin a [`RenderFrame`], run the render graphs against it, submit the
    /// frame, and record [`RenderDeltaTime`]. Returns false if the frame was skipped, because
    /// [`Graphics`] doesn't exist yet or the surface can't be drawn to (e.g. the window is
    /// hidden), in which case nothing has requested the next redraw.
    pub(crate) fn render(&mut self) -> anyhow::Result<bool> {
        // with a fixed time step, background loads finish before the frame that can use
        // them, so every run sees each asset arrive on the same frame
        #[cfg(not(target_arch = "wasm32"))]
        if self.fixed_timestep.is_some() {
            crate::wait_for_async_tasks();
        }

        // a fixed time step updates only on frames that render, so frame N always follows
        // N updates; otherwise updates keep their own pace while frames can't render
        if self.update_in_render() && self.fixed_timestep.is_none() {
            self.tick_update();
        }

        // get start time and vgpu
        let start = Instant::now();
        let Ok(vgpu) = self.world.resource::<Graphics>()
            else { return Ok(false) };

        // start and insert `Frame`
        {
            let Some(frame) = RenderFrame::begin(&vgpu)?
                else { return Ok(false) };
            self.world.resource_mut::<Frame>()?.store(frame);
        }
        let frame_number = {
            let mut count = self.world.resource_mut::<FrameCount>()?;
            count.0 += 1;
            count.0
        };

        // record delta time as the interval between frame starts, so it covers any time spent
        // waiting on the render rate cap or vsync and not just the work done this frame
        let previous = self.last_render.replace(start).unwrap_or(start);
        let deltatime = match self.fixed_timestep {
            Some(step) => step.as_secs_f32(),
            None => start.duration_since(previous).as_secs_f32(),
        };
        set_delta::<RenderDeltaTime>(&self.world, deltatime);
        if self.fixed_timestep.is_some() {
            self.tick_update();
        }

        if !self.render_started {
            self.render_started = true;
            let result = self.render_exec.run(&self.world, &self.render_startup);
            self.render_errors.report("Render startup", result);
        }
        let result = self.render_exec.run(&self.world, &self.render);
        self.render_errors.report("Render", result);

        let frame = self.world.resource_mut::<Frame>()?
            .take()
            .expect("Frame lost");

        // finalize frame, capturing it if a screenshot is due
        let due = self.world.resource_mut::<Screenshots>()?.take_due(frame_number);
        if due.is_empty() {
            frame.submit(&vgpu);
        } else {
            match frame.submit_and_capture(&vgpu) {
                Ok(capture) => for path in due {
                    match save_capture(&capture, &path) {
                        Ok(()) => log::info!("Saved frame {frame_number} to {}", path.display()),
                        Err(err) => log::error!("{err:#}"),
                    }
                },
                Err(err) => log::error!("Failed to capture frame {frame_number}: {err:#}"),
            }
        }

        if self.frame_limit.is_some_and(|limit| frame_number >= limit) {
            self.finished = true;
        }
        Ok(true)
    }
}

/// Configure the GPU surface for a `width` by `height` window and update [`WindowDimensions`].
/// Does nothing while either is 0 or before [`Graphics`] exists.
pub(crate) fn configure_surface(world: &World, width: u32, height: u32) {
    if width == 0 || height == 0 { return }
    let Ok(mut vgpu) = world.resource_mut::<Graphics>() else { return };

    vgpu.resize(width, height);
    drop(vgpu);
    if let Ok(mut dimensions) = world.resource_mut::<WindowDimensions>() {
        dimensions.0 = UVec2::new(width, height);
    }
}

/// Write `seconds` into the delta time resource `D`.
fn set_delta<D: Resource + anarchy::ResourceMeta + std::ops::DerefMut<Target = f32>>(world: &World, seconds: f32) {
    if let Ok(mut delta) = world.resource_mut::<D>() { **delta = seconds; }
}

/// Shows the update and render rates in an egui window, added by the `dbg-schedules` feature.
#[allow(dead_code)]
fn dbg_schedules(
    egui: Res<EguiCtx>,
    update: Res<UpdateDeltaTime>,
    render: Res<RenderDeltaTime>
) {
    Window::new("Schedules Debug").show(&egui.context, |ui| {
        ui.label("FPS Counters");
        ui.label(format!(" - Update -> {} FPS", 1.0 / update.0));
        ui.label(format!(" - Render -> {} FPS", 1.0 / render.0));
    });
}
