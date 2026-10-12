use std::sync::atomic::Ordering;

use anarchy::{Query, Res, ResMut, SystemPin, pin};
use crate::{App, Plugin, Transform, UpdateDeltaTime};
use log::warn;
use crate::glam::EulerRot;
use mutual::CowData;
use rapier3d::{control::{CharacterAutostep, KinematicCharacterController}, na::{Unit, Vector3}, prelude::{QueryFilter, RigidBodyBuilder}};

pub mod character;
pub mod colliders;
pub mod components;
pub mod engine;
pub mod queries;

pub use character::*;
pub use colliders::*;
pub use components::*;
pub use engine::*;
pub use queries::*;
pub use rapier3d as rapier3d;

/// `crate::Plugin` that wires `PhysicsEngine` and `PhysicsQueries` into the app as
/// resources and registers `update_engine_side` to run every tick, keeping rapier
/// rigidbodies, colliders, and character controllers in sync with `PhysicsObject`
/// and `PhysicsCharacter` components.
pub struct PhysicsPlugin;
impl Plugin for PhysicsPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(PhysicsEngine::new())
            .add_resource(PhysicsQueries::default())
            .on_update_with(update_engine_side, pin(SystemPin::End))
    }
}

/// Per-tick system that drives the physics engine: creates or refreshes rapier
/// rigidbodies/colliders and character controllers for entities with a
/// `PhysicsObject`, applies pending velocity/impulses/forces/torques, moves
/// kinematic characters, writes rapier's resulting transform back onto the ECS
/// `Transform`, removes bodies for entities that no longer exist, executes any
/// pending `PhysicsQueries`, and steps the `PhysicsEngine`.
fn update_engine_side(
    mut character_objects: Query<(&'static mut Transform, &'static PhysicsObject, &'static mut PhysicsCharacter)>,
    mut objects: Query<(&'static mut Transform, &'static PhysicsObject)>,
    mut engine: ResMut<PhysicsEngine>,
    queries: Res<PhysicsQueries>,
    delta_time: Res<UpdateDeltaTime>
) {
    let delta_time = delta_time.0.min(MAX_STEP);
    let mut bodies = engine.rigid_body_map.clone();
    let mut characters = engine.character_ids();

    // characters first: anarchy queries only match entities holding every listed component,
    // so characters and plain bodies are walked by separate queries
    let mut character_entities = Vec::new();
    while let Some((entity_id, (mut transform, object, mut character))) = character_objects.next().unwrap() {
        character_entities.push(entity_id);
        let force_refresh = object.refresh_rapier().load(Ordering::Acquire);

        characters.retain(|id| id != object.physics_id());

        // create controller if needed, skipping objects already known to have no shape
        let settings_changed = character.take_controller_dirty();
        let rebuild = force_refresh || settings_changed
            || (!engine.has_character(*object.physics_id()) && !object.using_empty_collider().load(Ordering::Acquire));
        if rebuild {
            let up = Unit::new_normalize(Vector3::new(character.up().x, character.up().y, character.up().z));
            object.refresh_rapier().store(false, Ordering::Release);

            // create controller
            let mut kinematic = KinematicCharacterController::default();
            character.slope_config().map(|slope_config| {
                kinematic.min_slope_slide_angle = *slope_config.min_slope_angle_rad();
                kinematic.max_slope_climb_angle = *slope_config.max_slope_angle_rad();
            });
            kinematic.autostep = character.step_config().map(|config| CharacterAutostep { 
                max_height: config.max_height().to_length(), 
                min_width: config.min_width().to_length(), 
                include_dynamic_bodies: *config.include_dynamic_bodies()
            });
            character.collider_expansion().map(|length| kinematic.offset = length.to_length());
            kinematic.snap_to_ground = character.snap_to_ground().map(|length| length.to_length());
            kinematic.up = up;

            // create shape and save
            // a collider that fails to convert is treated as empty until it's changed
            engine.remove(*object.physics_id());
            let shape = object.collider().get_ref().into_rapier_shared_shape()
                .inspect_err(|err| warn!("Failed to convert character collider to a rapier shape! {err:?}"));
            let Ok(Some(shape)) = shape else { object.using_empty_collider().store(true, Ordering::Release); continue };
            object.using_empty_collider().store(false, Ordering::Release);
            let shape = shape.0.clone_dyn();
            engine.insert_character(*object.physics_id(), kinematic, shape);

            // stop here as nothing has happened yet
            continue;
        }

        // update character
        let (_scale, rotation, position) = transform.as_matix().to_scale_rotation_translation();
        let result = engine.move_character(
            *object.physics_id(),
            position, rotation,
            character.target_movement() * delta_time,
            *object.mass().get_ref(),
            delta_time
        );
        if let Some(result) = result {
            transform.translation += result.translation;
            character.set_is_grounded(result.is_grounded);
            character.set_is_sliding(result.is_sliding);
        }
    }

    while let Some((entity_id, (mut transform, object))) = objects.next().unwrap() {
        if character_entities.contains(&entity_id) { continue }
        let force_refresh = object.refresh_rapier().load(Ordering::Acquire);

        let handle = bodies.remove_by_left(object.physics_id());
        let is_collider_empty = object.using_empty_collider().load(Ordering::Acquire);

        // create or refresh collider in engine
        if (handle.is_none() && !is_collider_empty) || force_refresh {
            object.refresh_rapier().store(false, Ordering::Release);

            // compute position and rotation
            let (_scale, rotation, position) = transform.as_matix().to_scale_rotation_translation();
            let position = Vector3::new(position.x, position.y, position.z);
            let rotation = rotation.to_euler(EulerRot::XYZ);
            let rotation = Vector3::new(rotation.0, rotation.1, rotation.2);

            // create rigidbody
            let rigidbody = if *object.is_fixed().get_ref() {
                RigidBodyBuilder::fixed()
                    .translation(position)
                    .rotation(rotation)
                    .can_sleep(true)
                    .build()
            } else {
                RigidBodyBuilder::dynamic()
                    .translation(position)
                    .rotation(rotation)
                    .can_sleep(true)
                    .build()
            };

            // drop the old body first, so a collider changed to an empty one leaves none behind
            if force_refresh { engine.remove(*object.physics_id()); }

            // create collider
            let collider = object.collider().get_ref().into_rapier_collider();
            let Ok(collider) = collider else {
                warn!("Failed to convert collider to rapier collider! {:?}", collider.err().unwrap());
                // treated as empty until the collider is changed, rather than warning every tick
                object.using_empty_collider().store(true, Ordering::Release);
                continue;
            };
            let Some(collider) = collider else { object.using_empty_collider().store(true, Ordering::Release); continue };
            let collider = collider.mass(*object.mass().get_ref()).build();
            object.using_empty_collider().store(false, Ordering::Release);

            engine.insert(*object.physics_id(), entity_id, collider, rigidbody);

            continue; // no updates to take so stop here
        }

        // get handle, skip if still cannot be found (may not be loaded yet)
        if handle.is_none() { continue }
        let (_, handle) = handle.unwrap();

        if let Some(snapshot) = engine.snapshot_by_rb_handle(handle) {
            // if snapshot.velocity.length_squared() < 0.001 { return }
            let movement = transform.translation().distance(snapshot.translation);
            let diff = (movement - (snapshot.velocity.length() * delta_time)).abs();
            if diff < 0.5 {
                transform.set_translation(snapshot.translation);
                transform.set_rotation(snapshot.rotation);
            } else {
                engine.set_rb_position_rotation(handle, *transform.translation(), *transform.rotation());
            }

            object.last_snapshot().set(Some(snapshot));
        }

        // apply velocity
        let velocity = object.next_velocity().swap(&CowData::null());
        if let Some(velocity) = *velocity {
            engine.apply_velocity(handle, velocity);
        }

        // apply forces and torques
        engine.apply_impulses_torques_forces(
            handle,
            object.impulses().drain().map(|a| a.clone()).collect(),
            object.torque_impulses().drain().map(|a| a.clone()).collect(),
            object.forces().iter().map(|a| a.clone().2).collect(),
            object.torques().iter().map(|a| a.clone().2).collect()
        );
    }

    // anything left in the bodies or characters lists, assume has been removed from outside
    bodies.iter().for_each(|(id, _)| engine.remove(*id));
    characters.into_iter().for_each(|id| engine.remove(id));

    // update queries
    let query_pipeline = engine.create_query_pipeline(QueryFilter::new());
    queries.execute_queries(&engine, &query_pipeline);

    // step physics engine by the real time since the last update, so the simulation keeps
    // real time at any `UpdateRate`
    engine.tick(delta_time);
}

#[cfg(test)]
mod tests {
    use super::*;
    use anarchy::{EntityID, SingleThreadedExecutor, SystemGraph, SystemInstruction, World};
    use crate::glam::{Quat, Vec3};
    use crate::spawn;

    /// A world with the physics resources and a fixed update delta time of `dt`.
    fn world(dt: f32) -> World {
        let world = World::default();
        world.insert_resource(PhysicsEngine::new());
        world.insert_resource(PhysicsQueries::default());
        world.insert_resource(UpdateDeltaTime(dt));
        world
    }

    /// Runs `update_engine_side` `ticks` times.
    fn tick(world: &World, ticks: usize) {
        let mut graph = SystemGraph::new();
        graph.append_system(update_engine_side, SystemInstruction::default(), std::iter::empty()).unwrap();
        for _ in 0..ticks { SingleThreadedExecutor::new().run(world, &graph).unwrap(); }
    }

    fn ball(world: &World) -> EntityID {
        spawn(world, vec![
            Box::new(Transform::new(Vec3::new(0.0, 10.0, 0.0), Quat::IDENTITY, Vec3::ONE)),
            Box::new(PhysicsObject::movable(Collider::Sphere { radius: 0.5 }, 1.0)),
        ])
    }

    fn height(world: &World, id: EntityID) -> f32 {
        world.handle(id).get::<&Transform>().unwrap().unwrap().translation.y
    }

    fn character(world: &World, collider: Collider) -> EntityID {
        spawn(world, vec![
            Box::new(Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE)),
            Box::new(PhysicsObject::movable(collider, 1.0)),
            Box::new(PhysicsCharacter::new()),
        ])
    }

    #[test]
    fn simulation_follows_update_delta_time() {
        // one second of free fall at 30 updates per second drops about 5 units (g/2)
        let world = world(1.0 / 30.0);
        let ball = ball(&world);
        tick(&world, 31);
        let fallen = 10.0 - height(&world, ball);
        assert!((4.4..5.6).contains(&fallen), "fell {fallen}");
    }

    #[test]
    fn bad_character_collider_skips_only_that_character() {
        let world = world(1.0 / 60.0);
        let broken = character(&world, Collider::Mesh { vertices: Box::new([]), indices: Box::new([]) });
        let ball = ball(&world);
        tick(&world, 10);
        assert!(height(&world, ball) < 10.0, "other bodies still simulate");
        let engine = world.resource::<PhysicsEngine>().unwrap();
        let id = *world.handle(broken).get::<&PhysicsObject>().unwrap().unwrap().physics_id();
        assert!(!engine.has_character(id));
    }

    #[test]
    fn despawned_characters_are_removed() {
        let world = world(1.0 / 60.0);
        let entity = character(&world, Collider::Capsule { radius: 0.5, half_height: 1.0 });
        tick(&world, 2);
        let id = *world.handle(entity).get::<&PhysicsObject>().unwrap().unwrap().physics_id();
        assert!(world.resource::<PhysicsEngine>().unwrap().has_character(id));

        world.handle(entity).despawn();
        tick(&world, 1);
        assert!(!world.resource::<PhysicsEngine>().unwrap().has_character(id));
    }

    #[test]
    fn character_settings_rebuild_the_controller() {
        let mut character = PhysicsCharacter::new();
        assert!(!character.take_controller_dirty());
        character.set_slope_config(Some(PhysicsCharacterSlopeConfig::new(0.5, 0.8)))
            .set_snap_to_ground(Some(PhysicsLength::Absolute(0.2)));
        assert!(character.take_controller_dirty());
        assert!(!character.take_controller_dirty());
        assert_eq!(*character.slope_config().unwrap().max_slope_angle_rad(), 0.8);
    }
}
