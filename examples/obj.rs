use gearbox::anarchy::{EntityBuilder, Query, Res, World};
use gearbox::{App, Graphics};
use gearbox::{AssetContent, TextureType, Camera, GearboxRenderPlugin, LazyAssetVault, LoadableAssetVault, MaterialRef, MaterialVault, MeshAssetVault, MeshLoadType, MeshRef, SimpleTexturedMaterial, TextureVault, Transform};
use gearbox::magician_vgpu::glam::{self, Quat};

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

fn setup(
    world: World,
    graphics: Res<Graphics>,
    meshes: Res<MeshAssetVault>,
    materials: Res<MaterialVault>
) {
    let mesh = meshes.load(&world, AssetContent::LocalPath("./examples/SM_Prop_Bonsai_01.obj".into()), MeshLoadType::OBJ).unwrap();
    let vault = TextureVault::current(&world, &graphics).unwrap();
    let texture_handle = AssetContent::LocalPath("./examples/cobblestone.png".into());
    let texture_handle = vault.load(&world, texture_handle, TextureType::PNG).unwrap();
    let mat_handle = materials.allocate(1).unwrap();
    materials.store(&world, mat_handle.clone(), Box::new(SimpleTexturedMaterial::new(texture_handle)));

    world.spawn(
        EntityBuilder::new()
            .push(Transform::identity())
            .push(MaterialRef::new(mat_handle))
            .push(MeshRef::new(mesh))
    );

    world.spawn(
        EntityBuilder::new()
            .push(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .push(Camera::default())
    );
}

fn update(
    mut query: Query<(&'static MeshRef, &'static mut Transform)>
) {
    while let Some((_id, (_mesh, mut transform))) = query.next().unwrap() {
        transform.rotate_by(Quat::from_euler(glam::EulerRot::XYZ, 0.01, 0.01, 0.01));
    }
}
