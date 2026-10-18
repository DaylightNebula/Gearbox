use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use anarchy::Component;
use getset::Getters;
use crate::glam::Vec3;
use mutual::{CowData, SharedList, SharedMap};
use rapier3d::control::CharacterLength;

use crate::rapier::{Collider, NEXT_PHYSICS_WORLD_ID, PhysicsWorldID, RigidBodySnapshot};

/// Component that links an entity to a rigidbody/collider (and optionally a
/// character controller, via `PhysicsCharacter`) inside a `PhysicsEngine`.
/// Create with `fixed` or `movable`; the resulting collider, mass, and
/// fixed/movable state can be changed later through the setters below, and
/// forces/torques/impulses/velocity queued here are consumed and applied by
/// `update_engine_side` on the next physics tick.
///
/// Fields:
/// - `physics_id`: The `PhysicsWorldID` used to look this object up inside
///   `PhysicsEngine`.
/// - `last_snapshot`: The most recent `RigidBodySnapshot` read from rapier,
///   if any.
/// - `collider`: The current `Collider` shape.
/// - `next_velocity`: A linear velocity queued to be applied on the next
///   tick, if any.
/// - `impulses` / `torque_impulses`: Queued one-shot linear/angular impulses
///   to apply on the next tick.
/// - `forces` / `torques`: Persistent linear/angular forces, keyed by an ID
///   returned from `add_force`/`add_torque`, applied every tick until
///   removed.
/// - `force_idx`: Counter used to assign IDs to new forces and torques.
/// - `refresh_rapier`: Set when the rapier rigidbody/collider/controller for
///   this object needs to be rebuilt (e.g. after `set_collider`).
/// - `using_empty_collider`: Set when `collider` resolves to no rapier shape,
///   so no rigidbody/controller is created for it.
/// - `mass`: The mass passed to rapier's collider builder.
/// - `is_fixed`: Whether this object is a fixed (immovable) or dynamic rigidbody.
/// - `friction` / `restitution`: The collider's friction and restitution
///   coefficients, rapier's defaults when `None`.
#[derive(Component, Getters)]
#[getset(get = "pub")]
pub struct PhysicsObject {
    physics_id: PhysicsWorldID,
    last_snapshot: CowData<Option<RigidBodySnapshot>>,
    collider: CowData<Collider>,
    next_velocity: CowData<Vec3>,
    impulses: SharedList<Vec3>,
    torque_impulses: SharedList<Vec3>,
    forces: SharedMap<u32, Vec3>,
    torques: SharedMap<u32, Vec3>,
    force_idx: AtomicU32,
    refresh_rapier: AtomicBool,
    using_empty_collider: AtomicBool,
    mass: CowData<f32>,
    is_fixed: CowData<bool>,
    friction: CowData<Option<f32>>,
    restitution: CowData<Option<f32>>
}

impl std::fmt::Debug for PhysicsObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsObject")
            .field("physics_id", &self.physics_id)
            .field("mass", &*self.mass.get_ref())
            .field("is_fixed", &*self.is_fixed.get_ref())
            .finish_non_exhaustive()
    }
}

