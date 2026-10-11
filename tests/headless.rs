//! Runs whole apps headless: frame limits, fixed time steps, screenshots, and checks on the
//! pixels they render. Each test skips itself on machines without a GPU adapter.
#![cfg(not(target_arch = "wasm32"))]

use std::{path::PathBuf, sync::atomic::{AtomicU32, Ordering}, time::Duration};

use anarchy::Resource;
use gearbox::anarchy::{Res, ResMut, World};
use gearbox::glam::{Quat, Vec3, Vec4};
use gearbox::*;

const CLEAR: Vec4 = Vec4::new(1.0, 0.0, 0.0, 1.0);
const CLEAR_RGBA: [u8; 4] = [255, 0, 0, 255];

/// Runs `app`, returning false (and printing why) if the machine has no GPU adapter.
fn run(app: App) -> bool {
    match app.run() {
        Ok(()) => true,
        Err(err) if format!("{err}").contains("No GPU adapter") => {
            eprintln!("skipping, no GPU: {err}");
            false
        }
        Err(err) => panic!("app failed: {err:#}"),
    }
}

/// A screenshot path unique to this test run.
fn screenshot_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("gearbox_test_{}_{name}.png", std::process::id()))
}

/// Loads a screenshot as tightly packed RGBA.
fn load(path: &PathBuf) -> image::RgbaImage {
    let image = image::open(path).unwrap_or_else(|err| panic!("no screenshot at {}: {err}", path.display())).to_rgba8();
    let _ = std::fs::remove_file(path);
    image
}

/// An app that renders [`CLEAR`] through a camera looking at the origin from z = 6.
fn scene(width: u32, height: u32) -> App {
    App::new()
        .headless(width, height)
        .add_plugin(GearboxRenderPlugin)
        .add_resource(ClearColor(CLEAR))
        .on_render_startup(spawn_camera)
}

fn spawn_camera(world: World) {
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 0.0, 6.0), Quat::IDENTITY, Vec3::ONE)),
        Box::new(Camera::default()),
    ]);
}

#[test]
fn runs_exactly_the_frame_limit() {
    static RENDERS: AtomicU32 = AtomicU32::new(0);
    fn count() { RENDERS.fetch_add(1, Ordering::Relaxed); }

    let app = App::new().headless(16, 16).with_frame_limit(5).on_render_update(count);
    let world = app.world().clone();
    if !run(app) { return }

    assert_eq!(RENDERS.load(Ordering::Relaxed), 5);
    assert_eq!(*world.resource::<FrameCount>().unwrap(), FrameCount(5));
}

#[derive(Debug, Default, Resource)]
struct Steps { startups: u32, updates: Vec<f32>, frames: Vec<u64> }

#[test]
fn fixed_timestep_runs_one_update_per_frame() {
    fn startup(mut steps: ResMut<Steps>) { steps.startups += 1; }
    fn update(mut steps: ResMut<Steps>, dt: Res<UpdateDeltaTime>, frame: Res<FrameCount>) {
        steps.updates.push(dt.0);
        steps.frames.push(frame.0);
    }

    let step = Duration::from_millis(50);
    let app = App::new()
        .headless(16, 16)
        .with_frame_limit(4)
        .with_fixed_timestep(step)
        .add_resource(Steps::default())
        .on_startup(startup)
        .on_update(update);
    let world = app.world().clone();
    if !run(app) { return }

    let steps = world.resource::<Steps>().unwrap();
    assert_eq!(steps.startups, 1);
    assert_eq!(steps.updates, vec![0.05; 4]);
    assert_eq!(steps.frames, vec![1, 2, 3, 4], "each update runs within its own frame");
    assert_eq!(world.resource::<RenderDeltaTime>().unwrap().0, 0.05);
}

#[test]
fn headless_apps_default_to_a_sixtieth_of_a_second() {
    let app = App::new().headless(16, 16).with_frame_limit(2);
    let world = app.world().clone();
    if !run(app) { return }

    assert_eq!(world.resource::<RenderDeltaTime>().unwrap().0, DEFAULT_FIXED_TIMESTEP.as_secs_f32());
}

#[test]
fn screenshot_captures_the_clear_color() {
    let path = screenshot_path("clear");
    if !run(scene(32, 24).with_frame_limit(2).with_screenshot(2, &path)) { return }

    let image = load(&path);
    assert_eq!(image.dimensions(), (32, 24));
    assert!(image.pixels().all(|pixel| pixel.0 == CLEAR_RGBA));
}

