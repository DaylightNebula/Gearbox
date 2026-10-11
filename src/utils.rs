use std::sync::atomic::{AtomicU64, Ordering};

use anarchy::{DynComponent, EntityID, Resource, World};
use derive_more::{Deref, DerefMut};
use mutual::RelaxedMutex;

/// ECS event wrapping a raw `winit::event::WindowEvent`. Sent into the `World` for every
/// window event received by the event loop, so systems can react to input, resizes, etc. Read
/// it with an `Event<WindowEvent>` system parameter.
#[derive(Debug, Clone, Deref, DerefMut)]
pub struct WindowEvent(pub winit::event::WindowEvent);

/// ECS event wrapping a raw `winit::event::DeviceEvent`. Sent into the `World` for every
/// device event received by the event loop, so systems can react to input, resizes, etc. Read
/// it with an `Event<DeviceEvent>` system parameter.
#[derive(Debug, Clone, Deref, DerefMut)]
pub struct DeviceEvent(pub winit::event::DeviceEvent);

/// Resource handing out unique entity ids, see [`next_entity_id`].
#[derive(Debug, Default, Resource)]
pub struct NextEntityId(AtomicU64);

/// Allocates an entity id no other caller of this function has received for `world`. `anarchy`
/// never picks ids itself, so use this (or [`spawn`]) for every entity.
pub fn next_entity_id(world: &World) -> EntityID {
    world.init_resource_with(NextEntityId::default);
    world.resource::<NextEntityId>().unwrap().0.fetch_add(1, Ordering::Relaxed)
}

/// Inserts an entity made of `components` under a fresh id from [`next_entity_id`] and returns
/// that id. `components` must not be empty.
pub fn spawn(world: &World, components: Vec<DynComponent>) -> EntityID {
    let id = next_entity_id(world);
    world.insert(id, components.into_iter().map(RelaxedMutex::new).collect());
    id
}

/// Runs `future` in the background: on a new thread natively, or on the browser's event loop on
/// web. Use it for asset loading and other work that shouldn't block a schedule.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_async<F>(future: F)
    where F: Future<Output = ()> + Send + 'static
{
    std::thread::spawn(move || pollster::block_on(future));
}

/// Runs `future` in the background: on a new thread natively, or on the browser's event loop on
/// web. Use it for asset loading and other work that shouldn't block a schedule.
#[cfg(target_arch = "wasm32")]
pub fn run_async<F>(future: F)
    where F: Future<Output = ()> + 'static
{
    wasm_bindgen_futures::spawn_local(future);
}
