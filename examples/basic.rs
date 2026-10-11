use anarchy::{Query, Res, World};
use cell::{App, Graphics, spawn};
use gearbox::{BasicMaterial, BasicMesh, Camera, GearboxRenderPlugin, LazyAssetVault, MaterialRef, MaterialVault, MeshRef, Transform};
use magician_vgpu::{glam::{self, Quat}, rust::{Vec2, Vec3}};
use shaders::basic_vertex;

fn main() -> anyhow::Result<()> {
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

    let mat_handle = materials.allocate(1).unwrap();
    materials.store(&world, mat_handle.clone(), Box::new(BasicMaterial::new(glam::Vec4::new(0.1, 0.8, 0.2, 1.0))));

    spawn(&world, vec![
        Box::new(Transform::identity()),
        Box::new(MaterialRef::new(mat_handle)),
        Box::new(MeshRef::new(mesh)),
    ]);

    spawn(&world, vec![
        Box::new(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE)),
        Box::new(Camera::default()),
    ]);
}

fn update(
    mut query: Query<(&'static MeshRef, &'static mut Transform)>
) {
    while let Some((_id, (_mesh, mut transform))) = query.next().unwrap() {
        transform.rotate_by(Quat::from_euler(glam::EulerRot::XYZ, 0.01, 0.01, 0.01));
    }
}
