---
name: ui-layout
description: Use when building UI with gearbox's ui module (gearbox::ui) - adding UIPlugin, laying nodes out with CSS flexbox (flex_direction, justify_content, align_items, gap, grow/shrink/basis, wrap, grid), sizing with Val (Px, Percent, PercentWidth/PercentHeight), the box model (margin, padding, border), anchoring UI to parts of the window or filling the screen (Anchor), absolute and fixed positioning, z_index, overflow with scrolling and scrollbars, text with SDFFont (multiline, wrapping, clipping, colored/underlined TextSpans with hovers), colors and backgrounds (including vector shapes), and UINodeSDFRoot node trees with their hover/press events and listeners. For widgets see the gearbox ui-entity-widgets and ui-immediate-widgets skills.
---

# gearbox ui: layout and nodes

`gearbox::ui` draws UI as signed distance fields in one fullscreen pass, over the rest of a
gearbox app. It's always compiled in, no feature needed. Layout follows CSS flexbox through
[taffy](https://github.com/DioxusLabs/taffy). The API is early and changes; if this skill and the
compiler disagree, trust the compiler.

## Setup

gearbox and its siblings are path dependencies, so the app sits next to them (`../gearbox`,
`../anarchy`, `../shader_magician`):

```toml
[dependencies]
gearbox = { path = "../gearbox" }
anarchy = { path = "../anarchy" }   # for #[derive(Component)] / #[derive(Resource)]
winit = "0.30"                      # only to build or match window events
log = "0.4"                         # the snippets log with it
```

```rust
use std::sync::Arc;

use gearbox::{App, GearboxRenderPlugin, anyhow};
use gearbox::ui::*;

fn main() -> anyhow::Result<()> {
    let font = Arc::new(SDFFont::new(include_bytes!("../assets/Font.ttf"))?);
    App::new()
        .add_plugin(GearboxRenderPlugin) // clears and draws the scene the UI goes over
        .add_plugin(UIPlugin)            // layout, picking and the UI render pass
        .add_plugin(WidgetPlugin::new(font)) // widgets, theme and fonts; optional
        .run()
}
```

- `SDFFont::new` takes `&'static [u8]` of a TTF, so load it with `include_bytes!`. Share it
  as an `Arc<SDFFont>`.
- UI is drawn in the render graph's `End` pin, over the main pass. Put your UI systems in
  `on_render_update` (the `Normal` pin), not `on_update`.

## Two ways to hold a tree

| | Entity nodes (preferred) | `UINodeSDFRoot` |
| --- | --- | --- |
| What | one entity per node, a `Node(Style)` component, children through anarchy's `Hierarchy` | one component holding a whole `UINode` tree |
| Build | `UiBundle` + `world.spawn(bundle.into_entity_builder())` (entity-widgets skill) | `UINode::new(id)` + `add` |
| Sizes | logical pixels, scaled by the window's scale factor | window pixels, not scaled |
| Widgets | yes | no, raw nodes only |
| Input | `Interaction` component, widget events | `UINode*Event`s and listeners |

Both use the same `Style`, described below. Entity nodes draw under `UINodeSDFRoot` trees.

## Style

Set with `Style`'s setters (`set_width(..)`, which return `&mut Style`) on a `UINode` or
`node.style_mut()`, or with `Node`'s consuming builders (`Node::row().width(px(10.0))`).

- **Flexbox:**
  - `flex_direction`: `Row`, `Column` (the default, unlike CSS), `RowReverse`,
    `ColumnReverse`.
  - `justify_content` (main axis): `Start`, `End`, `Center`, `SpaceBetween`, `SpaceAround`,
    `SpaceEvenly`.
  - `align_items` (cross axis): `Start`, `End`, `Center`, `Stretch` (the default),
    `Baseline`. `align_self` overrides it for one child.
  - `flex_wrap` with `align_content`, `row_gap`/`column_gap` (`set_gap` sets both),
    `flex_grow`, `flex_shrink` (default 1) and `flex_basis`.
  - Shorthands: `set_row(justify, align)`, `set_column(justify, align)` and
    `set_flex(grow, shrink, basis)`.
- **`Val`:**
  - `Auto` sizes by content and layout, and is zero for margins, padding, borders and gaps.
  - `Px(f32)`.
  - `Percent(f32)` is a fraction of the parent (`0.5` = half).
  - `PercentWidth`/`PercentHeight` are fractions of the window.
  - `px(..)` and `percent(..)` make them.
- **Size:** `width`, `height`, `min_*`, `max_*`, and `aspect_ratio` (width/height, 0 = the
  window's).
- **Box model:** `margin`, `padding` (`Rect::single`, `Rect::new(top, bottom, left, right)`,
  `Rect::new_left`, ...) and `border` (a width). Border and padding are inside the node's size,
  like `box-sizing: border-box`.
- **Display:** `Flex` (default), `Grid` (equal cells, `grid_columns` wide or square-ish), or
  `None` (hidden, no room, not picked).
- **Overflow:** `overflow_x`/`overflow_y` (`set_overflow` sets both), see Scrolling below.
- **Look:**
  - `background` (`Background::Color(Vec4)`, `Image(TextureHandle)`, `Shape(Outline, Vec4)`
    for a filled vector shape over the content box, or `Empty`);
  - `border_color`;
  - `border_radius` (`RectCorners::single(px(6.0))`; `Percent(0.5)` makes a pill or circle).
- **Text:** `set_text(Some(Text::new(font, content, color, font_size)))`, then
  `.with_align(horizontal, vertical)` (both `Start` by default), `.with_wrap()` to wrap words
  at the content box's width, and `.with_clip()` to hide what falls outside the content box
  (`scroll` moves clipped text). Newlines start new lines. A text node with no children is as
  big as its text plus padding.
  - **Spans:** `.with_span(TextSpan::new(range))` or `with_spans(..)` draw byte ranges of the
    text differently: `.color(..)`, `.background(..)`, `.underline(..)`, `.wavy(..)` (a
    squiggle, like an error marker) and `.hover(text)`. Later spans win where they overlap.
    Hovered spans show in themed tooltips and send `TextHover { entity, span, index, text }`
    (also `UiInput::hovered_span`).
  - `SDFFont::layout(text, size, wrap)` gives the `TextLayout` text is drawn with: lines,
    glyph positions, `index_at(point)`, `caret_at(index)` and `range_rects(range)`.

### Colors

Colors are linear `Vec4` RGBA for an sRGB surface. Use `srgb(0xF2A33A)` for hex colors, and
`Vec4::ZERO` for transparent.

## Placing nodes

- **Anchors**, the usual way to place roots: `set_anchored(Anchor::X)` / `.anchored(..)` takes
  a node out of the flow and pins it to part of its parent, or of the window for a root. Its
  margins keep it in from the edges it touches.
  - `TopLeft` … `BottomRight` (9 points) keep the node's own size.
  - `FillTop`, `FillBottom`, `FillLeft` and `FillRight` stretch it along that edge.
  - `Fill` covers the whole parent, or the whole screen for a root.
- **Absolute:** `PositionType::Absolute(Rect)` / `.absolute(..)` places a node by offsets from
  its parent's edges. Set opposite edges to stretch it between them.
- **Fixed:** `PositionType::Fixed(Rect)` / `.fixed(..)` places a node by offsets from the
  window's edges and lays it out and draws it as a root, after the others, so the nodes it's
  under can't cut it off or scroll it. Like CSS `position: fixed`, for menus and popups that
  belong to a widget. It's still hidden with a hidden ancestor.
- **Roots** that aren't anchored or absolute stack as a column filling the window.
- **Paint order:** children draw over their parent, later siblings over earlier ones, and
  anchored or absolute nodes over flowing ones. A higher `z_index` draws over siblings (and
  over other roots).

```rust
use gearbox::ui::*;
use gearbox::glam::Vec4;

/// A toolbar across the top of the screen, a dimmed overlay over everything, and a centered card.
fn screens() -> (Node, Node, Node) {
    let toolbar = Node::row().anchored(Anchor::FillTop).height(px(40.0))
        .justify(JustifyContent::SpaceBetween).align(AlignItems::Center)
        .padding(Rect::new(px(0.0), px(0.0), px(12.0), px(12.0)))
        .background(srgb(0x15151A));
    let overlay = Node::new().anchored(Anchor::Fill).background(Vec4::new(0.0, 0.0, 0.0, 0.5)).z_index(10);
    let card = Node::column().anchored(Anchor::Center).width(px(320.0)).padding(Rect::single(px(14.0)))
        .gap(px(8.0)).radius(px(6.0)).background(srgb(0x202028)).border(1.0, srgb(0x33333D)).z_index(11);
    (toolbar, overlay, card)
}
```

## Scrolling

A node's children and text are always cut off at its edges. `overflow_x`/`overflow_y` say what
else happens to content that doesn't fit along each axis, like CSS `overflow`:

- `Overflow::Clip` (default): nothing more.
- `Overflow::Scroll`: cut off inside the border and scrolled, with room for a scrollbar always
  kept along the right (vertical) or bottom (horizontal) edge. The node can shrink below its
  content, so give it a size or `max_*`.
- `Overflow::Auto`: like `Scroll`, but the bar and its room only appear while the content
  doesn't fit.

```rust
use gearbox::ui::*;

/// A column that grows with its rows up to 200 logical pixels, then scrolls.
fn list(rows: impl IntoIterator<Item = UiBundle>) -> UiBundle {
    UiBundle::new(Node::column().gap(px(4.0)).max_height(px(200.0)).overflow_y(Overflow::Auto)
            // the immediate mode look; leave it out for plain grey bars
            .scrollbar(Theme::default().scrollbar()))
        .children(rows)
}
```

- With the `UIPlugin`, entity nodes scroll by the wheel (Shift turns it sideways) while the
  pointer is over them, the innermost one that can still move taking it; by dragging the
  thumb; and by a page when the track is clicked.
- `Style::scroll` holds the offset (logical pixels for entity nodes). Set it to scroll in code;
  layout keeps it in range. `UINodeSDFRoot` trees aren't scrolled by input, so set it yourself.
- `ComputedNode::scroll` (`LastState::scroll` for `UINode`s) gives last frame's `Scrolled`:
  the `view` box, `offset`, `max` and the bars' rects. `ComputedNode::clip` is the part of the
  window a node shows in; picking ignores what's cut off.
- `Style::scrollbar` (`Scrollbar { width, gap, track, thumb, thumb_hovered }`) is how bars
  look. The thumb is `thumb_hovered` while the pointer is over the node.
- Long text scrolls the same way: give a wrapped text node a parent with a `max_height` and
  `overflow_y(Overflow::Auto)`, or an unwrapped one `overflow_x`.

## `UINodeSDFRoot` trees

```rust
use std::sync::Arc;

use gearbox::{anarchy::{Event, World}, glam::Vec4, spawn};
use gearbox::ui::*;

fn spawn_hello(world: &World, font: Arc<SDFFont>) {
    let mut button = UINode::new("hello".to_string()); // an id makes it send events
    button.set_anchored(Anchor::BottomRight);
    button.set_padding(Rect::single(Val::Px(8.0)));
    button.set_margin(Rect::single(Val::Px(20.0)));
    button.set_background(Background::Color(srgb(0x2A2A33)));
    button.set_text(Some(Text::new(font, "Hello", Vec4::ONE, 20.0).with_align(Align::Start, Align::Center)));
    button.set_listener(|_world, interaction| log::info!("now {interaction:?}"));
    spawn(world, vec![Box::new(UINodeSDFRoot(button))]);
}

fn on_press(pressed: Event<UINodePressedEvent>) {
    for event in pressed.read() { log::info!("pressed {}", event.id); }
}
```

- Nodes with an id send `UINodeStartHoverEvent`, `UINodeEndHoverEvent`, `UINodePressedEvent`
  and `UINodeReleasedEvent`, and call their listener with the new `Interaction`.
- `node.last_state()` gives where a node was laid out (`position` is its center) and its
  interaction.
- To change a tree, edit the `UINodeSDFRoot` component through a query or entity handle.
- The tree is cloned every frame, so keep these trees small. Clones share `last_state` and
  listeners.

## Gotchas

- **Element limits.** Each glyph is one element. Where the GPU has storage buffers (desktop)
  the SDF buffers hold 16384 elements of each kind; on WebGL2 they're uniform buffers holding
  a few hundred to a few thousand. Nodes and lines of text that are cut off (outside the
  window, a scrolling node's view, or the content box of clipped text) aren't drawn, so long
  scrolled lists and text only cost what shows.
- **Auto margins** are zero, not CSS's centering margins. Center with `justify`/`align` or an
  anchor.
- **Borders take room.** A 1px border with 8px padding leaves the content 9px in from the
  edge.
- **Startup warning.** "Missing bind group for bindless textures" can be logged once on the
  first frame, while gearbox's blank texture loads. It's harmless.
