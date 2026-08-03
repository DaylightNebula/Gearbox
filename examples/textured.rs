use anarchy::{EntityBuilder, Query, Res, WorldDatabase, anyhow, macros::system};
use cell::{App, Graphics};
use gearbox::{AssetContent, BasicMesh, TextureType, Camera, GearboxRenderPlugin, LazyAssetVault, MaterialRef, MaterialVault, MeshRef, SimpleTexturedMaterial, TextureVault, Transform};
use magician_vgpu::{glam::{self, Quat}, rust::{Vec2, Vec3}};
use shaders::basic_vertex;

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

#[system]
fn setup(
    graphics: Res<Graphics>,
    materials: Res<MaterialVault>
) {
    let vertices: [basic_vertex::VertexInput; 3] = [
        basic_vertex::VertexInput { position: Vec3::new(0.0,  0.5, 0.0), uvs: Vec2::new(0.5, 0.0), normals: Vec3::default() },
        basic_vertex::VertexInput { position: Vec3::new(-0.5,  -0.5, 0.0), uvs: Vec2::new(0.0, 1.0), normals: Vec3::default() },
        basic_vertex::VertexInput { position: Vec3::new(0.5,  -0.5, 0.0), uvs: Vec2::new(1.0, 1.0), normals: Vec3::default() }
    ];

    let mesh = BasicMesh::new(
        &*graphics,
        &vertices,
        &[0, 1, 2]
    );

    // let mat_handle = materials.allocate(1)?;
    // materials.store(world, mat_handle.clone(), Box::new(BasicMaterial::new(glam::Vec4::new(0.1, 0.8, 0.2, 1.0))));

    let vault = TextureVault::current(world, &graphics)?;
    let texture_handle = AssetContent::LocalPath("./examples/cobblestone.png".into());
    let texture_handle = vault.load(world, texture_handle, TextureType::PNG)?;
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
