use anarchy::{EntityBuilder, World};
use gearbox::{App, Camera, ClearColor, GearboxRenderPlugin, Transform, anyhow};
use gearbox::ui::*;
use magician_vgpu::glam::{self, Vec2, Vec4};

const CARD: u32 = 0x2C2C2E;
const SLOT: u32 = 0x3A3A3C;

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        .add_resource(ClearColor(Vec4::new(0.01, 0.01, 0.012, 1.0)))
        .on_render_startup(setup)
        .run()
}

fn mirrored(points: &[Vec2]) -> Vec<Vec2> { points.iter().map(|p| Vec2::new(-p.x, p.y)).collect() }

fn play() -> Outline {
    Outline::builder().rounded_polygon(&[Vec2::new(-0.2, -0.35), Vec2::new(0.4, 0.0), Vec2::new(-0.2, 0.35)], 0.07).build()
}

fn pause() -> Outline {
    Outline::builder()
        .rounded_rect(Vec2::new(-0.28, -0.35), Vec2::new(-0.08, 0.35), 0.05)
        .rounded_rect(Vec2::new(0.08, -0.35), Vec2::new(0.28, 0.35), 0.05)
        .build()
}

/// Two triangles, the first's tip touching the second's base.
const FORWARD: [[Vec2; 3]; 2] = [
    [Vec2::new(-0.4, -0.3), Vec2::new(0.0, 0.0), Vec2::new(-0.4, 0.3)],
    [Vec2::new(0.0, -0.3), Vec2::new(0.4, 0.0), Vec2::new(0.0, 0.3)],
];

fn forward() -> Outline {
    FORWARD.iter().fold(Outline::builder(), |b, tri| b.rounded_polygon(tri, 0.05)).build()
}

fn backward() -> Outline {
    FORWARD.iter().fold(Outline::builder(), |b, tri| b.rounded_polygon(&mirrored(tri), 0.05)).build()
}

const NEXT: [Vec2; 3] = [Vec2::new(-0.3, -0.3), Vec2::new(0.15, 0.0), Vec2::new(-0.3, 0.3)];

fn next() -> Outline {
    Outline::builder()
        .rounded_polygon(&NEXT, 0.05)
        .rounded_rect(Vec2::new(0.2, -0.3), Vec2::new(0.3, 0.3), 0.03)
        .build()
}

fn previous() -> Outline {
    Outline::builder()
        .rounded_polygon(&mirrored(&NEXT), 0.05)
        .rounded_rect(Vec2::new(-0.3, -0.3), Vec2::new(-0.2, 0.3), 0.03)
        .build()
}

fn stop() -> Outline { Outline::builder().rounded_rect(Vec2::splat(-0.3), Vec2::splat(0.3), 0.06).build() }

/// A slot `size` square with `outline` drawn in white, `inset` in from its edges.
fn icon(outline: Outline, size: f32, inset: f32) -> UiBundle {
    UiBundle::new(Node::row().size(px(size), px(size)).radius(px(size / 2.0)).background(srgb(SLOT)))
        .child(UiBundle::new(Node::new().size(px(size), px(size)).padding(Rect::single(px(inset))).shape(outline, Vec4::ONE)))
}

fn setup(world: World) {
    // the UI draws over the camera's render
    world.spawn(
        EntityBuilder::new()
            .push(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .push(Camera::default())
    );

    let icons = [previous(), backward(), play(), pause(), forward(), next(), stop()];
    let row = |size: f32| UiBundle::new(Node::row().gap(px(12.0)).align(AlignItems::Center))
        .children(icons.iter().cloned().map(move |outline| icon(outline, size, size * 0.2)));
    let card = UiBundle::new(
        Node::column()
            .anchored(Anchor::Center)
            .padding(Rect::single(px(20.0)))
            .gap(px(16.0))
            .radius(px(22.0))
            .background(srgb(CARD)),
    )
    .children([row(24.0), row(48.0), row(96.0)]);
    world.spawn(card.into_entity_builder());
}
