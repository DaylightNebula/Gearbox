---
name: ui-entity-widgets
description: Use when building retained, Bevy-style UI with gearbox's ui module (gearbox::ui) - entity UI nodes built as UiBundles (Node, UiBundle, into_entity_builder, find_named, reserve_id/with_id, despawning), filled vector shapes and icons (Outline, Node::shape), hover styling, behavior-only widget components (Button, Checkbox, Slider, RadioGroup/RadioButton, TextInput single and multiline text fields with caret, selection, undo, clipboard, colored/underlined TextSpans and span hovers, Collapsible, DragHandle, Tooltip, right click context menus with ContextMenu/Menu/MenuItem and submenus) and their events (Activate, ValueChange, Submit), scrolling nodes by wheel and scrollbar, the ready-styled themed:: builders (panel, button, slider, text_input, section, scroll_rows, scroll_area, text_box, menu, ...; prefer hand-built UiBundles for app UI), themed::Label text, styling your own widgets, UiTheme/Theme colors and per-tree ThemeOverride, and UiInput for pointer and keyboard state. For Style and layout see the gearbox ui-layout skill.
---

# gearbox ui: entity widgets

UI nodes are entities. Each has a `Node(Style)` component and is parented through anarchy's
`Hierarchy`, and widgets are components on those nodes. Snippets assume the setup from the
gearbox ui-layout skill (`UIPlugin` plus `WidgetPlugin::new(font)`) and these imports:

```rust
use gearbox::{anarchy::{EntityID, Event, Query, Res, World}, glam::{Vec2, Vec4}};
use gearbox::ui::{themed::{self, Label}, *};
```

## Prefer `UiBundle`s over `themed::`

For retained (non immediate mode) UI, build your own `UiBundle`s from `Node` builders and
behavior-only widgets. Reach for `themed::` only when you want the immediate mode look.

- **Use `UiBundle`s** for app and product UI: anything with its own look (a media player, a
  HUD, a desktop widget), anything you restyle on hover or state, and reusable pieces you
  compose. You own every node, so nothing overwrites your colors, sizes or children.
- **Use `themed::`** for debug panels and tools that should match `Widget` windows, or to
  sketch UI quickly. Themed widgets are restyled from `UiTheme` every frame, find their parts by
  child index, and overwrite the colors they control, so they resist customizing.
- **Mixing is fine.** A themed builder returns a `UiBundle`, so it can sit inside your own tree,
  and `.style(..)` tweaks its layout.

A hand-built button is a node, a behavior and a look:

```rust
/// A pill button showing `text`, sending `Activate` when clicked.
fn pill_button(font: &std::sync::Arc<SDFFont>, text: &str) -> UiBundle {
    let label = Text::new(font.clone(), text, Vec4::ONE, 15.0).with_align(Align::Center, Align::Center);
    UiBundle::new(Node::row().padding(Rect::new(px(6.0), px(6.0), px(14.0), px(14.0)))
            .radius(px(16.0)).background(srgb(0x2C2C2E)).text(label))
        .with(Button::default())
        .interactive()
}
```

Hover and press looks are yours too, see Styling on hover below.

## Building and spawning

A `UiBundle` is a node waiting to be spawned, with its components and child bundles:

```rust
fn settings(world: &World) -> EntityID {
    let window = UiBundle::new(Node::column().anchored(Anchor::TopRight).width(px(280.0))
            .padding(Rect::single(px(10.0))).gap(px(6.0)).background(srgb(0x15151A)).radius(px(6.0)))
        .children([
            themed::checkbox("Enabled", true).named("enabled"),
            themed::slider(Slider::new(0.0, 100.0, 50.0).step(1.0), "%", 0).named("volume"),
            themed::row().children([themed::text_input("Name", 160.0).named("name"), themed::button("Save").named("save")]),
        ]);
    world.spawn(window.into_entity_builder())
}
```

- **`UiBundle` methods:**
  - `new(node)`;
  - `.with(component)` adds any component, including your own;
  - `.interactive()` adds an `Interaction`, making the node pickable;
  - `.named(..)` adds a `Name`;
  - `.child(..)` / `.children(..)`;
  - `.style(|node| node.grow(1.0))` tweaks the node, handy on themed bundles;
  - `.theme(ThemeOverride { .. })` (see Theming);
  - `.with_id(id)`.
- **Spawning:**
  - `world.spawn(bundle.into_entity_builder())` spawns the tree and returns the root's id.
    Every node also gets a `ComputedNode`, holding where it was last laid out in window pixels.
  - To grow a spawned tree (a list), spawn the bundle, then
    `world.hierarchy().add_child(parent, id)` makes it the parent's last child.
  - `find_named(world, root, "save")` finds an entity by `Name` at or below `root`.
  - `world.handle(id).despawn_recursive()` removes a node and everything under it.
