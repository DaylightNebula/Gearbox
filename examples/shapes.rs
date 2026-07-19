use anarchy::{EntityBuilder, Query, World, WorldDatabase, anyhow};
use anarchy::{Res, macros::system};
use cell::{App, Graphics};
use gearbox::{GearboxRenderPlugin, MaterialRef, MeshRef, ShapeBuilder, SimpleTexturedMaterial, glam::*};
use gearbox::{Camera, Transform};

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(startup_triangle)
        .on_render_update(update_triangle)
        .run()
}

#[system]
fn startup_triangle(
    graphics: Res<Graphics>
) {
    world.insert(
        EntityBuilder::default()
            .add(Transform::new(Vec3::new(0.0, 0.0, 12.0), Quat::IDENTITY, Vec3::ONE))
            .add(Camera::default())
            .build()
    );

    add_shape(
        &graphics,
        world,
        ShapeBuilder::new().cube(Vec3::ZERO, Quat::IDENTITY, 1.0, 1.0, 1.0),
        Vec3::new(2.5, 0.0, 0.0)
    )?;

    add_shape(
        &graphics,
        world,
        ShapeBuilder::new().sphere(Vec3::ZERO, Quat::IDENTITY, 1.0),
        Vec3::new(0.0, 0.0, 0.0)
    )?;

    add_shape(
        &graphics,
        world,
        ShapeBuilder::new().capsule(Vec3::ZERO, Quat::IDENTITY, 0.5, 2.0),
        Vec3::new(-2.5, 0.0, 0.0)
    )?;

    add_shape(
        &graphics,
        world,
        ShapeBuilder::new().cylinder(Vec3::ZERO, Quat::IDENTITY, 0.5, 2.0),
        Vec3::new(0.0, 2.5, 0.0)
    )?;

    add_shape(
        &graphics,
        world,
        ShapeBuilder::new().cone(Vec3::ZERO, Quat::IDENTITY, 0.5, 2.0),
        Vec3::new(0.0, -2.5, 0.0)
    )?;
}

fn add_shape(
    graphics: &Graphics,
    world: &World,
    builder: &ShapeBuilder,
    position: Vec3
) -> anyhow::Result<()> {
    // create shape
    let mesh = builder
        .build_mesh(&*graphics)?;

    world.insert(
        EntityBuilder::default()
            .add(Transform::new(position, Quat::IDENTITY, Vec3::ONE))
            .add(MaterialRef::new(SimpleTexturedMaterial::from_png(&*graphics, include_bytes!("./cobblestone.png"))?))
            .add(MeshRef::new(mesh))
            .build()
    );

    Ok(())
}

#[system]
fn update_triangle(
    transforms: Query<(&mut Transform, &MeshRef)>
) {
    for (mut transform, _mesh) in transforms.as_iter() {
        transform.rotate_by(Quat::from_euler(EulerRot::XYZ, 0.001, 0.001, 0.001));
    }
}
