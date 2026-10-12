use anarchy::Component;
use getset::{Getters, Setters};
use crate::glam::Vec3;

use crate::rapier::PhysicsLength;

/// Standard character controller for a `PhysicsObject`, backed by rapier's
/// `KinematicCharacterController`. Must be applied to an entity with a
/// `PhysicsObject`; `update_engine_side` creates the underlying controller
/// the first time it sees the pair and moves it every tick using
/// `target_movement`.
///
/// Fields:
/// - `collider_expansion`: Extra offset added around the character's collider
///   (rapier's `offset`), or `None` to use rapier's default.
/// - `slope_config`: Optional min/max slope angles controlling when the
///   character slides or is allowed to climb; `None` uses rapier's defaults.
/// - `step_config`: Optional autostep configuration for climbing small
///   ledges; `None` disables autostepping.
/// - `snap_to_ground`: Optional max distance used to snap the character to
///   the ground when just barely leaving it; `None` disables snapping.
/// - `target_movement`: Desired movement vector for the next physics tick,
///   set via `set_target_movement`.
/// - `up`: The character's up direction, set via `set_up`.
/// - `is_sliding`: Whether the character is currently sliding down a slope,
///   as reported by `PhysicsEngine` after the last move.
/// - `is_grounded`: Whether the character is currently touching the ground,
///   as reported by `PhysicsEngine` after the last move.
#[derive(Component, Getters, Debug, Clone, Copy)]
#[getset(get = "pub")]
pub struct PhysicsCharacter {
    collider_expansion: Option<PhysicsLength>,
    slope_config: Option<PhysicsCharacterSlopeConfig>,
    step_config: Option<PhysicsCharacterStepConfig>,
    snap_to_ground: Option<PhysicsLength>,
    target_movement: Vec3,
    up: Vec3,
    is_sliding: bool,
    is_grounded: bool,
    /// Set when a controller setting changes, so the engine rebuilds the controller.
    #[getset(skip)]
    controller_dirty: bool
}

/// Slope-handling configuration for a `PhysicsCharacter`, mapped to rapier's
/// `KinematicCharacterController::min_slope_slide_angle` and
/// `max_slope_climb_angle`.
///
/// Fields:
/// - `min_slope_angle_rad`: The minimum slope angle, in radians, at which the
///   character starts sliding.
/// - `max_slope_angle_rad`: The maximum slope angle, in radians, the
///   character is allowed to climb.
#[derive(Getters, Setters, Debug, Clone, Copy)]
#[getset(get = "pub", set = "pub")]
pub struct PhysicsCharacterSlopeConfig {
    min_slope_angle_rad: f32,
    max_slope_angle_rad: f32
}

/// Autostep configuration for a `PhysicsCharacter`, mapped to rapier's
/// `CharacterAutostep`.
///
/// Fields:
/// - `max_height`: The maximum height of a step the character can climb.
/// - `min_width`: The minimum width a step must have to be climbed.
/// - `include_dynamic_bodies`: Whether dynamic rigidbodies count as steps, in
///   addition to fixed ones.
#[derive(Getters, Setters, Debug, Clone, Copy)]
#[getset(get = "pub", set = "pub")]
pub struct PhysicsCharacterStepConfig {
    max_height: PhysicsLength,
    min_width: PhysicsLength,
    include_dynamic_bodies: bool
}

impl PhysicsCharacter {
    /// Creates a new `PhysicsCharacter` this produces
    /// the same result as `default`.
    pub fn new() -> Self {
        Self { 
            collider_expansion: None, 
            slope_config: None, 
            step_config: None, 
            snap_to_ground: None, 
            target_movement: Vec3::ZERO, 
            up: Vec3::Y,
            is_sliding: false,
            is_grounded: false,
            controller_dirty: false
        }
    }

    /// Sets the extra offset kept around the character's collider, or `None` for rapier's
    /// default. Rebuilds the controller on the next physics tick.
    pub fn set_collider_expansion(&mut self, expansion: Option<PhysicsLength>) -> &mut Self {
        self.collider_expansion = expansion; self.controller_dirty = true; self
    }

    /// Sets the slopes the character slides down or can climb, or `None` for rapier's
    /// defaults. Rebuilds the controller on the next physics tick.
    pub fn set_slope_config(&mut self, config: Option<PhysicsCharacterSlopeConfig>) -> &mut Self {
        self.slope_config = config; self.controller_dirty = true; self
    }

    /// Sets the steps the character climbs automatically, or `None` to disable autostep.
    /// Rebuilds the controller on the next physics tick.
    pub fn set_step_config(&mut self, config: Option<PhysicsCharacterStepConfig>) -> &mut Self {
        self.step_config = config; self.controller_dirty = true; self
    }

    /// Sets how far down the character snaps to the ground when it barely leaves it, or
    /// `None` to disable snapping. Rebuilds the controller on the next physics tick.
    pub fn set_snap_to_ground(&mut self, distance: Option<PhysicsLength>) -> &mut Self {
        self.snap_to_ground = distance; self.controller_dirty = true; self
    }

    /// Returns whether a controller setting changed since the last call, and clears it.
    pub(crate) fn take_controller_dirty(&mut self) -> bool {
        std::mem::take(&mut self.controller_dirty)
    }

    /// Sets the target movement vector of this character.
    /// This will begin to apply at the next physics tick, 
    /// and the first physics tick after being overriden.
    pub fn set_target_movement(&mut self, movement: Vec3) -> &mut Self { 
        self.target_movement = movement; self
    }

    /// Sets the up vector of this character. Rebuilds the controller on the next physics tick.
    pub fn set_up(&mut self, up: Vec3) -> &mut Self { self.up = up; self.controller_dirty = true; self }

    /// Used to set if the character is grounded by `PhysicsEngine`.
    pub(crate) fn set_is_grounded(&mut self, is_grounded: bool) { self.is_grounded = is_grounded }

    /// Used to set if the character is sliding by `PhysicsEngine`.
    pub(crate) fn set_is_sliding(&mut self, is_sliding: bool) { self.is_sliding = is_sliding }
}

impl PhysicsCharacterSlopeConfig {
    /// Slides down slopes steeper than `min_slope_angle_rad` and can't climb slopes steeper
    /// than `max_slope_angle_rad`, both in radians.
    pub fn new(min_slope_angle_rad: f32, max_slope_angle_rad: f32) -> Self {
        Self { min_slope_angle_rad, max_slope_angle_rad }
    }
}

impl PhysicsCharacterStepConfig {
    /// Climbs steps up to `max_height` tall with at least `min_width` of room on top,
    /// including dynamic bodies when `include_dynamic_bodies` is set.
    pub fn new(max_height: PhysicsLength, min_width: PhysicsLength, include_dynamic_bodies: bool) -> Self {
        Self { max_height, min_width, include_dynamic_bodies }
    }
}

impl Default for PhysicsCharacter {
    fn default() -> Self {
        Self::new()
    }
}
