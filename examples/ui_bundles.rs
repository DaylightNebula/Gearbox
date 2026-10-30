//! A task list showing how to build entity UIs from `UiBundle`s:
//!
//! 1. building nodes by hand with `Node`'s builders, `px` and `percent`;
//! 2. putting components on them with `.with(..)`: names, widget behaviors, your own;
//! 3. nesting bundles with `.child(..)` and `.children(..)`;
//! 4. starting from a themed bundle and tweaking it with `.style(..)`;
//! 5. writing functions that return bundles, to reuse a piece of UI;
//! 6. reserving ids with `reserve_id` and `.with_id(..)`, to know a part's entity before spawning;
//! 7. spawning with `into_entity_builder`, adding to a spawned tree with `add_child`, finding entities
//!    with `find_named`, and removing them with `despawn_recursive`;
//! 8. reacting to widget events and changing spawned components;
//! 9. recoloring a tree's themed widgets with `.theme(ThemeOverride { .. })`;
//! 10. scrolling a column once it grows past its `max_height` with `Overflow::Auto`;
//! 11. right click menus built by hand: a `Menu` node of `MenuItem` rows opened with
//!     `.context_menu(..)`, a submenu nested in an item as its child, and your own component and
//!     system lighting items under the pointer.

use std::sync::Arc;

use anarchy::{EntityBuilder, Component, EntityID, Event, Query, Res, Resource, World};
use gearbox::{App, Camera, GearboxRenderPlugin, Transform, anyhow};
use gearbox::ui::{themed::{self, Label}, *};
use magician_vgpu::glam::{self, Vec4};

const PILL: Vec4 = Vec4::new(0.02, 0.06, 0.16, 1.0);
const MENU: Vec4 = Vec4::new(0.012, 0.012, 0.016, 1.0);
const MENU_EDGE: Vec4 = Vec4::new(0.08, 0.08, 0.1, 1.0);
const MENU_HOVER: Vec4 = Vec4::new(0.04, 0.09, 0.22, 1.0);

/// A task's priority, the tint its row takes, and the menu item that picks it.
const PRIORITIES: [(&str, Vec4); 3] = [
    ("High", Vec4::new(0.25, 0.03, 0.02, 0.6)),
    ("Normal", Vec4::ZERO),
    ("Low", Vec4::new(0.02, 0.08, 0.03, 0.6)),
];

/// Your own component on a bundle: one task row, and the ids of the parts the systems below
/// need. The ids are reserved before the row is spawned, see `task_row`.
#[derive(Debug, Component)]
struct Task {
    checkbox: EntityID,
    remove: EntityID,
    /// Items of the row's right click menu.
    toggle: EntityID,
    forget: EntityID,
    /// Items of its "Priority" submenu, in `PRIORITIES` order.
    priorities: [EntityID; 3],
}

/// Your own component lighting a menu item while it's hovered, or while the submenu it holds is
/// open, see `light_menu_items`.
#[derive(Debug, Clone, Copy, Component)]
struct MenuHighlight;

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
        .on_render_update(toggle_tasks)
        .on_render_update(prioritize_tasks)
        .on_render_update(light_menu_items)
        .on_render_update(show_progress)
        .run()
}

/// A reusable piece of UI is just a function returning a bundle. Each call reserves fresh ids
/// for the row's checkbox, remove button and menu items, gives them to the parts with
/// `.with_id(..)`, and keeps them in the row's `Task`, so the systems can go from a click straight
/// to its row.
fn task_row(text: &str) -> UiBundle {
    let (checkbox, remove, toggle, forget) = (reserve_id(), reserve_id(), reserve_id(), reserve_id());
    let priorities = [reserve_id(), reserve_id(), reserve_id()];
    // shown at the pointer when the row is right clicked; it's the row's child, so it goes when
    // the row does. The submenu is a child of its item, opened beside it while it's hovered
    let menu = menu(150.0, [
        menu_item("Toggle done").with_id(toggle),
        submenu("Priority", menu(110.0, PRIORITIES.iter().zip(priorities).map(|((name, _), id)| menu_item(name).with_id(id)))),
        menu_separator(),
        menu_item("Remove").with_id(forget),
    ]);
    UiBundle::new(Node::row().align(AlignItems::Center).gap(px(4.0)).radius(px(3.0)))
        .with(Task { checkbox, remove, toggle, forget, priorities })
        // right clicks need the row to be pickable
        .interactive()
        .context_menu(menu)
        .children([
            // a themed checkbox, tweaked to take the row's leftover room
            themed::checkbox(text, false).with_id(checkbox).style(|node| node.grow(1.0)),
            themed::button_tone("Remove", Tone::Danger).with_id(remove),
        ])
}

/// A closed menu `width` logical pixels wide holding `items`. A `Menu` on a `fixed` node, so the
/// scrolling list it's spawned in doesn't cut it off, hidden until a right click opens it; the
/// `Menu` then shows it and places it each frame.
fn menu(width: f32, items: impl IntoIterator<Item = UiBundle>) -> UiBundle {
    UiBundle::new(Node::column()
            .width(px(width))
            .padding(Rect::single(px(4.0)))
            .gap(px(2.0))
            .background(MENU)
            .border(1.0, MENU_EDGE)
            .radius(px(5.0))
            .fixed(Rect::default())
            .z_index(100)
            .display(Display::None))
        .with(Menu::default())
        .children(items)
}

