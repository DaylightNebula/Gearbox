use gearbox::anarchy::{EntityBuilder, Query, Res, World};
use gearbox::{App, Graphics};
use gearbox::{AssetContent, BasicMesh, TextureType, Camera, GearboxRenderPlugin, LazyAssetVault, MaterialRef, MaterialVault, MeshRef, SimpleTexturedMaterial, TextureVault, Transform};
use gearbox::magician_vgpu::{glam::{self, Quat}, rust::{Vec2, Vec3}};
use gearbox::shaders::basic_vertex;

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
