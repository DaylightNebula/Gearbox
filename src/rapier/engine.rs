use std::sync::atomic::AtomicU32;

use anarchy::{EntityID, Resource};
use bimap::BiMap;
use crate::glam::{Quat, Vec3};
use rapier3d::{control::KinematicCharacterController, na::{Isometry, Quaternion, Translation, UnitQuaternion, Vector3}, parry::utils::hashmap::HashMap, prelude::{CCDSolver, Collider, ColliderHandle, ColliderSet, DefaultBroadPhase, ImpulseJointSet, IntegrationParameters, IslandManager, MultibodyJointSet, NarrowPhase, PhysicsPipeline, QueryFilter, QueryPipeline, RigidBody, RigidBodyHandle, RigidBodySet, Shape}};

/// Identifier used to look up an object's rapier rigidbody, collider, and/or
/// character controller within a `PhysicsEngine`, independent of rapier's own
/// handle types. Assigned to each `PhysicsObject` on creation.
pub type PhysicsWorldID = u32;

/// Source of the next `PhysicsWorldID` handed out to a new `PhysicsObject`.
pub static NEXT_PHYSICS_WORLD_ID: AtomicU32 = AtomicU32::new(0);

/// ECS resource wrapping a rapier physics world. Owns the rapier simulation
/// state (rigidbodies, colliders, character controllers, and solver
/// pipelines) and maps each object's `PhysicsWorldID` to its rapier handles
/// and owning `EntityID`. Stepped once per tick by `update_engine_side`.
#[allow(unused)]
#[derive(Resource)]
pub struct PhysicsEngine {
    pub(crate) physics_pipeline: PhysicsPipeline,
    pub(crate) collider_map: BiMap<PhysicsWorldID, ColliderHandle>,
    pub(crate) rigid_body_map: BiMap<PhysicsWorldID, RigidBodyHandle>,
    pub(crate) entity_id_map: BiMap<PhysicsWorldID, EntityID>,
    pub(crate) controllers: HashMap<PhysicsWorldID, (KinematicCharacterController, Box<dyn Shape>)>,

    pub(crate) gravity: Vector3<f32>,
    pub(crate) integration_parameters: IntegrationParameters,
    pub(crate) island_manager: IslandManager,
    pub(crate) broad_phase: DefaultBroadPhase,
    pub(crate) narrow_phase: NarrowPhase,
    pub(crate) rigid_body_set: RigidBodySet,
    pub(crate) collider_set: ColliderSet,
    pub(crate) impulse_joint_set: ImpulseJointSet,
    pub(crate) multibody_joint_set: MultibodyJointSet,
    pub(crate) ccd_solver: CCDSolver
}

impl std::fmt::Debug for PhysicsEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsEngine")
            .field("rigid_bodies", &self.rigid_body_set.len())
            .field("colliders", &self.collider_set.len())
            .field("characters", &self.controllers.len())
            .finish_non_exhaustive()
    }
}

/// Outcome of moving a kinematic character controller one tick, returned by
/// `PhysicsEngine::move_character`. Holds the actual `translation` applied
/// after collision resolution, whether the character `is_sliding` down a
/// slope, and whether it `is_grounded`.
pub(crate) struct MoveCharacterResult {
    pub(crate) translation: Vec3,
    pub(crate) is_sliding: bool,
    pub(crate) is_grounded: bool
}

/// The longest step, in seconds, `PhysicsEngine` simulates in one update.
pub const MAX_STEP: f32 = 0.1;

#[allow(unused)]
impl PhysicsEngine {
    /// Create new engine.
    pub fn new() -> Self {
        Self {
            physics_pipeline: PhysicsPipeline::default(),
            collider_map: BiMap::default(),
            rigid_body_map: BiMap::default(),
            entity_id_map: BiMap::default(),
            controllers: HashMap::default(),
            gravity: Vector3::new(0.0, -9.81, 0.0),
            integration_parameters: IntegrationParameters::default(),
            island_manager: IslandManager::default(),
            broad_phase: DefaultBroadPhase::default(),
            narrow_phase: NarrowPhase::default(),
            rigid_body_set: RigidBodySet::default(),
            collider_set: ColliderSet::default(),
            impulse_joint_set: ImpulseJointSet::default(),
            multibody_joint_set: MultibodyJointSet::default(),
            ccd_solver: CCDSolver,
        }
    }

    /// Get a reference to the current gravity value
    pub fn gravity(&self) -> &Vector3<f32> { &self.gravity }

    /// Get a mutable reference to the current gravity value
    pub fn gravity_mut(&mut self) -> &mut Vector3<f32> { &mut self.gravity }