- **Reusing UI:** write functions returning `UiBundle`. To know a part's id before spawning,
  `reserve_id()` it, give it to the part with `.with_id(id)`, and keep it in your own
  component on the root.
- **Node builders:** `Node::row()`/`column()`, `width`, `height`, `size`, `min_*`/`max_*`,
  `margin`, `padding`, `gap`, `justify`, `align`, `align_self`, `wrap`, `grow`, `shrink`,
  `basis`, `background`, `shape(outline, color)`, `border(width, color)`, `radius`, `anchored`,
  `absolute`, `fixed`, `z_index`, `display`, `overflow`/`overflow_x`/`overflow_y`, `scrollbar` and `text`. After spawning, change a node through `node.style_mut()`; `Node` doesn't
  deref to `Style`.
- **Sizes are logical pixels.** They're scaled by the window's scale factor when laid out, and
  `ComputedNode` is in window pixels.

## Behavior-only widgets

Put one on any node with an `Interaction` and style it yourself. Each updates its own value as
it's used and sends events. Your systems may also set the value directly; doing so sends no
event.

| Component | Behavior | Event |
| --- | --- | --- |
| `Button { disabled }` | pressed then let go over it | `Activate { entity }` |
| `Checkbox { checked, disabled }` | flips on click | `ValueChange<bool>` |
| `Slider { value, min, max, step, scale, disabled }` | follows the pointer while dragged; `Slider::new(min, max, v)`, `Slider::log(..)`, `.step(1.0)`; `fraction()` gives 0..1 | `ValueChange<f64>` |
| `RadioGroup { selected }` + `RadioButton { index }` | a button selects its index in the nearest group above it | `ValueChange<usize>` (entity = group) |
| `TextInput { text, hint, replace_on_focus, disabled, multiline, editor, scroll, .. }` | click to focus and place the caret, type, Enter (Command/Control+Enter if `multiline`) or click elsewhere hands it back; draws through the node's `Text`, see Text inputs below | `ValueChange<String>` while typing, `Submit { entity, text }` |
| `Collapsible { open }` | clicking its first child (needs an `Interaction`) shows or hides its second | `ValueChange<bool>` |
| `ContextMenu { disabled }` | a right click on it, or on a node below it without its own, opens the `Menu` among its children at the pointer; add it with `UiBundle::context_menu(menu)` | none |
| `Menu { open }` | a `PositionType::Fixed` node shown while `open`; a `MenuItem` with a `Menu` child opens it beside itself while hovered; Escape, choosing an item or pressing elsewhere closes them all | none |
| `MenuItem { disabled }` | a choice in the `Menu` above it, chosen when pressed then let go over it; closes the menus | `Activate { entity }` |
| `DragHandle { target }` | dragging moves `target` (or its parent if `None`), an absolutely positioned node | none |
| `Tooltip(String)` | shown by the themed tooltip window while hovered | none |

`UiWidgetsPlugin` runs just these, without the themed look. `WidgetPlugin` adds it too.

A hand-styled slider:

```rust
fn volume_slider() -> UiBundle {
    let fill = UiBundle::new(Node::new().height(percent(1.0)).background(srgb(0xE5591A)).radius(px(10.0))).named("fill");
    UiBundle::new(Node::row().size(px(240.0), px(20.0)).background(srgb(0x101014)).radius(px(10.0)))
        .with(Slider::new(0.0, 100.0, 40.0).step(1.0))
        .interactive()
        .child(fill)
}

/// Sizes each slider's fill to its value, every frame.
fn show_sliders(world: World, mut sliders: Query<&'static Slider>) {
    while let Ok(Some((id, slider))) = sliders.next() {
        let fraction = slider.fraction() as f32;
        if let Some(fill) = find_named(&world, id, "fill")
            && let Ok(Some(mut node)) = world.handle(fill).get::<&mut Node>() {
            node.style_mut().set_width(percent(fraction));
        }
    }
}
```

## Context menus

```rust
use gearbox::ui::{themed, *};

/// A row that opens a menu, with a submenu, where it's right clicked. Chosen items send
/// `Activate { entity }` with the item's id.
fn file_row(name: &str) -> UiBundle {
    let menu = themed::menu(160.0, [
        themed::menu_item_shortcut("Open", "Enter").named("open"),
        themed::submenu("Move to", 120.0, [themed::menu_item("Archive").named("archive"), themed::menu_item("Trash").named("trash")]),
        themed::separator(),
        themed::menu_item("Delete").named("delete"),
    ]);
    UiBundle::new(Node::row().height(px(24.0)).align(AlignItems::Center))
        .interactive() // right clicks need a pickable node
        .context_menu(menu)
        .child(themed::label(name))
}
```

