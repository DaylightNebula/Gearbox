---
name: physics
description: Use when adding 3D physics to a gearbox app with its rapier module (the `rapier` feature) - PhysicsPlugin, PhysicsObject rigidbodies (fixed or movable, mass, colliders), Collider shapes (sphere, capsule, cuboid, cylinder, cone, compound, voxels, convex mesh, halfspace), velocity, impulses, forces and torques, kinematic characters with PhysicsCharacter, raycasts and shape casts with PhysicsQueries, gravity through PhysicsEngine, and the raw rapier3d re-export.
---

# gearbox: physics (rapier)

`gearbox::rapier` connects entities with a `Transform` to a [rapier3d](https://rapier.rs)
simulation. It's behind the `rapier` feature, and is early: its API and behavior still change.

```toml
[dependencies]
gearbox = { path = "../gearbox", features = ["rapier"] }
```

Snippets assume the setup from the gearbox app skill and these imports:

```rust
use gearbox::anarchy::{Query, Res, ResMut, World};
use gearbox::glam::{Quat, Vec3};
use gearbox::rapier::{Collider, PhysicsCharacter, PhysicsEngine, PhysicsObject, PhysicsPlugin, PhysicsQueries, PhysicsQuery};
use gearbox::{App, GearboxRenderPlugin, Transform, anyhow, spawn};
```

## Bodies

```rust
fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(PhysicsPlugin)   // PhysicsEngine, PhysicsQueries and the physics system
        .on_startup(setup)
        .run()
}

fn setup(world: World) {
    // ground: a fixed body never moves
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE)),
        Box::new(PhysicsObject::fixed(Collider::Cuboid { half_width: 50.0, half_height: 0.5, half_depth: 50.0 }, 0.0)),
    ]);
    // a ball that falls onto it
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 10.0, 0.0), Quat::IDENTITY, Vec3::ONE)),
        Box::new(PhysicsObject::movable(Collider::Sphere { radius: 0.5 }, 1.0)),
    ]);
}
```

- `PhysicsObject::fixed(collider, mass)` / `movable(collider, mass)`. An entity needs a
  `Transform` too; add `MaterialRef` and `MeshRef` to draw it as usual.
- The rapier body is created on the next physics tick from the entity's `Transform` (scale is
  ignored, so size the collider instead). After that, physics writes the body's position and
  rotation back into `Transform` every tick.
- Moving a `Transform` yourself teleports the body when it's further from the simulated
  position than the body could have moved in one tick (more than 0.5 units off). Small edits
  are overwritten.
- Despawning the entity removes its body.
- `set_collider`, `set_mass` and `set_fixed` take `&self` and rebuild the body next tick.
  `collider()`, `mass()`, `is_fixed()` and `last_snapshot()` (position, rotation and
  velocities after the last tick) read it.

## Velocity, impulses and forces

All take `&self`, so a `Query<&'static PhysicsObject>` is enough, and apply on the next tick:

| Method | Effect |
| --- | --- |
| `set_velocity(v)` / `clear_velocity()` | set the linear velocity once |
| `add_impulse(v)` / `add_torque_impulse(v)` | one-shot push or spin |
| `add_force(v) -> id`, `set_force(id, v)`, `remove_force(id)` | a force applied every tick until removed |
| `add_torque(v) -> id`, `set_torque(id, v)`, `remove_torque(id)` | the same for torque |

```rust
fn jump(mut bodies: Query<&'static PhysicsObject>) {
    while let Some((_id, body)) = bodies.next().unwrap() {
        body.add_impulse(Vec3::Y * 5.0);
    }
}
```

## Colliders

`Collider` is an engine-agnostic shape, converted to rapier internally:

| Variant | Shape |
| --- | --- |
| `Sphere { radius }`, `Capsule { radius, half_height }`, `Cylinder { .. }`, `Cone { .. }` | primitives, along Y |
| `Cuboid { half_width, half_height, half_depth }` | a box |
| `Compound { children: [(offset, rotation, Collider)] }` | several shapes on one body |
| `Voxels { size, points }` | a grid of cells of `size` |
| `Mesh { vertices, indices }` | a convex polyhedron; the triangles must already be convex, or no body is created |
| `Halfspace { normal }` | an infinite plane |
| `Empty` | no shape: no body is created |

## Characters

Add `PhysicsCharacter` next to a `PhysicsObject` to move it with rapier's kinematic character
controller instead of simulating it. Set where it wants to go each update, in units per
second; it slides along walls and is stopped by the ground:

```rust
fn walk(mut characters: Query<&'static mut PhysicsCharacter>) {
    while let Some((_id, mut character)) = characters.next().unwrap() {
        character.set_target_movement(Vec3::new(2.0, -9.81, 0.0)); // add your own gravity
    }
}
```

- Gravity isn't applied to characters; include it in the movement.
- `is_grounded()` and `is_sliding()` report the last move. `set_up` changes the up vector
  (default `Vec3::Y`).
- The controller is created on the first tick, so the character starts moving on the second.
- Controller settings, all off (rapier's defaults) until set. Each setter rebuilds the
  controller on the next tick:

```rust
fn configure(mut characters: Query<&'static mut PhysicsCharacter>) {
    use gearbox::rapier::{PhysicsCharacterSlopeConfig, PhysicsCharacterStepConfig, PhysicsLength};
    while let Some((_id, mut character)) = characters.next().unwrap() {
        character
            .set_slope_config(Some(PhysicsCharacterSlopeConfig::new(0.6, 0.8)))   // slide above, can't climb above (radians)
            .set_step_config(Some(PhysicsCharacterStepConfig::new(PhysicsLength::Absolute(0.3), PhysicsLength::Absolute(0.2), false)))
            .set_snap_to_ground(Some(PhysicsLength::Absolute(0.2)))
            .set_collider_expansion(Some(PhysicsLength::Absolute(0.01)));
    }
}
```

## Raycasts and shape casts

`PhysicsQueries` runs queries on the next physics tick, so ask in one update and read the
result in a later one, under an id you choose. Results are kept for 10 seconds.

```rust
fn look(queries: Res<PhysicsQueries>) {
    queries.create_query("look", PhysicsQuery::Ray {
        eye: Vec3::new(0.0, 5.0, 0.0), dir: Vec3::NEG_Y, max_dist: 100.0, solid: true, stop_at_first: true,
    });
    if let Some((hits, _data)) = queries.consume_query("look") {
        for hit in hits {
            log::info!("hit entity {:?} at {:?}", hit.entity_id, hit.hit_position_normal);
        }
    }
}
```

`PhysicsQuery::Shape { shape, position, rotation, max_dist, stop_at_first }` casts a
`Collider` instead. `create_query_with_data` attaches a `Box<dyn Any>` that comes back from
`consume_query`.

## Engine and timing

- `ResMut<PhysicsEngine>`: `gravity_mut()` (default `-9.81` on Y), `rigid_body_set()`,
  `collider_set()`, and `entity_id`/`physics_id` to map between entities and bodies.
- The physics system runs in the update graph's `End` pin, once per update pass.
- Each tick steps the simulation by `UpdateDeltaTime`, so it keeps real time at any
  `UpdateRate` (and is repeatable with a fixed time step). Steps are capped at `MAX_STEP`
  (0.1 s), so after a long hitch the simulation falls behind rather than tunneling.
- A collider that rapier rejects (such as a non-convex `Mesh`) logs a warning once and is
  treated as `Empty` until `set_collider` changes it.
- `gearbox::rapier::rapier3d` re-exports rapier for anything not wrapped here.
