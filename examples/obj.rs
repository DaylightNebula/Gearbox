use anarchy::{EntityBuilder, Query, Res, WorldDatabase, anyhow, macros::system};
use cell::App;
use gearbox::{AssetContent, BindlessArrayTextureType, BindlessArrayTextureVault, Camera, GearboxRenderPlugin, LazyAssetVault, LoadableAssetVault, MaterialRef, MaterialVault, MeshAssetVault, MeshLoadType, MeshRef, SimpleTexturedMaterial, Transform};
use magician_vgpu::glam::{self, Quat};

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

#[system]
fn setup(
    meshes: Res<MeshAssetVault>,
    vault: Res<BindlessArrayTextureVault>,
    materials: Res<MaterialVault>
) {
    let mesh = meshes.load(world, AssetContent::LocalPath("./examples/SM_Prop_Bonsai_01.obj".into()), MeshLoadType::OBJ)?;
    let texture_handle = AssetContent::Binary(Box::new(*include_bytes!("./cobblestone.png")));
    let texture_handle = vault.load(world, texture_handle, BindlessArrayTextureType::PNG)?;
    let mat_handle = materials.allocate(1)?;
    materials.store(world, mat_handle.clone(), Box::new(SimpleTexturedMaterial::new(texture_handle)));

    world.insert(
        EntityBuilder::default()
            .add(Transform::identity())
            .add(MaterialRef::new(mat_handle))
            .add(MeshRef::new(mesh))
            .build()
    );

    world.insert(
        EntityBuilder::default()
            .add(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .add(Camera::default())
            .build()  
    );
}

#[system]
fn update(
    query: Query<(&MeshRef, &mut Transform)>
) {
    for (_mesh, mut transform) in query.as_iter() {
        transform.rotate_by(Quat::from_euler(glam::EulerRot::XYZ, 0.01, 0.01, 0.01));
    }
}
