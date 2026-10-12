---
name: ui-entity-widgets
description: Use when building retained, Bevy-style UI with gearbox's ui module (gearbox::ui) - entity UI nodes (Node, UiBundle, spawn_ui, spawn_ui_under, find_named, reserve_id/with_id, despawning), behavior-only widget components (Button, Checkbox, Slider, RadioGroup/RadioButton, TextInput, Collapsible, ScrollRows, DragHandle, Tooltip) and their events (Activate, ValueChange, Submit), the ready-styled themed:: builders (panel, button, slider, text_input, section, scroll_rows, ...), themed::Label text, styling your own widgets, UiTheme/Theme colors and per-tree ThemeOverride, and UiInput for pointer and keyboard state. For Style and layout see the gearbox ui-layout skill.
---

# gearbox ui: entity widgets

UI nodes are entities. Each has a `Node(Style)` component and is parented through anarchy's
`Hierarchy`, and widgets are components on those nodes. Snippets assume the setup from the
gearbox ui-layout skill (`UIPlugin` plus `WidgetPlugin::new(font)`) and these imports:

```rust
use gearbox::{anarchy::{EntityID, Event, Query, Res, World}, glam::{Vec2, Vec4}};
use gearbox::ui::{themed::{self, Label}, *};
```

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
    spawn_ui(world, window)
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
  - `spawn_ui(world, bundle)` returns the root's id. It also adds a `ComputedNode`, holding
    where the node was last laid out in window pixels.
  - `spawn_ui_under(world, parent, bundle)` adds a bundle as the parent's last child, for
    lists that grow.
  - `find_named(world, root, "save")` finds an entity by `Name` at or below `root`.
  - `world.handle(id).despawn_recursive()` removes a node and everything under it.
- **Reusing UI:** write functions returning `UiBundle`. To know a part's id before spawning,
  `reserve_id(world)` it, give it to the part with `.with_id(id)`, and keep it in your own
  component on the root.
- **Node builders:** `Node::row()`/`column()`, `width`, `height`, `size`, `min_*`/`max_*`,
  `margin`, `padding`, `gap`, `justify`, `align`, `align_self`, `wrap`, `grow`, `shrink`,
  `basis`, `background`, `border(width, color)`, `radius`, `anchored`, `absolute`, `z_index`,
  `display` and `text`. After spawning, change a node through `node.style_mut()`; `Node` doesn't
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
| `TextInput { text, hint, replace_on_focus, disabled }` | click to focus, type, Enter or click elsewhere hands it back; `editing()` is the text shown while typing | `ValueChange<String>` while typing, `Submit { entity, text }` |
| `Collapsible { open }` | clicking its first child (needs an `Interaction`) shows or hides its second | `ValueChange<bool>` |
| `ScrollRows { first, visible }` | shows `visible` of the `ScrollItem` nodes below it, scrolled by the wheel; `rows` is the count | none |
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
- Keep the ids you need in a resource, found with `find_named` after spawning.
- **Changing widgets:** get a component through `world.handle(id).get::<&mut T>()`, for
  example `Checkbox::checked`, `Label::text`, `TextInput::text`, `Slider::disabled`, or a
  node's `style_mut().set_display(Display::None)` to hide it.

### Frame order

The pins of the render graph:
- **`Start`:** input becomes `UiInput`, picking sets each `Interaction`, and the widget
  behaviors update values and send events.
- **`Normal`:** your systems run, and see this frame's events.
- **`End`:** themed widgets restyle, then everything is laid out and drawn.

Picking uses last frame's layout, so there's one frame of latency. `UiInput` holds `cursor`,
`down`/`pressed`/`released`, `wheel`, `keys`, `hovered`, `captured` and `wants_pointer()`,
which tells game input whether the pointer belongs to the UI.

## Themed widgets

`themed::` builders give the look of the immediate mode widgets, styled from `UiTheme` every frame:

| Kind | Builders |
| --- | --- |
| Text | `label`, `colored_label`, `label_right(text, width)`, `legend(text, width)`, `strong`, `heading`, `caption(text, color)` |
| Layout | `row()`, `column()`, `separator()`, `vseparator()`, `space(px)` |
| Windows | `frame(Option<width>)` (untitled; anchor or position it), `panel(title, pos, width)` (dragged by its title) |
| Controls | `button`, `button_tone(text, Tone::Primary \| Danger)`, `checkbox(text, checked)`, `toggle(text, on)` (a `Checkbox`), `slider(Slider, suffix, decimals)`, `slider_bar(Slider, width)`, `text_input(hint, width)`, `number_input(text, width)`, `segmented(options, selected)` |
| Lists | `list_row(index, ListItem::new(text).swatch(c).trailing(t))` (selects in the `RadioGroup` above), `collapsible(title, open, children)`, `section(..)` (shows a count), `scroll_rows(visible, children)` (add `.with(RadioGroup::default())` to select rows) |
| Readouts | `led(text, lit, color)` (a `themed::Led`), `keycap(text)`, `seven_segment(text, digits, lit)` (a `themed::SevenSegment`) |

- **Text:** a themed widget's text is a `themed::Label { text, size, bold, align, color }`
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
  `list_row`, `header`, `field_border`, `option`, `led`, `segments`, `scroll_thumb` and
  `separator`.

## Gotchas

- **Locks.** Component and resource locks aren't reentrant. Drop a query or guard before
  spawning, despawning, or locking the same component through `world.handle(..)`. Collect
  ids in one loop, then act on them.
- **Fixed components.** An entity's components are fixed when it's spawned. Give widgets
  everything they'll need up front: `.interactive()` and `ThemeOverride::default()`.
- **Layering.** Entity UI draws under immediate-mode windows. `z_index` only orders entity
  nodes among themselves.
- **Scrolling** is by whole rows: `ScrollRows` hides rows rather than clipping them.
- **Typing** reaches `TextInput`s through window keyboard events, so the window needs focus.
- **Headless testing:** headless apps get no input. A test can send
  `gearbox::WindowEvent(winit::event::WindowEvent::CursorMoved { .. })` and `MouseInput`
  through `world.send_event` (use `winit::event::DeviceId::dummy()`). Key events can't be
  built, so write `TextInput::text` directly instead.
