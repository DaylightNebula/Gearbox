use anarchy::{EntityBuilder, Query, Res, WorldDatabase, macros::system};
use cell::{App, Graphics};
use gearbox::{BasicMaterial, BasicMesh, Camera, MaterialRef, MeshRef, RenderPlugin, Transform};
use magician_vgpu::{glam::{self, Quat}, rust::{Vec2, Vec3}};
use shaders::basic_shader;

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
    let vertices: [basic_shader::VertexInput; 3] = [
        basic_shader::VertexInput { position: Vec3::new(0.0,  0.5, 0.0), tex_coords: Vec2::new(0.5, 0.0) },
        basic_shader::VertexInput { position: Vec3::new(-0.5,  -0.5, 0.0), tex_coords: Vec2::new(0.0, 1.0) },
        basic_shader::VertexInput { position: Vec3::new(0.5,  -0.5, 0.0), tex_coords: Vec2::new(1.0, 1.0) }
    ];

    let mesh = BasicMesh::new(
        &*graphics, 
        &vertices, 
        &[0, 1, 2]
    );

    world.insert(
        EntityBuilder::default()
            .add(Transform::identity())
            .add(MaterialRef::new(BasicMaterial::new(glam::Vec4::new(0.1, 0.8, 0.2, 1.0))))
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
