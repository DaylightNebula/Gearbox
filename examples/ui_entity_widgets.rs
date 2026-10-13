use std::sync::Arc;

use anarchy::{EntityID, Event, Res, ResMut, Resource, World};
use gearbox::{App, Camera, GearboxRenderPlugin, Transform, anyhow, spawn};
use gearbox::ui::{themed::{self, Label}, *};
use magician_vgpu::glam::{self, Vec2};

/// The entities `react` changes, found once their windows are spawned.
#[derive(Debug, Resource)]
struct Ids {
    enabled: EntityID,
    rate: EntityID,
    scale: EntityID,
    name: EntityID,
    greet: EntityID,
    status: EntityID,
    number: EntityID,
    open_popup: EntityID,
    food: EntityID,
    popup: EntityID,
    picked: EntityID,
    close: EntityID,
}

/// What `react` keeps between frames.
#[derive(Debug, Default, Resource)]
struct State {
    clicks: u32,
    number: u8,
}

fn main() -> anyhow::Result<()> {
    let font = Arc::new(SDFFont::new(include_bytes!("./LiberationSans-Regular.ttf"))?);
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        .add_plugin(WidgetPlugin::new(font))
        .add_resource(State { clicks: 0, number: 7 })
        .on_render_startup(setup)
        .on_render_update(react)
        .run()
}

/// Spawns the windows once: the same ones `examples/widgets.rs` describes every frame.
fn setup(world: World) {
    // the UI draws over the camera's render
    spawn(&world, vec![
        Box::new(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE)),
        Box::new(Camera::default()),
    ]);

    let fruit = ["Apple", "Banana", "Cherry"].into_iter().enumerate().map(|(index, name)| themed::list_row(index, name));
    let veg = (1..=20).map(|n| themed::list_row(2 + n, format!("Veg {n}")));
    let panel = themed::panel("Widgets", Vec2::splat(10.0), 320.0).children([
        themed::checkbox("Enabled", true).named("enabled"),
        themed::slider(Slider::log(1.0, 1000.0, 60.0).step(1.0), " per second", 0).named("rate"),
        themed::slider(Slider::log(0.1, 10.0, 1.0), " scale", 2).named("scale"),
        themed::separator(),
        themed::row().children([
            themed::text_input("Your name", 140.0).named("name"),
            themed::button("Greet").named("greet").with(Tooltip("Counts the greetings".into())),
        ]),
        themed::colored_label("No name yet", Theme::default().error).named("status"),
        themed::row().children([
            themed::label("Number"),
            themed::number_input("7", 60.0).named("number"),
            themed::button("Popup").named("open popup"),
        ]),
        themed::separator(),
        themed::scroll_rows(10, [
            themed::section("Fruit", true, fruit),
            themed::section("Vegetables", false, veg),
        ]).with(RadioGroup::default()).named("food"),
    ]);
    let panel = spawn_ui(&world, panel);

    // popups draw over panels
    let popup = themed::frame(Some(180.0))
        .style(|node| node.absolute(Rect::new_top_left(px(120.0), px(200.0))).display(Display::None).z_index(1))
        .children([themed::label("Picked nothing").named("picked"), themed::button("Close").named("close")]);
    let popup = spawn_ui(&world, popup);

    let find = |root: EntityID, name: &str| find_named(&world, root, name).expect("a widget the windows spawned");
    world.insert_resource(Ids {
        enabled: find(panel, "enabled"),
        rate: find(panel, "rate"),
        scale: find(panel, "scale"),
        name: find(panel, "name"),
        greet: find(panel, "greet"),
        status: find(panel, "status"),
        number: find(panel, "number"),
        open_popup: find(panel, "open popup"),
        food: find(panel, "food"),
        popup,
        picked: find(popup, "picked"),
        close: find(popup, "close"),
    });
}

/// Changes `id`'s component `T`, if it has one.
fn change<T: anarchy::ComponentMeta + Sync>(world: &World, id: EntityID, f: impl FnOnce(&mut T)) {
    if let Ok(Some(mut component)) = world.handle(id).get::<&mut T>() { f(&mut component); }
}

/// The first child of `id`, where a themed slider keeps its `Slider`.
fn first_child(world: &World, id: EntityID) -> EntityID {
    world.hierarchy().children(id).and_then(|children| children.first().copied()).unwrap_or(id)
}

/// Reacts to this frame's clicks and changes by editing the widgets.
fn react(
    world: World,
    ids: Res<Ids>,
    mut state: ResMut<State>,
    activated: Event<Activate>,
    toggled: Event<ValueChange<bool>>,
    submitted: Event<Submit>,
    picked: Event<ValueChange<usize>>,
) {
    for Activate { entity } in activated.read() {
        if entity == ids.greet { state.clicks += 1; }
        let toggle_popup = entity == ids.open_popup;
        if toggle_popup || entity == ids.close {
            change(&world, ids.popup, |node: &mut Node| {
                let shown = toggle_popup && *node.style().display() == Display::None;
                node.style_mut().set_display(if shown { Display::Flex } else { Display::None });
            });
        }
    }
    for ValueChange { entity, value } in toggled.read() {
        if entity != ids.enabled { continue }
        for slider in [ids.rate, ids.scale] { change(&world, first_child(&world, slider), |slider: &mut Slider| slider.disabled = !value); }
    }
    for Submit { entity, text } in submitted.read() {
        if entity != ids.number { continue }
        if let Ok(number) = text.trim().parse() { state.number = number; }
        let number = state.number.to_string();
        change(&world, ids.number, |field: &mut TextInput| field.text = number);
    }
    for ValueChange { entity, value } in picked.read() {
        if entity != ids.food { continue }
        let name = match value { 0 => "Apple".to_string(), 1 => "Banana".to_string(), 2 => "Cherry".to_string(), n => format!("Veg {}", n - 2) };
        change(&world, ids.picked, |label: &mut Label| label.text = format!("Picked {name}"));
    }

    let name = world.handle(ids.name).get::<&TextInput>().ok().flatten().map(|field| field.text.clone()).unwrap_or_default();
    let (text, color) = match (name.is_empty(), state.clicks) {
        (true, _) => ("No name yet".to_string(), Some(Theme::default().error)),
        (false, 0) => (String::new(), None),
        (false, clicks) => (format!("Hello {name} x{clicks}"), None),
    };
    change(&world, ids.status, |node: &mut Node| { node.style_mut().set_display(if text.is_empty() { Display::None } else { Display::Flex }); });
    change(&world, ids.status, |label: &mut Label| {
        label.text = text;
        label.color = color;
    });
}