/// A row of a menu, sending `Activate` with its id when chosen. It needs an `Interaction` to be
/// picked, and its text is a `Label` like the badge's.
fn menu_item(text: &str) -> UiBundle {
    UiBundle::new(Node::row().height(px(26.0)).align(AlignItems::Center).radius(px(3.0)))
        .with(MenuItem::default())
        .interactive()
        .with(MenuHighlight)
        .child(UiBundle::new(Node::new().grow(1.0).padding(Rect::new(px(0.0), px(0.0), px(10.0), px(10.0)))).with(Label::new(text)))
}

/// A `menu_item` opening `menu` beside it while hovered, rather than being chosen: the menu is
/// just its child.
fn submenu(text: &str, menu: UiBundle) -> UiBundle {
    let arrow = UiBundle::new(Node::new().padding(Rect::new_right(px(10.0)))).with(Label::new("\u{203A}"));
    menu_item(text).children([arrow, menu])
}

/// A thin line between groups of items. Lines under 2px can vanish between pixels.
fn menu_separator() -> UiBundle {
    UiBundle::new(Node::new().height(px(2.0)).margin(Rect::new(px(2.0), px(2.0), px(0.0), px(0.0))).background(MENU_EDGE))
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
    world.spawn(
        EntityBuilder::new()
            .push(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .push(Camera::default())
    );

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
            // a column to spawn rows into later, with a few to start with, scrolling once it's
            // taller than its `max_height`
            UiBundle::new(Node::column().gap(px(4.0)).min_height(px(24.0)).max_height(px(110.0))
                    .overflow_y(Overflow::Auto)
                    .scrollbar(Theme::default().scrollbar()))
                .named("list")
                // rows spawned under the list later pick this up too, ticking in green
                .theme(ThemeOverride { accent: Some(srgb(0x3FBF6F)), ..Default::default() })
                .children(["Write the example", "Read the README", "Try the scrollbar", "Right click a task", "Ship it"].map(task_row)),
            themed::caption("Tick a task when it's done, or press Enter to add one", Theme::default().dim),
        ]);

    let toolbar = world.spawn(toolbar.into_entity_builder());
    let card = world.spawn(card.into_entity_builder());
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
        // spawn the row, then add it as the last child of the list
        let row = world.spawn(task_row(text.trim()).into_entity_builder());
        if let Err(err) = world.hierarchy().add_child(ids.list, row) { log::error!("Failed to add the row: {err:#}"); }
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
        if clicked.contains(&task.remove) || clicked.contains(&task.forget) || (clear && done(&world, &task)) { doomed.push(row); }
    }
    // despawn after the query lets go of the rows' components
    drop(tasks);
    for row in doomed { world.handle(row).despawn_recursive(); }
}

/// Ticks or unticks a task chosen in its right click menu.
fn toggle_tasks(world: World, activated: Event<Activate>, mut tasks: Query<&'static Task>) {
    let clicked: Vec<EntityID> = activated.read().map(|event| event.entity).collect();
    if clicked.is_empty() { return }
    while let Ok(Some((_, task))) = tasks.next() {
        if !clicked.contains(&task.toggle) { continue }
        if let Ok(Some(mut checkbox)) = world.handle(task.checkbox).get::<&mut Checkbox>() { checkbox.checked = !checkbox.checked; }
    }
}

/// Tints a task's row with the priority chosen in its menu's submenu.
fn prioritize_tasks(activated: Event<Activate>, mut tasks: Query<(&'static Task, &'static mut Node)>) {
    let clicked: Vec<EntityID> = activated.read().map(|event| event.entity).collect();
    if clicked.is_empty() { return }
    while let Ok(Some((_, (task, mut node)))) = tasks.next() {
        let Some(chosen) = task.priorities.iter().position(|id| clicked.contains(id)) else { continue };
        let tint = PRIORITIES[chosen].1;
        node.style_mut().set_background(if tint == Vec4::ZERO { Background::Empty } else { Background::Color(tint) });
    }
}

/// Lights menu items under the pointer, and items whose submenu is open, the way the `Styling on
/// hover` pattern does for any interactive node.
fn light_menu_items(world: World, mut items: Query<(&'static Interaction, &'static MenuHighlight, &'static mut Node)>) {
    while let Ok(Some((id, (interaction, _, mut node)))) = items.next() {
        let submenu_open = world.hierarchy().children(id).is_some_and(|children| children.iter().any(|child| {
            world.handle(*child).get::<&Menu>().ok().flatten().is_some_and(|menu| menu.open)
        }));
        let lit = *interaction != Interaction::None || submenu_open;
        node.style_mut().set_background(if lit { Background::Color(MENU_HOVER) } else { Background::Empty });
    }
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
