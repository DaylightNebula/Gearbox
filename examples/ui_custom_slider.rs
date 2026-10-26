use std::sync::Arc;

use anarchy::{EntityBuilder, EntityID, Event, Query, Res, Resource, World};
use gearbox::{App, Camera, GearboxRenderPlugin, Transform, anyhow};
use gearbox::ui::*;
use magician_vgpu::glam::{self, Vec4};

const TRACK: Vec4 = Vec4::new(0.02, 0.02, 0.03, 1.0);
const FILL: Vec4 = Vec4::new(0.9, 0.35, 0.05, 1.0);

/// The node showing the slider's value, and the font to show it in.
#[derive(Debug, Resource)]
struct Readout { label: EntityID, font: Arc<SDFFont> }

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        // just the widget behaviors: everything below is styled by hand
        .add_plugin(UiWidgetsPlugin)
        .on_render_startup(setup)
        .on_render_update(show_volume)
        .run()
}

fn setup(world: World) {
    world.spawn(
        EntityBuilder::new()
            .push(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .push(Camera::default())
    );
    let font = match SDFFont::new(include_bytes!("./LiberationSans-Regular.ttf")) {
        Ok(font) => Arc::new(font),
        Err(err) => return log::error!("Failed to load the font: {err:#}"),
    };

    // a pill-shaped track whose fill shows the value; the `Slider` and `Interaction` on the
    // track are all it takes to make it draggable
    let fill = UiBundle::new(Node::new().height(percent(1.0)).background(FILL).radius(px(10.0))).named("fill");
    let track = UiBundle::new(Node::row().size(px(260.0), px(20.0)).background(TRACK).radius(px(10.0)).border(1.0, FILL))
        .with(Slider::new(0.0, 100.0, 40.0).step(1.0))
        .interactive()
        .named("volume")
        .child(fill);
    let label = UiBundle::new(Node::new().height(px(24.0))).named("readout");
    let card = UiBundle::new(Node::column().anchored(Anchor::Center).padding(Rect::single(px(16.0))).gap(px(10.0))
        .background(Vec4::new(0.05, 0.05, 0.06, 1.0)).radius(px(8.0)))
        .children([label, track]);
    let card = world.spawn(card.into_entity_builder());
    if let Some(label) = find_named(&world, card, "readout") {
        world.insert_resource(Readout { label, font });
    }
}

/// Sizes the fill to the slider's value every frame, and logs changes as they happen.
fn show_volume(world: World, readout: Res<Readout>, mut sliders: Query<&'static Slider>, changes: Event<ValueChange<f64>>) {
    for change in changes.read() { log::info!("volume is now {}", change.value); }
    while let Ok(Some((id, slider))) = sliders.next() {
        let fraction = slider.fraction() as f32;
        let (value, disabled) = (slider.value, slider.disabled);
        if let Some(fill) = find_named(&world, id, "fill")
            && let Ok(Some(mut node)) = world.handle(fill).get::<&mut Node>() {
            node.style_mut().set_width(percent(fraction));
        }
        if let Ok(Some(mut node)) = world.handle(readout.label).get::<&mut Node>() {
            node.style_mut().set_text(Some(Text {
                font: readout.font.clone(),
                content: format!("Volume {value:.0}"),
                color: if disabled { Vec4::splat(0.3) } else { Vec4::ONE },
                font_size: 18.0,
                horizontal_align: Align::Start,
                vertical_align: Align::Center,
            }));
        }
    }
}