#[test]
fn renders_a_cube_through_the_camera() {
    fn cube(world: World, graphics: Res<Graphics>) {
        let mesh = ShapeBuilder::new()
            .cube(Vec3::ZERO, Quat::IDENTITY, 2.0, 2.0, 2.0)
            .build_mesh(&graphics)
            .unwrap();
        spawn(&world, vec![
            Box::new(Transform::identity()),
            Box::new(MaterialRef::new(BasicMaterial::new(Vec4::new(0.0, 1.0, 0.0, 1.0)))),
            Box::new(MeshRef::new(mesh)),
        ]);
    }

    let path = screenshot_path("cube");
    if !run(scene(64, 64).on_render_startup(cube).with_frame_limit(1).with_screenshot(1, &path)) { return }

    let image = load(&path);
    assert_eq!(image.get_pixel(32, 32).0, [0, 255, 0, 255], "the cube covers the center");
    assert_eq!(image.get_pixel(0, 0).0, CLEAR_RGBA, "the corners are clear");
}

#[test]
fn systems_can_request_screenshots() {
    #[derive(Debug, Resource)]
    struct ShotPath(PathBuf);

    fn shoot_on_third_frame(frame: Res<FrameCount>, path: Res<ShotPath>, mut screenshots: ResMut<Screenshots>) {
        if frame.0 == 3 { screenshots.capture(path.0.clone()); }
    }

    let path = screenshot_path("from_system");
    let app = scene(8, 8)
        .with_frame_limit(4)
        .add_resource(ShotPath(path.clone()))
        .on_render_update(shoot_on_third_frame);
    if !run(app) { return }

    assert_eq!(load(&path).get_pixel(4, 4).0, CLEAR_RGBA);
}

#[test]
fn textures_load_by_the_frame_after_they_are_requested() {
    fn textured_cube(world: World, graphics: Res<Graphics>, materials: Res<MaterialVault>) {
        let vault = TextureVault::current(&world, &graphics).unwrap();
        let texture = vault.load(&world, AssetContent::LocalPath("examples/cobblestone.png".into()), TextureType::PNG).unwrap();
        let material = materials.allocate(1).unwrap();
        materials.store(&world, material.clone(), Box::new(SimpleTexturedMaterial::new(texture)));

        let mesh = ShapeBuilder::new()
            .cube(Vec3::ZERO, Quat::IDENTITY, 2.0, 2.0, 2.0)
            .build_mesh(&graphics)
            .unwrap();
        spawn(&world, vec![
            Box::new(Transform::identity()),
            Box::new(MaterialRef::new(material)),
            Box::new(MeshRef::new(mesh)),
        ]);
    }

    let first = screenshot_path("texture_frame1");
    let second = screenshot_path("texture_frame2");
    let app = scene(64, 64)
        .on_render_startup(textured_cube)
        .with_frame_limit(2)
        .with_screenshot(1, &first)
        .with_screenshot(2, &second);
    if !run(app) { return }

    assert_eq!(load(&first).get_pixel(32, 32).0, CLEAR_RGBA, "frame 1 requests the texture, so nothing draws yet");
    let center = load(&second).get_pixel(32, 32).0;
    assert_ne!(center, CLEAR_RGBA, "the fixed time step waits for the load, so frame 2 draws it");
    assert!(center[0].abs_diff(center[1]) < 40 && center[1].abs_diff(center[2]) < 40, "cobblestone is grey, got {center:?}");
}

#[test]
fn egui_draws_without_a_window() {
    fn panel(egui: Res<EguiCtx>) {
        use gearbox::egui::egui::{Color32, LayerId, Rect, pos2};
        egui.context.layer_painter(LayerId::background())
            .rect_filled(Rect::from_min_max(pos2(16.0, 16.0), pos2(48.0, 48.0)), 0.0, Color32::BLUE);
    }

    let path = screenshot_path("egui");
    let app = scene(64, 64)
        .add_plugin(EguiPlugin)
        .on_render_update(panel)
        .with_frame_limit(2)
        .with_screenshot(2, &path);
    if !run(app) { return }

    let image = load(&path);
    assert_eq!(image.get_pixel(32, 32).0, [0, 0, 255, 255], "egui painted the square");
    assert_eq!(image.get_pixel(2, 2).0, CLEAR_RGBA, "and nothing outside it");
}