impl PhysicsObject {
    /// Creates a new fixed `PhysicsObject`.
    pub fn fixed(collider: Collider, mass: f32) -> Self {
        Self {
            physics_id: NEXT_PHYSICS_WORLD_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst), 
            collider: CowData::new(collider), 
            last_snapshot: CowData::new(None),
            next_velocity: CowData::null(),
            impulses: SharedList::default(),
            torque_impulses: SharedList::default(),
            forces: SharedMap::default(),
            torques: SharedMap::default(),
            force_idx: AtomicU32::new(0),
            refresh_rapier: AtomicBool::new(false),
            using_empty_collider: AtomicBool::new(false),
            mass: CowData::new(mass),
            is_fixed: CowData::new(true),
            friction: CowData::new(None),
            restitution: CowData::new(None)
        }
    }

    /// Creates a new moveable `PhysicsObject`.
    pub fn movable(collider: Collider, mass: f32) -> Self {
        Self { 
            physics_id: NEXT_PHYSICS_WORLD_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst), 
            collider: CowData::new(collider), 
            last_snapshot: CowData::new(None),
            next_velocity: CowData::null(),
            impulses: SharedList::default(),
            torque_impulses: SharedList::default(),
            forces: SharedMap::default(),
            torques: SharedMap::default(),
            force_idx: AtomicU32::new(0),
            refresh_rapier: AtomicBool::new(false),
            using_empty_collider: AtomicBool::new(false),
            mass: CowData::new(mass),
            is_fixed: CowData::new(false),
            friction: CowData::new(None),
            restitution: CowData::new(None)
        }
    }

    /// Call for the refreshing of rapier colliders and rigidbodies
    pub fn refresh_rapier_counterpart(&self) {
        self.refresh_rapier.store(true, Ordering::Release);
    }

    /// Sets the collider of this object
    pub fn set_collider(&self, collider: Collider) {
        self.collider.set(collider);
        self.refresh_rapier_counterpart();
    }

    /// Sets the mass of the rapier physics counterpart
    pub fn set_mass(&self, mass: f32) {
        self.mass.set(mass);
        self.refresh_rapier_counterpart();
    }

    /// Sets whether this object is a fixed (immovable) or dynamic rigidbody.
    pub fn set_fixed(&self, fixed: bool) {
        self.is_fixed.set(fixed);
        self.refresh_rapier_counterpart();
    }

    /// Sets the friction coefficient of the collider, `None` for rapier's default.
    pub fn set_friction(&self, friction: Option<f32>) {
        self.friction.set(friction);
        self.refresh_rapier_counterpart();
    }

    /// Sets the restitution (bounciness) coefficient of the collider, `None` for rapier's default.
    pub fn set_restitution(&self, restitution: Option<f32>) {
        self.restitution.set(restitution);
        self.refresh_rapier_counterpart();
    }

    /// Sets velocity of this object to be applied at the next physics tick.
    pub fn set_velocity(&self, velocity: Vec3) {
        self.next_velocity.set(velocity);
    }

    /// Clears the velocity meant to be set at the next physics tick.
    pub fn clear_velocity(&self) {
        self.next_velocity.set_null();
    }

    /// Adds an impulse force to this object.
    /// This will be applied during the next physics tick.
    /// The provided force is in global space, not local space.
    pub fn add_impulse(&self, force: Vec3) {
        self.impulses.push(force);
    }

    /// Adds a torque impulse force to this object.
    /// This will be applied during the next physics tick.
    /// The provided force is in global space, not local space.
    pub fn add_torque_impulse(&self, torque: Vec3) {
        self.torque_impulses.push(torque);
    }

    /// Adds a force to this object.
    /// This will be applied during the next physics tick.
    /// The provided force is in global space, not local space.
    /// The ID returned is the ID assigned to this force as the force is persistent.
    pub fn add_force(&self, force: Vec3) -> u32 {
        let id = self.force_idx.fetch_add(1, Ordering::SeqCst);
        self.forces.insert(id, force);
        id
    }

    /// Sets the force of an ID on this object.  Replacing previous forces of the same ID.
    /// This will be applied during the next physics tick.
    /// The provided force is in global space, not local space.
    /// Returns true if a previous force existed.
    pub fn set_force(&self, id: u32, force: Vec3) -> bool {
        self.forces.insert(id, force)
    }

    /// Removes a force from this object.
    /// This will be applied during the next physics tick.
    /// Returns the removed force, if any.
    pub fn remove_force(&self, id: u32) -> Option<Vec3> {
        self.forces.remove(&id).map(|a| a.clone())
    }

    /// Adds a torque to this object.
    /// This will be applied during the next physics tick.
    /// The provided torque is in global space, not local space.
    /// The ID returned is the ID assigned to this torque as the torque is persistent.
    pub fn add_torque(&self, torque: Vec3) -> u32 {
        let id = self.force_idx.fetch_add(1, Ordering::SeqCst);
        self.torques.insert(id, torque);
        id
    }

    /// Sets the torque of an ID on this object.  Replacing previous forces of the same ID.
    /// This will be applied during the next physics tick.
    /// The provided torque is in global space, not local space.
    /// Returns true if a previous torque existed.
    pub fn set_torque(&self, id: u32, torque: Vec3) -> bool {
        self.torques.insert(id, torque)
    }

    /// Removes a torque from this object.
    /// This will be applied during the next physics tick.
    /// Returns the removed torque, if any.
    pub fn remove_torque(&self, id: u32) -> Option<Vec3> {
        self.torques.remove(&id).map(|a| a.clone())
    }
}

/// A length used to configure a `PhysicsCharacter`'s controller (collider
/// expansion, snap-to-ground distance, autostep dimensions), mirroring
/// rapier's `CharacterLength`.
///
/// Variants:
/// - `Absolute`: A length in world units.
/// - `Relative`: A length as a fraction of the character's collider size.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum PhysicsLength {
    Absolute(f32),
    Relative(f32)
}

impl PhysicsLength {
    /// Converts this length to rapier's equivalent `CharacterLength`.
    pub fn to_length(&self) -> CharacterLength {
        match self {
            PhysicsLength::Absolute(length) => 
                CharacterLength::Absolute(*length),
            PhysicsLength::Relative(length) => 
                CharacterLength::Relative(*length)
        }
    }
}
