//! A task list showing how to build entity UIs from `UiBundle`s:
//!
//! 1. building nodes by hand with `Node`'s builders, `px` and `percent`;
//! 2. putting components on them with `.with(..)`: names, widget behaviors, your own;
//! 3. nesting bundles with `.child(..)` and `.children(..)`;
//! 4. starting from a themed bundle and tweaking it with `.style(..)`;
//! 5. writing functions that return bundles, to reuse a piece of UI;
//! 6. reserving ids with `reserve_id` and `.with_id(..)`, to know a part's entity before spawning;
//! 7. spawning with `spawn_ui`, adding to a spawned tree with `spawn_ui_under`, finding entities
//!    with `find_named`, and removing them with `despawn_recursive`;
//! 8. reacting to widget events and changing spawned components;
//! 9. recoloring a tree's themed widgets with `.theme(ThemeOverride { .. })`.

use std::sync::Arc;

use anarchy::{Component, EntityID, Event, Query, Res, Resource, World};
use gearbox::{App, Camera, GearboxRenderPlugin, Transform, anyhow, spawn};
use gearbox::ui::{themed::{self, Label}, *};
use magician_vgpu::glam::{self, Vec4};

const PILL: Vec4 = Vec4::new(0.02, 0.06, 0.16, 1.0);

/// Your own component on a bundle: one task row, and the ids of the parts the systems below
/// need. The ids are reserved before the row is spawned, see `task_row`.
#[derive(Debug, Component)]
struct Task {
    checkbox: EntityID,
    remove: EntityID,
}

/// The entities the systems change, found by name once the UI is spawned.
#[derive(Debug, Resource)]
struct Ids {
    input: EntityID,
    add: EntityID,
    clear: EntityID,
    list: EntityID,
    badge: EntityID,
}

fn main() -> anyhow::Result<()> {
    let font = Arc::new(SDFFont::new(include_bytes!("./LiberationSans-Regular.ttf"))?);
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        // themed widgets, their behaviors, and the fonts `themed::Label` draws with
        .add_plugin(WidgetPlugin::new(font))
        .on_render_startup(setup)
        // after `Start`, where this frame's clicks were turned into events
        .on_render_update(add_tasks)
        .on_render_update(remove_tasks)
        .on_render_update(show_progress)
        .run()
}

/// A reusable piece of UI is just a function returning a bundle. Each call reserves fresh ids
/// for the row's checkbox and remove button, gives them to the parts with `.with_id(..)`, and
/// keeps them in the row's `Task`, so the systems can go from a click straight to its row.
fn task_row(world: &World, text: &str) -> UiBundle {
    let (checkbox, remove) = (reserve_id(world), reserve_id(world));
    UiBundle::new(Node::row().align(AlignItems::Center).gap(px(4.0)))
        .with(Task { checkbox, remove })
        .children([
            // a themed checkbox, tweaked to take the row's leftover room
            themed::checkbox(text, false).with_id(checkbox).style(|node| node.grow(1.0)),
            themed::button_tone("Remove", Tone::Danger).with_id(remove),
        ])
}

/// A node styled entirely by hand: a pill whose text is a `themed::Label`, which works on any
/// node and draws in the theme's font.
fn badge() -> UiBundle {
    UiBundle::new(Node::new()
            .height(px(22.0))
            .padding(Rect::new(px(0.0), px(0.0), px(10.0), px(10.0)))
            .justify(JustifyContent::Center)
            .background(PILL)
            .border(1.0, Theme::default().accent)
            // half the shorter side rounds the ends into a pill
            .radius(percent(0.5)))
        .with(Label::new(""))
        .named("badge")
}

