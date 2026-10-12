//! The `editor` feature's inspector on a small scene. Select an entity in the "Entities"
//! window to edit its components; `Spin` is a custom component made editable with `Reflect`.
//!
//! Run with `cargo run --example editor --features editor`.

use gearbox::anarchy::{Component, Query, Reflect, Res, World};
use gearbox::{App, Graphics, spawn};
use gearbox::{AssetContent, TextureType, GearboxRenderPlugin, LazyAssetVault, MaterialRef, MaterialVault, MeshRef, ShapeBuilder, SimpleTexturedMaterial, TextureVault, glam::*};
use gearbox::{Camera, Transform};

/// Spins an entity around its Y axis.
#[derive(Debug, Component, Reflect)]
struct Spin {
    /// Radians per update.
    #[reflect(hint(min = 0.0, max = 0.2))]
    speed: f32,
    /// Stops the spin without losing the speed.
    paused: bool
}

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(startup)
        .on_update(spin)
        .run()
}

fn startup(
    world: World,
    graphics: Res<Graphics>,
    materials: Res<MaterialVault>
) {
    let vault = TextureVault::current(&world, &graphics).unwrap();
    let texture = vault.load(&world, AssetContent::LocalPath("./examples/cobblestone.png".into()), TextureType::PNG).unwrap();
    let material = materials.allocate(1).unwrap();
    materials.store(&world, material.clone(), Box::new(SimpleTexturedMaterial::new(texture)));

    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 0.0, 8.0), Quat::IDENTITY, Vec3::ONE)),
        Box::new(Camera::default()),
    ]);

    let shapes = [
        (ShapeBuilder::new().cube(Vec3::ZERO, Quat::IDENTITY, 1.0, 1.0, 1.0).build_mesh(&*graphics), Vec3::new(-1.5, 0.0, 0.0), 0.02),
        (ShapeBuilder::new().cylinder(Vec3::ZERO, Quat::IDENTITY, 0.5, 1.5).build_mesh(&*graphics), Vec3::new(1.5, 0.0, 0.0), 0.0),
    ];
    for (mesh, position, speed) in shapes {
        spawn(&world, vec![
            Box::new(Transform::new(position, Quat::IDENTITY, Vec3::ONE)),
            Box::new(MaterialRef::new(material.clone())),
            Box::new(MeshRef::new(mesh.unwrap())),
            Box::new(Spin { speed, paused: false }),
        ]);
    }
}

fn spin(mut spinners: Query<(&'static mut Transform, &'static Spin)>) {
    while let Some((_id, (mut transform, spin))) = spinners.next().unwrap() {
        if !spin.paused { transform.rotate_by(Quat::from_rotation_y(spin.speed)); }
    }
}
