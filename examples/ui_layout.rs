use std::sync::Arc;

use anarchy::{EntityBuilder, World};
use gearbox::{App, Camera, GearboxRenderPlugin, Transform, anyhow};
use gearbox::ui::*;
use magician_vgpu::glam::{self, Vec4};

const BAR: Vec4 = Vec4::new(0.02, 0.02, 0.025, 1.0);
const CARD: Vec4 = Vec4::new(0.04, 0.04, 0.05, 1.0);
const CHIP: Vec4 = Vec4::new(0.13, 0.35, 0.9, 1.0);
const EDGE: Vec4 = Vec4::new(0.2, 0.2, 0.22, 1.0);

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        .on_render_startup(setup)
        .run()
}

fn text(font: &Arc<SDFFont>, content: &str, size: f32) -> UINode {
    let mut node = UINode::default();
    node.set_text(Some(Text {
        font: font.clone(),
        content: content.into(),
        color: Vec4::ONE,
        font_size: size,
        horizontal_align: Align::Start,
        vertical_align: Align::Center,
    }));
    node
}

fn panel(color: Vec4) -> UINode {
    let mut node = UINode::default();
    node.set_background(Background::Color(color));
    node.set_border(Val::Px(1.0));
    node.set_border_color(Some(EDGE));
    node.set_border_radius(RectCorners::single(Val::Px(6.0)));
    node
}

/// Builds the UI once: every root is anchored to part of the window.
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

    // a bar across the top, its three labels spread to the ends and middle
    let mut bar = panel(BAR);
    bar.set_anchored(Anchor::FillTop);
    bar.set_height(Val::Px(40.0));
    bar.set_row(JustifyContent::SpaceBetween, AlignItems::Center);
    bar.set_padding(Rect::new(Val::Auto, Val::Auto, Val::Px(14.0), Val::Px(14.0)));
    bar.add(text(&font, "iso_ui", 18.0));
    bar.add(text(&font, "Layout", 14.0));
    bar.add(text(&font, "v0.1", 14.0));

    // a sidebar down the left, under the bar
    let mut sidebar = panel(BAR);
    sidebar.set_anchored(Anchor::FillLeft);
    sidebar.set_width(Val::Px(160.0));
    sidebar.set_margin(Rect::new_top(Val::Px(48.0)));
    sidebar.set_padding(Rect::single(Val::Px(10.0)));
    sidebar.set_gap(Val::Px(6.0));
    for name in ["Scene", "Assets", "Settings"] {
        let mut item = panel(CARD);
        item.set_height(Val::Px(30.0));
        item.set_column(JustifyContent::Center, AlignItems::Start);
        item.set_padding(Rect::new_left(Val::Px(10.0)));
        item.add(text(&font, name, 14.0));
        sidebar.add(item);
    }

    // a card in the middle, as tall as its content
    let mut card = panel(CARD);
    card.set_anchored(Anchor::Center);
    card.set_width(Val::Px(320.0));
    card.set_padding(Rect::single(Val::Px(14.0)));
    card.set_gap(Val::Px(10.0));
    card.add(text(&font, "Flexbox", 22.0));

    // the middle box grows into what the fixed ones leave
    let mut row = UINode::default();
    row.set_row(JustifyContent::Start, AlignItems::Stretch);
    row.set_height(Val::Px(36.0));
    row.set_gap(Val::Px(8.0));
    for grow in [0.0, 1.0, 0.0] {
        let mut part = panel(if grow > 0.0 { CHIP } else { BAR });
        part.set_width(if grow > 0.0 { Val::Auto } else { Val::Px(48.0) });
        part.set_flex_grow(grow);
        row.add(part);
    }
    card.add(row);

    // chips wrap onto new lines when the row runs out
    let mut chips = UINode::default();
    chips.set_row(JustifyContent::Start, AlignItems::Center);
    chips.set_flex_wrap(FlexWrap::Wrap);
    chips.set_gap(Val::Px(6.0));
    for name in ["row", "column", "wrap", "grow", "shrink", "gap", "justify", "align"] {
        let mut chip = panel(CHIP);
        chip.set_padding(Rect::new(Val::Px(4.0), Val::Px(4.0), Val::Px(8.0), Val::Px(8.0)));
        chip.set_border_radius(RectCorners::single(Val::Percent(0.5)));
        chip.add(text(&font, name, 13.0));
        chips.add(chip);
    }
    card.add(chips);

    // a badge in the bottom right corner, kept in from the edges by its margin
    let mut badge = panel(BAR);
    badge.set_anchored(Anchor::BottomRight);
    badge.set_margin(Rect::single(Val::Px(12.0)));
    badge.set_padding(Rect::single(Val::Px(8.0)));
    badge.add(text(&font, "Anchored bottom right", 13.0));

    for root in [bar, sidebar, card, badge] {
        world.spawn(EntityBuilder::new().push(UINodeSDFRoot(root)));
    }
}