- The menu and its submenus are ordinary bundles in the target's children: find items with
  `find_named` or `reserve_id`/`with_id`, and set `MenuItem::disabled` to grey one out.
- A hand-styled menu is any node with a `Menu`, made `fixed(..)` and `Display::None` so it
  starts closed, holding nodes with a `MenuItem` and an `Interaction`. The system shows it and
  places it each frame, flipping submenus left at the window's edge.

## Text inputs

A `TextInput` shows itself through its node's `Text`, given on the bundle. Each frame it
writes the text (or hint), caret, selection and scroll there, and turns on `clip`; the font,
size, color, `wrap` and spans stay yours. Build fields from `UiBundle`s:

```rust
fn search_box(font: &std::sync::Arc<SDFFont>) -> UiBundle {
    UiBundle::new(Node::new().size(px(300.0), px(30.0)).padding(Rect::single(px(6.0))).background(srgb(0x1E1E1E)).radius(px(4.0)))
        .text(Text::new(font.clone(), "", Vec4::ONE, 16.0).with_align(Align::Start, Align::Center))
        .with(TextInput::default().hint("Search…"))
        .interactive()
}

/// A code editor: `spans` color and mark the text, and follow it as it's edited.
fn code_editor(font: &std::sync::Arc<SDFFont>, code: &str) -> UiBundle {
    UiBundle::new(Node::new().size(px(520.0), px(240.0)).padding(Rect::single(px(6.0))).background(srgb(0x1E1E1E)))
        .text(Text::new(font.clone(), code, srgb(0xD4D4D4), 16.0))
        .spans([
            TextSpan::new(0..2).color(srgb(0x569CD6)),
            TextSpan::new(3..7).wavy(srgb(0xF14C4C)).hover("unused function"),
        ])
        .with(TextInput::new(code).multiline())
        .interactive()
        .named("code")
}

/// Highlights the code again whenever it's edited.
fn rehighlight(world: World, changed: Event<ValueChange<String>>) {
    for change in changed.read() {
        if let Ok(Some(mut node)) = world.handle(change.entity).get::<&mut Node>()
            && let Some(text) = node.style_mut().text_mut() {
            text.spans = vec![]; // your highlighter's spans for `change.value`
        }
    }
}
```

- **Editing:** clicks place the caret, double and triple clicks select words and lines, and
  dragging selects. Arrows move (Alt on macOS, Control elsewhere, by word; Command on macOS by
  line), Home/End go to the line's ends, and Command (macOS) or Control does select all, copy,
  cut, paste, undo and redo (Shift+Z or Y). `multiline` fields take Enter and Tab, scroll with
  the wheel, and move by line with Up and Down.
- **State:** `editor` (a `TextEditor`) holds the caret, `selection()` and undo history; set
  `editor.caret`/`anchor` to move it. `scroll` is in logical pixels.
- **Looks:** `caret_color`, `selection_color` and `hint_color` default to the text's color
  (the selection and hint fainter).
- **Spans follow edits:** typing inside a span grows it, typing at its ends doesn't, and
  deleting shrinks it. Set them again on `ValueChange` to recolor.
- **Hovers:** a span's `hover` shows in the themed tooltip window (with `WidgetPlugin`) and
  sends `TextHover` events; `UiInput::hovered_span` holds the current one. Hovers work on any
  node's text, not just inputs.

## Shapes and icons

`Node::shape(outline, color)` fills an `Outline` instead of a background, stretched over the
node's content box (inside its padding, so padding insets it). An outline is closed contours of
quadratic curves in a unit box, from (-0.5, -0.5) at the top left to (0.5, 0.5) at the bottom
right, filled by the nonzero rule. Build one with `Outline::builder()`: `move_to`, `line_to`,
`quad_to` and `close`, plus `polygon`, `rounded_polygon(points, radius)`,
`rounded_rect(min, max, radius)` and `circle(center, radius)`.

```rust
fn play_icon() -> Outline {
    Outline::builder().rounded_polygon(&[Vec2::new(-0.2, -0.35), Vec2::new(0.4, 0.0), Vec2::new(-0.2, 0.35)], 0.07).build()
}

/// A round icon button: the outer node is picked and can show a hover circle, the inner one
/// draws the icon.
fn icon_button(outline: Outline, size: f32) -> UiBundle {
    UiBundle::new(Node::row().size(px(size), px(size)).radius(px(size / 2.0)).shrink(0.0))
        .with(Button::default())
        .interactive()
        .child(UiBundle::new(Node::new().size(px(size), px(size)).padding(Rect::single(px(size * 0.2))).shape(outline, Vec4::ONE)))
}
```

