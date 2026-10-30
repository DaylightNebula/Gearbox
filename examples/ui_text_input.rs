//! Text inputs built from `UiBundle`s: a search box, a code editor whose spans color keywords
//! and underline an error with a hover, and a text area that wraps.

use std::sync::Arc;

use anarchy::{EntityBuilder, EntityID, Event, Res, ResMut, Resource, World};
use gearbox::{App, Camera, GearboxRenderPlugin, Transform, anyhow};
use gearbox::ui::*;
use magician_vgpu::glam::{self, Vec4};

const CODE: &str = "// A tiny program. TODO: handle errors
fn main() {
    let greeting = \"hello\";
    let count = 3;
    for i in 0..count {
        println!(\"{greeting} {i}\");
    }
    print(undefined_thing);
}";

const NOTES: &str = "Notes wrap at the edge of their box. Click to place the caret, drag or \
double click to select, and use the arrows, Home and End to move around.";

#[derive(Debug, Resource)]
struct Fonts(Arc<SDFFont>);

/// The nodes `react` changes.
#[derive(Debug, Resource)]
struct Ids { code: EntityID, status: EntityID }

fn main() -> anyhow::Result<()> {
    let font = Arc::new(SDFFont::new(include_bytes!("./LiberationSans-Regular.ttf"))?);
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        // themed tooltips show span hovers
        .add_plugin(WidgetPlugin::new(font.clone()))
        .add_resource(Fonts(font))
        .add_resource(LastHover::default())
        .on_render_startup(setup)
        .on_render_update(react)
        .run()
}

fn color(hex: u32) -> Vec4 { srgb(hex) }

/// A box that edits text, drawn with `text`.
fn field(text: Text, input: TextInput, width: f32, height: f32) -> UiBundle {
    let node = Node::new().size(px(width), px(height)).padding(Rect::single(px(6.0))).background(color(0x1e1e1e))
        .border(1.0, color(0x3c3c3c)).radius(px(4.0));
    UiBundle::new(node).text(text).with(input).interactive()
}

fn setup(world: World, fonts: Res<Fonts>) {
    world.spawn(
        EntityBuilder::new()
            .push(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .push(Camera::default())
    );
    let font = fonts.0.clone();
    let ink = color(0xd4d4d4);
    let line = |text: &str| Text::new(font.clone(), text, ink, 16.0).with_align(Align::Start, Align::Center);

    let search = field(line(""), TextInput::default().hint("Search…"), 320.0, 30.0);
    let code = field(Text::new(font.clone(), CODE, ink, 16.0), TextInput::new(CODE).multiline(), 520.0, 200.0)
        .spans(highlight(CODE))
        .named("code");
    let notes = field(Text::new(font.clone(), NOTES, ink, 16.0).with_wrap(), TextInput::new(NOTES).multiline(), 520.0, 70.0);
    let status = UiBundle::new(Node::new().height(px(24.0))).text(line("Hover the squiggle")).named("status");

    let panel = Node::column().anchored(Anchor::TopLeft).margin(Rect::single(px(10.0))).padding(Rect::single(px(12.0)))
        .gap(px(10.0)).background(color(0x252526)).radius(px(6.0));
    let root = world.spawn(UiBundle::new(panel).children([search, code, notes, status]).into_entity_builder());
    let ids = Ids { code: find_named(&world, root, "code").unwrap(), status: find_named(&world, root, "status").unwrap() };
    world.insert_resource(ids);
}

/// Colors `code`'s keywords, strings, numbers and comments, and marks the unknown name.
fn highlight(code: &str) -> Vec<TextSpan> {
    const KEYWORDS: [&str; 8] = ["fn", "let", "mut", "for", "in", "if", "else", "return"];
    let mut spans = vec![];
    let bytes = code.as_bytes();
    let mut at = 0;
    while at < code.len() {
        let rest = &code[at..];
        let ch = rest.chars().next().unwrap();
        let run = |stop: &dyn Fn(char) -> bool| rest.find(|c: char| stop(c)).unwrap_or(rest.len());
        let len = if rest.starts_with("//") {
            let len = run(&|c| c == '\n');
            spans.push(TextSpan::new(at..at + len).color(color(0x6a9955)));
            if let Some(todo) = rest[..len].find("TODO") { spans.push(TextSpan::new(at + todo..at + todo + 4).underline(color(0x6a9955)).hover("Something to do later")); }
            len
        } else if ch == '"' {
            let len = rest[1..].find('"').map_or(rest.len(), |end| end + 2);
            spans.push(TextSpan::new(at..at + len).color(color(0xce9178)));
            len
        } else if ch.is_ascii_digit() {
            let len = run(&|c| !c.is_ascii_digit());
            spans.push(TextSpan::new(at..at + len).color(color(0xb5cea8)));
            len
        } else if ch.is_alphabetic() || ch == '_' {
            let len = run(&|c| !(c.is_alphanumeric() || c == '_'));
            let word = &rest[..len];
            if KEYWORDS.contains(&word) { spans.push(TextSpan::new(at..at + len).color(color(0x569cd6))); }
            if bytes.get(at + len) == Some(&b'(') || bytes.get(at + len) == Some(&b'!') { spans.push(TextSpan::new(at..at + len).color(color(0xdcdcaa))); }
            if word == "undefined_thing" {
                spans.push(TextSpan::new(at..at + len).wavy(color(0xf14c4c)).hover("cannot find value `undefined_thing` in this scope"));
            }
            len
        } else {
            ch.len_utf8()
        };
        at += len.max(1);
    }
    spans
}

/// Highlights the code again as it's edited, and shows what's hovered.
fn react(world: World, ids: Res<Ids>, input: Res<UiInput>, changed: Event<ValueChange<String>>, mut last: ResMut<LastHover>) {
    for change in changed.read() {
        if change.entity != ids.code { continue }
        if let Ok(Some(mut node)) = world.handle(ids.code).get::<&mut Node>() && let Some(text) = node.style_mut().text_mut() {
            text.spans = highlight(&change.value);
        }
    }
    let hovered = input.hovered_span.as_ref().map(|hover| hover.text.clone());
    if hovered != last.0 {
        let status = hovered.clone().unwrap_or_else(|| "Hover the squiggle".into());
        if let Ok(Some(mut node)) = world.handle(ids.status).get::<&mut Node>() && let Some(text) = node.style_mut().text_mut() {
            text.content = status;
        }
        last.0 = hovered;
    }
}

/// The hover `react` showed last.
#[derive(Debug, Default, Resource)]
struct LastHover(Option<String>);
