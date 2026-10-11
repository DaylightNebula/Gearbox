use anarchy::{Query, Res, World};
use cell::{App, Graphics, spawn};
use gearbox::{AssetContent, TextureType, GearboxRenderPlugin, Handle, LazyAssetVault, Material, MaterialRef, MaterialVault, MeshRef, ShapeBuilder, SimpleTexturedMaterial, TextureVault, glam::*};
use gearbox::{Camera, Transform};

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(startup_triangle)
        .on_render_update(update_triangle)
        .run()
}

fn startup_triangle(
    world: World,
    graphics: Res<Graphics>,
    materials: Res<MaterialVault>
) {
    let vault = TextureVault::current(&world, &graphics).unwrap();
    let texture_handle = AssetContent::LocalPath("./examples/cobblestone.png".into());
    let texture_handle = vault.load(&world, texture_handle, TextureType::PNG).unwrap();
    let mat_handle = materials.allocate(1).unwrap();
    materials.store(&world, mat_handle.clone(), Box::new(SimpleTexturedMaterial::new(texture_handle)));

    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 0.0, 12.0), Quat::IDENTITY, Vec3::ONE)),
        Box::new(Camera::default()),
    ]);

    add_shape(
        &graphics,
        &world,
        ShapeBuilder::new().cube(Vec3::ZERO, Quat::IDENTITY, 1.0, 1.0, 1.0),
        Vec3::new(2.5, 0.0, 0.0),
        mat_handle.clone()
    ).unwrap();

    add_shape(
        &graphics,
        &world,
        ShapeBuilder::new().sphere(Vec3::ZERO, Quat::IDENTITY, 1.0),
        Vec3::new(0.0, 0.0, 0.0),
        mat_handle.clone()
    ).unwrap();

    add_shape(
        &graphics,
        &world,
        ShapeBuilder::new().capsule(Vec3::ZERO, Quat::IDENTITY, 0.5, 2.0),
        Vec3::new(-2.5, 0.0, 0.0),
        mat_handle.clone()
    ).unwrap();

    add_shape(
        &graphics,
        &world,
        ShapeBuilder::new().cylinder(Vec3::ZERO, Quat::IDENTITY, 0.5, 2.0),
        Vec3::new(0.0, 2.5, 0.0),
        mat_handle.clone()
    ).unwrap();

    add_shape(
        &graphics,
        &world,
        ShapeBuilder::new().cone(Vec3::ZERO, Quat::IDENTITY, 0.5, 2.0),
        Vec3::new(0.0, -2.5, 0.0),
        mat_handle
    ).unwrap();
}

fn add_shape(
    graphics: &Graphics,
    world: &World,
    builder: &ShapeBuilder,
    position: Vec3,
    mat_handle: Handle<Box<dyn Material>>
) -> anyhow::Result<()> {
    // create shape
    let mesh = builder
        .build_mesh(&*graphics)?;

    spawn(world, vec![
        Box::new(Transform::new(position, Quat::IDENTITY, Vec3::ONE)),
        Box::new(MaterialRef::new(mat_handle)),
        Box::new(MeshRef::new(mesh)),
    ]);

    Ok(())
}

fn update_triangle(
    mut transforms: Query<(&'static mut Transform, &'static MeshRef)>
) {
    while let Some((_id, (mut transform, _mesh))) = transforms.next().unwrap() {
        transform.rotate_by(Quat::from_euler(EulerRot::XYZ, 0.001, 0.001, 0.001));
    }
}