- A node draws a background *or* a shape. Put the shape on a child to show it over a color.
- To swap an icon (play to pause), set `Background::Shape(outline, color)` through `style_mut()`.
- A contour wound the other way cuts a hole. Keep outlines to tens of curves, since each curve
  is checked for every pixel of the node.
- `examples/ui_shapes.rs` draws a set of media icons.

## Reacting to widgets

```rust
/// Saves on "Save", and follows the volume slider. Add with `on_render_update`.
fn react(world: World, activated: Event<Activate>, volume: Event<ValueChange<f64>>) {
    let Some(save) = world.resource::<SettingsIds>().ok().map(|ids| ids.save) else { return };
    for event in activated.read() {
        if event.entity == save { log::info!("saving"); }
    }
    for change in volume.read() { log::info!("slider {} is now {}", change.entity, change.value); }
}

#[derive(Debug, anarchy::Resource)]
struct SettingsIds { save: EntityID }
```

- `event.entity` is the widget's own entity. For themed widgets that's:
  - the bundle's root for buttons, checkboxes, toggles, text inputs and list rows;
  - the slider's first child for `themed::slider` (the bundle's root is a row);
  - the `RadioGroup` for selections.
- Keep the ids you need in a resource, found with `find_named` after spawning. Or tag widgets
  with your own component (`.with(Action::Save)`) and read it back from `event.entity`, which
  needs no ids at all.
- **Changing widgets:** get a component through `world.handle(id).get::<&mut T>()`, for
  example `Checkbox::checked`, `Label::text`, `TextInput::text`, `Slider::disabled`, or a
  node's `style_mut().set_display(Display::None)` to hide it.

### Styling on hover

`Interaction` is `None`, `Hovered` or `Pressed`, set by picking on every node made
`.interactive()`. Only those nodes are picked, with or without a background, so a node can show
its background only while hovered:

```rust
/// Colors a node's background while the pointer is on it.
#[derive(Debug, Clone, Copy, gearbox::anarchy::Component)]
struct HoverBackground { hovered: Vec4, pressed: Vec4 }

fn hover_backgrounds(mut nodes: Query<(&'static Interaction, &'static HoverBackground, &'static mut Node)>) {
    while let Ok(Some((_, (interaction, colors, mut node)))) = nodes.next() {
        node.style_mut().set_background(match *interaction {
            Interaction::None => Background::Empty,
            Interaction::Hovered => Background::Color(colors.hovered),
            Interaction::Pressed => Background::Color(colors.pressed),
        });
    }
}
```

### Frame order

The pins of the render graph:
- **`Start`:** input becomes `UiInput`, picking sets each `Interaction`, and the widget
  behaviors update values and send events.
- **`Normal`:** your systems run, and see this frame's events.
- **`End`:** themed widgets restyle, then everything is laid out and drawn.

Picking uses last frame's layout, so there's one frame of latency. `UiInput` holds `cursor`,
`down`/`pressed`/`released`, `right_pressed`, `clicks` (2 for a double click), `wheel`/`wheel_x`, `keys`, `modifiers`
(`command()` is Command on macOS, Control elsewhere), `hovered`, `captured`, `hovered_span`,
`over_scrollbar`/`scroll_drag` and `wants_pointer()`, which tells game input whether the pointer belongs to the UI.
`UiClipboard` is the OS clipboard (`text()`, `set_text(..)`), kept in memory on the web.

## Themed widgets

`themed::` builders give the look of the immediate mode widgets, styled from `UiTheme` every
frame. Prefer your own `UiBundle`s for app UI (see above); these suit tools and debug panels:

