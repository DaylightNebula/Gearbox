use anarchy::{Query, Res, World};
use gearbox::{App, AssetContent, Camera, spawn, GearboxRenderPlugin, LoadableAssetVault, MaterialRef, MeshRef, Transform};
use magician_vgpu::glam::*;
use gearbox::skeletal::anim::Animator;
use gearbox::skeletal::{SkeletalMeshLoadType, SkeletalMeshPlugin, SkeletalMeshVault};

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(SkeletalMeshPlugin)
        .on_render_startup(startup_triangle)
        .on_render_update(update_triangle)
        .run()
}

fn startup_triangle(
    world: World,
    meshes: Res<SkeletalMeshVault>
) {
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 10.0, 30.0), Quat::from_rotation_x(-0.5), Vec3::ONE)),
        Box::new(Camera::default()),
    ]);

    // load mesh and material
    let loaded = meshes.load(&world, AssetContent::LocalPath("./examples/skeletal_gltf/Barbarian.glb".into()), SkeletalMeshLoadType::GLTF);
    let (model, material, animations) = match loaded {
        Ok(loaded) => loaded,
        Err(err) => return log::error!("Failed to load Barbarian.glb: {err:#}"),
    };
    let mut animator = Animator::empty();
    animator.load_animations(animations);
    animator.play("2H_Melee_Attack_Spin", true);

    spawn(&world, vec![
        Box::new(Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE * 3.0)),
        Box::new(MaterialRef::new(material)),
        Box::new(animator),
        Box::new(MeshRef::new(model)),
    ]);
}

fn update_triangle(
    mut transforms: Query<(&'static mut Transform, &'static MeshRef)>
) {
    while let Some((_id, (mut transform, _))) = transforms.next().unwrap() {
        let rotation = Quat::from_euler(EulerRot::XYZ, 0.0, 0.005 / 3.0, 0.0) * transform.rotation();
        transform.set_rotation(rotation);
    }
}
