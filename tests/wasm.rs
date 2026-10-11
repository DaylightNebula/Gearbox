//! Tests that run inside WebAssembly, in Node, through `wasm-bindgen-test`. They cover code
//! that compiles for the web but could still panic there at runtime, such as anything
//! reading the clock. Run them with `scripts/test-wasm.sh`.
#![cfg(target_arch = "wasm32")]

use std::{cell::Cell, rc::Rc};

use anarchy::Resource;
use gearbox::anarchy::{Event, ResMut, SingleThreadedExecutor, SystemGraph, SystemInstruction, World, after};
use gearbox::{AssetContent, Screenshots, Transform, next_entity_id, run_async, spawn};
use wasm_bindgen_test::wasm_bindgen_test;

#[derive(Clone, Debug, PartialEq)]
struct Ping(u32);

#[derive(Debug, Default, Resource)]
struct Log(Vec<String>);

/// Runs every system in `graph` once.
fn run_once(world: &World, graph: &SystemGraph) {
    SingleThreadedExecutor::new().run(world, graph).unwrap();
}

/// Lets the browser or Node event loop run queued tasks, such as `spawn_local` futures.
async fn yield_to_event_loop() {
    let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

/// Events timestamp themselves when sent, which panics on wasm if it uses
/// `std::time::Instant`.
#[wasm_bindgen_test]
fn events_send_and_read() {
    fn read(pings: Event<Ping>, mut log: ResMut<Log>) {
        for ping in pings.read() { log.0.push(format!("ping {}", ping.0)); }
    }

    let world = World::default();
    world.insert_resource(Log::default());
    world.send_event(Ping(1));
    world.send_event(Ping(2));

    let mut graph = SystemGraph::new();
    graph.append_system(read, SystemInstruction::default(), std::iter::empty()).unwrap();
    run_once(&world, &graph);

    assert_eq!(world.resource::<Log>().unwrap().0, vec!["ping 1", "ping 2"]);
}

#[wasm_bindgen_test]
fn ordered_systems_run_in_order() {
    fn first(mut log: ResMut<Log>) { log.0.push("first".into()); }
    fn second(mut log: ResMut<Log>) { log.0.push("second".into()); }

    let world = World::default();
    world.insert_resource(Log::default());
    let mut graph = SystemGraph::new();
    graph.append_system(second, after(first), std::iter::empty()).unwrap();
    graph.append_system(first, SystemInstruction::default(), std::iter::empty()).unwrap();
    run_once(&world, &graph);

    assert_eq!(world.resource::<Log>().unwrap().0, vec!["first", "second"]);
}

#[wasm_bindgen_test]
fn spawn_hands_out_unique_ids() {
    let world = World::default();
    let a = spawn(&world, vec![Box::new(Transform::identity())]);
    let b = spawn(&world, vec![Box::new(Transform::identity())]);
    assert_ne!(a, b);
    assert_ne!(next_entity_id(&world), b);
}

#[wasm_bindgen_test]
fn screenshot_requests_queue() {
    let mut screenshots = Screenshots::default();
    assert!(screenshots.is_empty());
    screenshots.capture("frame.png");
    assert!(!screenshots.is_empty());
}

#[wasm_bindgen_test]
async fn in_memory_asset_content_resolves() {
    let bytes = AssetContent::Binary(Box::new([1, 2, 3])).into_bytes().await.unwrap();
    assert_eq!(&*bytes, &[1, 2, 3]);
    let text = AssetContent::Content("hi".into()).into_bytes().await.unwrap();
    assert_eq!(&*text, b"hi");
}

#[wasm_bindgen_test]
async fn url_assets_fail_cleanly_on_the_web() {
    assert!(AssetContent::Url("https://example.com/a.png".into()).into_bytes().await.is_err());
}

/// `run_async` hands futures to the JS event loop on the web, where asset loads run.
#[wasm_bindgen_test]
async fn run_async_runs_on_the_event_loop() {
    let done = Rc::new(Cell::new(false));
    let flag = Rc::clone(&done);
    run_async(async move { flag.set(true) });

    for _ in 0..10 {
        if done.get() { return }
        yield_to_event_loop().await;
    }
    panic!("the run_async future never ran");
}