| Kind | Builders |
| --- | --- |
| Text | `label`, `colored_label`, `label_right(text, width)`, `legend(text, width)`, `strong`, `heading`, `caption(text, color)` |
| Layout | `row()`, `column()`, `separator()`, `vseparator()`, `space(px)` |
| Windows | `frame(Option<width>)` (untitled; anchor or position it), `panel(title, pos, width)` (dragged by its title) |
| Controls | `button`, `button_tone(text, Tone::Primary \| Danger)`, `checkbox(text, checked)`, `toggle(text, on)` (a `Checkbox`), `slider(Slider, suffix, decimals)`, `slider_bar(Slider, width)`, `text_input(hint, width)`, `number_input(text, width)`, `segmented(options, selected)` |
| Lists | `list_row(index, ListItem::new(text).swatch(c).trailing(t))` (selects in the `RadioGroup` above), `collapsible(title, open, children)`, `section(..)` (shows a count), `scroll_rows(visible, children)` (scrolls past `visible` rows; add `.with(RadioGroup::default())` to select rows) |
| Scrolling | `scroll_area(max_height, children)` (a column that scrolls when taller), `text_box(text, max_height)` (wrapped text that scrolls) |
| Menus | `menu(width, items)` with `menu_item(text)`, `menu_item_shortcut(text, shortcut)`, `submenu(text, width, items)` and `separator()`; give it to `.context_menu(..)` on an interactive bundle |
| Readouts | `led(text, lit, color)` (a `themed::Led`), `keycap(text)`, `seven_segment(text, digits, lit)` (a `themed::SevenSegment`) |

- **Text:** a themed widget's text is a `themed::Label { text, size, bold, align, color, wrap }`
  component. Change `text` to change what it says.
- **Parts:** themed widgets find their parts by child position. For example, a checkbox's text
  is the `Label` on its second child. Don't reorder or remove their children.
- **What restyling overwrites:** each frame it resets the colors of parts it controls, the
  label colors of buttons, slider values, list rows, headers and text inputs, and segmented
  option widths. Other `Label` colors and node sizes stick.
- **Labels need `WidgetPlugin`.** `Label` text is only drawn with it, because it supplies the
  fonts. With `UiWidgetsPlugin` alone, set `Text` on nodes yourself.
- **Tooltips:** add `.with(Tooltip("..".into()))` to any interactive node.

## Theming

- **`UiTheme`** holds the 11 `Theme` colors (`panel`, `border`, `widget`, `hover`, `well`,
  `selected`, `accent`, `on_accent`, `text`, `dim`, `error`) and the `font`/`bold` fonts. Set
  it with `WidgetPlugin::new(font).with_theme(..).with_bold(..)`, or change it at runtime
  through `ResMut<UiTheme>`.
- **`ThemeOverride`** recolors one tree. Every field is an `Option`, `None` keeps the
  parent's, and nearer overrides win:

  ```rust
  fn warm_toolbar() -> UiBundle {
      themed::row()
          .theme(ThemeOverride { widget: Some(srgb(0x3A2A12)), text: Some(srgb(0xF2D6A3)), ..Default::default() })
          .children([themed::button("Undo"), themed::button("Redo")])
  }
  ```

  - Nodes spawned into the tree later pick it up.
  - A node only has an override if it was spawned with one, so spawn
    `ThemeOverride::default()` to fill it in at runtime.
  - `ThemeOverride::colors(theme)` sets every color.
- **Matching the look:** `local_theme(&world, id, &ui_theme)` gives the theme a node is drawn
  with. `Theme`'s methods give each widget's colors in each state, for styling headless
  widgets to match: `button(tone, hot)`, `switch(on, hot)`, `slider(enabled, lit)`,
  `list_row`, `header`, `field_border`, `option`, `led`, `segments`, `menu_item(hot, enabled)`,
  `scrollbar()` (a `Scrollbar` for `Node::scrollbar`) and `separator`.

## Gotchas

- **Locks.** Component and resource locks aren't reentrant. Drop a query or guard before
  spawning, despawning, or locking the same component through `world.handle(..)`. Collect
  ids in one loop, then act on them.
- **Fixed components.** An entity's components are fixed when it's spawned. Give widgets
  everything they'll need up front: `.interactive()` and `ThemeOverride::default()`.
- **Layering.** Entity UI draws under immediate-mode windows. `z_index` only orders entity
  nodes among themselves.
- **Scrolling** is by pixels on any node with `Overflow::Scroll` or `Overflow::Auto` (see the
  ui-layout skill). Every child is laid out, but what's scrolled out of view isn't drawn or
  picked.
- **Menus are children.** A context menu is a child of the node it belongs to, so it's found
  with `find_named`, recolored by its `ThemeOverride`s and despawned with it. Being `Fixed`, it
  isn't cut off by the scroll areas it's in. Clicks that close a menu still reach what's under
  the pointer.
- **Typing** reaches `TextInput`s through window keyboard events, so the window needs focus.
- **A `TextInput` needs a `Text`** on its node to show anything: give it one on the bundle
  (themed fields make their own).
- **Headless testing:** headless apps get no input. A test can send
  `gearbox::WindowEvent(winit::event::WindowEvent::CursorMoved { .. })` and `MouseInput`
  through `world.send_event` (use `winit::event::DeviceId::dummy()`). Key events can't be
  built, so write `TextInput::text` directly instead.
