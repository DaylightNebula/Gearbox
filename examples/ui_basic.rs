use std::sync::Arc;

use gearbox::{
    App, AssetContent, BasicMaterial, BasicMesh, Camera, GearboxRenderPlugin, Graphics,
    MaterialRef, MeshRef, TextureType, TextureVault, Transform,
    anarchy::{Event, Query, Res, World},
    anyhow, shaders::basic_vertex, spawn,
};
use gearbox::ui::*;
use magician_vgpu::{
    glam::{self, Quat, Vec4},
    rust::{Vec2, Vec3},
};

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

fn setup(world: World, graphics: Res<Graphics>) {
    if let Err(err) = build_scene(&world, &graphics) {
        log::error!("Failed to build the scene: {err:#}");
    }
}

fn build_scene(world: &World, graphics: &Graphics) -> anyhow::Result<()> {
    let test_texture = TextureVault::current(world, graphics)?.load(
        world,
        AssetContent::Binary(Box::new(*include_bytes!("./cobblestone.png"))),
        TextureType::PNG,
    )?;

    let vertices: [basic_vertex::VertexInput; 3] = [
        basic_vertex::VertexInput {
            position: Vec3::new(0.0, 0.5, 0.0),
            uvs: Vec2::new(0.5, 0.0),
            normals: Vec3::default(),
        },
        basic_vertex::VertexInput {
            position: Vec3::new(-0.5, -0.5, 0.0),
            uvs: Vec2::new(0.0, 1.0),
            normals: Vec3::default(),
        },
        basic_vertex::VertexInput {
            position: Vec3::new(0.5, -0.5, 0.0),
            uvs: Vec2::new(1.0, 1.0),
            normals: Vec3::default(),
        },
    ];

    let mesh = BasicMesh::new(graphics, &vertices, &[0, 1, 2]);

    spawn(world, vec![
        Box::new(Transform::identity()),
        Box::new(MaterialRef::new(BasicMaterial::new(glam::Vec4::new(0.1, 0.8, 0.2, 1.0)))),
        Box::new(MeshRef::new(mesh)),
    ]);

    spawn(world, vec![
        Box::new(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE)),
        Box::new(Camera::default()),
    ]);

    let font_bytes = include_bytes!("./LiberationSans-Regular.ttf");
    let font = Arc::new(SDFFont::new(font_bytes)?);

    let mut root = UINode::new("root".to_string());
    root.set_position_type(PositionType::Absolute(Rect::new_bottom_right(
        Val::Px(20.0),
        Val::Px(20.0),
    )));
    root.set_column(JustifyContent::End, AlignItems::End);
    root.set_width(Val::PercentWidth(0.5));
    root.set_height(Val::PercentHeight(0.5));
    root.set_background(Background::Color(Vec4::new(0.05, 0.05, 0.05, 1.0)));
    root.set_border_color(Some(Vec4::new(0.5, 0.5, 0.5, 1.0)));
    root.set_border(Val::Px(1.0));
    root.set_border_radius(RectCorners::single(Val::Px(15.0)));

    let mut root_a = UINode::new("root_a".to_string());
    root_a.set_width(Val::Px(100.0));
    root_a.set_height(Val::Px(100.0));
    root_a.set_margin(Rect::single(Val::Px(10.0)));
    root_a.set_background(Background::Image(test_texture));
    root_a.set_border_color(Some(Vec4::ONE));
    root_a.set_border(Val::Px(1.0));
    root_a.set_border_radius(RectCorners::new(
        Val::Px(15.0),
        Val::Px(15.0),
        Val::Px(15.0),
        Val::Px(0.0),
    ));

    let mut root_b = UINode::new("root_b".to_string());
    root_b.set_width(Val::Px(200.0));
    root_b.set_height(Val::Px(50.0));
    root_b.set_margin(Rect::single(Val::Px(10.0)));
    root_b.set_padding(Rect::single(Val::Px(5.0)));
    root_b.set_background(Background::Color(Vec4::new(0.05, 0.05, 0.05, 1.0)));
    root_b.set_border_color(Some(Vec4::new(0.5, 0.5, 0.5, 1.0)));
    root_b.set_border(Val::Px(1.0));
    root_b.set_border_radius(RectCorners::single(Val::Px(15.0)));
    root_b.set_listener(|_, interaction| log::info!("Test Listener Triggered! {interaction:?}"));
    root_b.set_text(Some(Text {
        font,
        content: "Hello World!".into(),
        color: Vec4::ONE,
        font_size: 24.0,
        horizontal_align: Align::End,
        vertical_align: Align::End,
    }));

    root.add(root_a);
    root.add(root_b);

    spawn(world, vec![Box::new(UINodeSDFRoot(root))]);
    Ok(())
}

fn update(mut query: Query<(&'static MeshRef, &'static mut Transform)>, pressed_events: Event<UINodePressedEvent>) {
    while let Ok(Some((_, (_mesh, mut transform)))) = query.next() {
        transform.rotate_by(Quat::from_euler(glam::EulerRot::XYZ, 0.01, 0.01, 0.01));
    }

    for event in pressed_events.read() {
        log::info!("Pressed \"{}\"", event.id);
    }
}