fn setup(world: World) {
    spawn(&world, vec![
        Box::new(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE)),
        Box::new(Camera::default()),
    ]);

    // a bar across the top of the window: a root anchored to the top edge, its children spread
    // out with `SpaceBetween`
    let toolbar = UiBundle::new(Node::row()
            .anchored(Anchor::FillTop)
            .height(px(44.0))
            .padding(Rect::new(px(0.0), px(0.0), px(12.0), px(12.0)))
            .justify(JustifyContent::SpaceBetween)
            .align(AlignItems::Center)
            .background(Theme::default().panel))
        // every themed widget in the bar takes these colors, the rest come from the `UiTheme`
        .theme(ThemeOverride {
            widget: Some(srgb(0x3A2A12)),
            hover: Some(srgb(0x5A4220)),
            text: Some(srgb(0xF2D6A3)),
            ..Default::default()
        })
        .children([
            themed::heading("Tasks"),
            badge(),
            themed::button("Clear done").named("clear"),
        ]);

    // a themed window, moved to the middle of the screen with `.style(..)`, holding a row to
    // type tasks into and the list itself
    let card = themed::frame(Some(380.0))
        .style(|node| node.anchored(Anchor::Center))
        .children([
            themed::row().children([
                themed::text_input("New task", 300.0).named("input"),
                themed::button_tone("Add", Tone::Primary).named("add"),
            ]),
            themed::separator(),
            // an empty column to spawn rows into later, with a few to start with
            UiBundle::new(Node::column().gap(px(4.0)).min_height(px(24.0)))
                .named("list")
                // rows spawned under the list later pick this up too, ticking in green
                .theme(ThemeOverride { accent: Some(srgb(0x3FBF6F)), ..Default::default() })
                .children(["Write the example", "Read the README", "Ship it"].map(|text| task_row(&world, text))),
            themed::caption("Tick a task when it's done, or press Enter to add one", Theme::default().dim),
        ]);

    let toolbar = spawn_ui(&world, toolbar);
    let card = spawn_ui(&world, card);
    let find = |root: EntityID, name: &str| find_named(&world, root, name).expect("a node the UI spawned");
    world.insert_resource(Ids {
        input: find(card, "input"),
        add: find(card, "add"),
        clear: find(toolbar, "clear"),
        list: find(card, "list"),
        badge: find(toolbar, "badge"),
    });
}

/// Adds a row under the list when "Add" is clicked or Enter is pressed in the field.
fn add_tasks(world: World, ids: Res<Ids>, activated: Event<Activate>, submitted: Event<Submit>) {
    let clicked = activated.read().any(|event| event.entity == ids.add);
    let entered = submitted.read().any(|event| event.entity == ids.input);
    if !clicked && !entered { return }

    let Ok(Some(mut field)) = world.handle(ids.input).get::<&mut TextInput>() else { return };
    let text = std::mem::take(&mut field.text);
    // let go of the field before spawning, which locks other entities
    drop(field);
    if !text.trim().is_empty() {
        // `spawn_ui_under` adds a bundle as the last child of a spawned node
        spawn_ui_under(&world, ids.list, task_row(&world, text.trim()));
    }
}

/// Whether `task`'s checkbox is ticked.
fn done(world: &World, task: &Task) -> bool {
    world.handle(task.checkbox).get::<&Checkbox>().ok().flatten().is_some_and(|checkbox| checkbox.checked)
}

/// Removes a task when its "Remove" is clicked, and every ticked one on "Clear done".
fn remove_tasks(world: World, ids: Res<Ids>, activated: Event<Activate>, mut tasks: Query<&'static Task>) {
    let clicked: Vec<EntityID> = activated.read().map(|event| event.entity).collect();
    if clicked.is_empty() { return }
    let clear = clicked.contains(&ids.clear);

    let mut doomed = vec![];
    while let Ok(Some((row, task))) = tasks.next() {
        if clicked.contains(&task.remove) || (clear && done(&world, &task)) { doomed.push(row); }
    }
    // despawn after the query lets go of the rows' components
    drop(tasks);
    for row in doomed { world.handle(row).despawn_recursive(); }
}

/// Dims ticked tasks and counts them in the badge.
fn show_progress(world: World, ids: Res<Ids>, mut tasks: Query<&'static Task>) {
    let dim = Theme::default().dim;
    let (mut total, mut finished) = (0, 0);
    while let Ok(Some((_, task))) = tasks.next() {
        let is_done = done(&world, &task);
        total += 1;
        finished += is_done as usize;
        // a themed checkbox's text is the `Label` on its second child
        let text = world.hierarchy().children(task.checkbox).and_then(|children| children.get(1).copied());
        if let Some(text) = text && let Ok(Some(mut label)) = world.handle(text).get::<&mut Label>() {
            label.color = is_done.then_some(dim);
        }
    }
    if let Ok(Some(mut label)) = world.handle(ids.badge).get::<&mut Label>() {
        label.text = if total == 0 { "Nothing to do".into() } else { format!("{finished} of {total} done") };
    }
}
