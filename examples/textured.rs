use anarchy::{EntityBuilder, Query, Res, WorldDatabase, macros::system};
use cell::{App, Graphics};
use gearbox::{BasicMesh, Camera, MaterialRef, MeshRef, RenderPlugin, SimpleTexturedMaterial, Transform};
use magician_vgpu::{glam::{self, Quat}, rust::{Vec2, Vec3}};
use shaders::basic_vertex;

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(RenderPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

#[system]
fn setup(
    graphics: Res<Graphics>
) {
    let vertices: [basic_vertex::VertexInput; 3] = [
        basic_vertex::VertexInput { position: Vec3::new(0.0,  0.5, 0.0), uvs: Vec2::new(0.5, 0.0) },
        basic_vertex::VertexInput { position: Vec3::new(-0.5,  -0.5, 0.0), uvs: Vec2::new(0.0, 1.0) },
        basic_vertex::VertexInput { position: Vec3::new(0.5,  -0.5, 0.0), uvs: Vec2::new(1.0, 1.0) }
    ];

    let mesh = BasicMesh::new(
        &*graphics, 
        &vertices, 
        &[0, 1, 2]
    );

    world.insert(
        EntityBuilder::default()
            .add(Transform::identity())
            .add(MaterialRef::new(SimpleTexturedMaterial::from_png(&*graphics, include_bytes!("./cobblestone.png"))?))
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