    /// Get a reference to the underlying rapier rigidbody set.
    pub fn rigid_body_set(&self) -> &RigidBodySet { &self.rigid_body_set }

    /// Get a reference to the underlying rapier collider set.
    pub fn collider_set(&self) -> &ColliderSet { &self.collider_set }

    /// Get the rapier rigidbody handle for the given `PhysicsWorldID`, if one exists.
    pub fn rigid_body_handle(&self, id: &PhysicsWorldID) -> Option<&RigidBodyHandle> { self.rigid_body_map.get_by_left(id) }

    /// Get the rapier collider handle for the given `PhysicsWorldID`, if one exists.
    pub fn collider_handle(&self, id: &PhysicsWorldID) -> Option<&ColliderHandle> { self.collider_map.get_by_left(id) }

    /// Get the `PhysicsWorldID` associated with an entity, if it has one.
    pub fn physics_id(&self, id: EntityID) -> Option<PhysicsWorldID> { self.entity_id_map.get_by_right(&id).copied() }

    /// Get the `EntityID` that owns the given `PhysicsWorldID`, if any.
    pub fn entity_id(&self, id: PhysicsWorldID) -> Option<EntityID> { self.entity_id_map.get_by_left(&id).copied() }

    /// Steps the physics solver by `dt` seconds. Steps are capped at `MAX_STEP`, so a long
    /// hitch slows the simulation down instead of letting bodies tunnel or explode.
    pub(crate) fn tick(&mut self, dt: f32) {
        if dt <= 0.0 { return }
        self.integration_parameters.dt = dt.min(MAX_STEP);
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            &(),
            &()
        );
    }

    /// Create a new query pipeline from this engine.
    pub(crate) fn create_query_pipeline<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a> {
        self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.rigid_body_set,
            &self.collider_set,
            filter
        )
    }

    /// Returns true if this engine has a character controller.
    pub(crate) fn character_ids(&self) -> Vec<PhysicsWorldID> {
        self.controllers.keys().copied().collect()
    }

    pub(crate) fn has_character(&self, id: PhysicsWorldID) -> bool {
        self.controllers.contains_key(&id)
    }

    /// Moves a character inside this engine.
    pub(crate) fn move_character(
        &mut self, 
        id: PhysicsWorldID,
        position: Vec3,
        rotation: Quat,
        desired_translation: Vec3,
        mass: f32,
        delta_time: f32
    ) -> Option<MoveCharacterResult> {
        let Some((controller, shape)) = 
            self.controllers.get_mut(&id) else { return None };

        // create immutable query pipeline for moving character
        let pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.rigid_body_set,
            &self.collider_set,
            QueryFilter::default()
        );

        // create isometry and desired translation
        let position = Vector3::new(position.x, position.y, position.z);
        let position = Translation::from(position);
        let rotation = Quaternion::new(rotation.w, rotation.x, rotation.y, rotation.z);
        let rotation = UnitQuaternion::from_quaternion(rotation);
        let character_pos = Isometry::from_parts(position, rotation);
        let desired_translation = Vector3::new(
            desired_translation.x, 
            desired_translation.y, 
            desired_translation.z
        );

        // update kinematic controller
        let mut collisions = vec![];
        let movement = controller.move_shape(
            delta_time,
            &pipeline,
            shape.as_ref(),
            &character_pos,
            desired_translation,
            |c| collisions.push(c)
        );

        // create mutable pipeline to apply impulses
        let mut query_pipeline_mut = self.broad_phase.as_query_pipeline_mut(
            self.narrow_phase.query_dispatcher(),
            &mut self.rigid_body_set,
            &mut self.collider_set,
            QueryFilter::default()
        );

        // apply movement impulses
        controller.solve_character_collision_impulses(
            delta_time,
            &mut query_pipeline_mut,
            shape.as_ref(),
            mass,
            &collisions,
        );

        Some(MoveCharacterResult {
            translation: Vec3::new(movement.translation.x, movement.translation.y, movement.translation.z),
            is_sliding: movement.is_sliding_down_slope,
            is_grounded: movement.grounded
        })
    }

    /// Extracts the position and rotation of a physics object from this engine.
    pub(crate) fn snapshot_by_rb_handle(&self, handle: RigidBodyHandle) -> Option<RigidBodySnapshot> {
        let Some(rb) = self.rigid_body_set.get(handle) else { return None };

        let translation = rb.translation();
        let rotation = rb.rotation().coords;
        let velocity = rb.vels().as_vector();

        Some(RigidBodySnapshot {
            translation: Vec3::new(translation.x, translation.y, translation.z),
            rotation: Quat::from_xyzw(rotation.x, rotation.y, rotation.z, rotation.w),
            velocity: Vec3::new(velocity[0], velocity[1], velocity[2]),
            angular_velocity: Vec3::new(velocity[3], velocity[4], velocity[5])
        })
    }

    /// Sets the rigidbody position and rotation.
    pub(crate) fn set_rb_position_rotation(
        &mut self, 
        handle: RigidBodyHandle,
        position: Vec3,
        rotation: Quat
    ) {
        let Some(rb) = self.rigid_body_set.get_mut(handle) else { return };
        let rotation = Quaternion::new(rotation.w, rotation.x, rotation.y, rotation.z);

        rb.set_translation(Vector3::new(position.x, position.y, position.z), true);
        rb.set_rotation(UnitQuaternion::from_quaternion(rotation), true);
    }

    /// Apply a velocity to a rigid body.
    pub(crate) fn apply_velocity(
        &mut self,
        handle: RigidBodyHandle,
        velocity: Vec3
    ) {
        let Some(rb) = self.rigid_body_set.get_mut(handle) else { return };
        let linvel = Vector3::new(velocity.x, velocity.y, velocity.z);
        rb.set_linvel(linvel, true);
    }

    /// Apply impulses, torques, and forces to a rigid body.
    pub(crate) fn apply_impulses_torques_forces(
        &mut self,
        handle: RigidBodyHandle,
        mut impulses: Vec<Vec3>,
        mut torque_impulses: Vec<Vec3>,
        mut forces: Vec<Vec3>,
        mut torques: Vec<Vec3>
    ) {
        let Some(rb) = self.rigid_body_set.get_mut(handle) else { return };
        
        torque_impulses.drain(..).for_each(|torque| {
            let impulse = Vector3::new(torque.x, torque.y, torque.z);
            rb.apply_torque_impulse(impulse, true);
        });

        impulses.drain(..).for_each(|impulse| {
            let impulse = Vector3::new(impulse.x, impulse.y, impulse.z);
            rb.apply_impulse(impulse, true);
        });

        torques.drain(..).for_each(|torque| {
            let torque = Vector3::new(torque.x, torque.y, torque.z);
            rb.add_torque(torque, true);
        });

        forces.drain(..).for_each(|force| {
            let force = Vector3::new(force.x, force.y, force.z);
            rb.add_force(force, true);
        });
    }

    /// Inserts a new rigidbody and collider pair into this engine.
    /// Note: This function does not check if the pair are linked, this must be handled externally.
    pub(crate) fn insert(
        &mut self, 
        id: PhysicsWorldID, 
        entity_id: EntityID,
        collider: Collider, 
        rigidbody: RigidBody
    ) -> (RigidBodyHandle, ColliderHandle) {
        // pass to rapier
        let rb_handle = self.rigid_body_set.insert(rigidbody);
        let collider_handle = self.collider_set.insert_with_parent(collider, rb_handle, &mut self.rigid_body_set);

        // store in maps
        self.rigid_body_map.insert(id, rb_handle);
        self.collider_map.insert(id, collider_handle);
        self.entity_id_map.insert(id, entity_id);

        (rb_handle, collider_handle)
    }

    /// Inserts a `KinematicCharacterController` into this engine.
    pub(crate) fn insert_character(
        &mut self,
        id: PhysicsWorldID,
        kinematic_controller: KinematicCharacterController,
        shape: Box<dyn Shape>
    ) {
        self.controllers.insert(id, (kinematic_controller, shape));
    }

    /// Remove a given physics object from this engine.
    pub(crate) fn remove(&mut self, id: PhysicsWorldID) {
        if let Some((_id, handle)) = self.rigid_body_map.remove_by_left(&id) {
            self.rigid_body_set.remove(
                handle, 
                &mut self.island_manager, 
                &mut self.collider_set, 
                &mut self.impulse_joint_set, 
                &mut self.multibody_joint_set, 
                true
            );
        }

        if let Some((_id, handle)) = self.collider_map.remove_by_left(&id) {
            self.collider_set.remove(
                handle, 
                &mut self.island_manager, 
                &mut self.rigid_body_set, 
                true
            );
        }

        self.entity_id_map.remove_by_left(&id);

        self.controllers.remove(&id);
    }
}

/// A snapshot of a rigidbody's transform and velocities read out of rapier
/// during a physics tick, as produced by `PhysicsEngine::snapshot_by_rb_handle`
/// and stored on `PhysicsObject::last_snapshot`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigidBodySnapshot {
    pub translation: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub angular_velocity: Vec3
}
