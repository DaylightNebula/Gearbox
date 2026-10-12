//! Ready-styled entity widgets with the immediate mode look, built from the behavior-only
//! widgets in `controls`. Each builder returns a `UiBundle` to spawn with `spawn_ui`, and
//! `restyle_themed` keeps every themed widget's look in step with its state and the `UiTheme`.

use anarchy::{Component, ComponentMeta, EntityID, Query, Res, World};
use magician_vgpu::glam::{Vec2, Vec4};

use crate::ui::{
    Align, AlignItems, Background, Button, Checkbox, Collapsible, ComputedNode, Display, DragHandle, Interaction, JustifyContent,
    Node, PositionType, RadioButton, RadioGroup, Rect, RectCorners, ScrollItem, ScrollRows, Slider, Style, Text, TextInput, Theme,
    Tone, Tooltip, UiBundle, UiFocus, UiInput, UiTheme, Val, local_theme, px,
    widgets::{
        CHECK, DIGIT, FONT_SIZE, GAP, GROOVE, HEADING_FONT_SIZE, INDENT, KNOB, LED, LIP, MARKER, NONE, PAD, RADIUS, ROW, SCROLLBAR,
        SEG_A, SEG_B, SEG_C, SEG_D, SEG_E, SEG_F, SEG_G, SEGMENT, SLIDER, SMALL_FONT_SIZE, SWATCH, SWITCH, TEXT_INSET, segments,
    },
};

/// How big a `Label`'s text is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextSize {
    #[default]
    Normal,
    Small,
    Heading,
}

impl TextSize {
    /// Logical pixels.
    pub fn px(self) -> f32 {
        match self {
            TextSize::Normal => FONT_SIZE,
            TextSize::Small => SMALL_FONT_SIZE,
            TextSize::Heading => HEADING_FONT_SIZE,
        }
    }
}

/// Text a node shows in the `UiTheme`'s fonts. Change `text` to change what it says.
#[derive(Clone, Debug, Component)]
pub struct Label {
    pub text: String,
    pub size: TextSize,
    pub bold: bool,
    pub align: Align,
    /// Overrides the theme's text color.
    pub color: Option<Vec4>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), size: TextSize::Normal, bold: false, align: Align::Start, color: None }
    }

    fn sized(mut self, size: TextSize, bold: bool) -> Self { self.size = size; self.bold = bold; self }

    fn colored(mut self, color: Vec4) -> Self { self.color = Some(color); self }

    fn aligned(mut self, align: Align) -> Self { self.align = align; self }
}

/// An indicator light's state, see `led`.
#[derive(Clone, Copy, Debug, Component)]
pub struct Led { pub lit: bool, pub color: Vec4 }

/// What a seven segment display shows, see `seven_segment`.
#[derive(Clone, Debug, Component)]
pub struct SevenSegment {
    pub text: String,
    /// Lights the segments in the accent.
    pub lit: bool,
}

/// What a themed widget is, so `restyle_themed` knows how to style it and where its parts are.
#[derive(Clone, Debug, Component)]
pub enum Themed {
    Frame,
    Separator,
    Button { tone: Tone },
    Checkbox,
    Toggle,
    /// A slider track, its value shown by its parent's second child with `suffix` after it.
    Slider { value_label: bool, suffix: String, decimals: usize, width: f32 },
    TextInput { width: f32 },
    Segmented,
    ListRow,
    Header,
    Scroll,
    Led,
    SevenSegment { digits: usize },
    Keycap,
    Tooltip,
}

/// What a `list_row` shows.
#[derive(Clone, Debug, Default)]
pub struct ListItem {
    pub text: String,
    /// A chip of color before the text.
    pub swatch: Option<Vec4>,
    /// Dim text at the right end.
    pub trailing: Option<String>,
}

impl ListItem {
    pub fn new(text: impl Into<String>) -> Self { Self { text: text.into(), ..Default::default() } }

    pub fn swatch(mut self, color: Vec4) -> Self { self.swatch = Some(color); self }

    pub fn trailing(mut self, text: impl Into<String>) -> Self { self.trailing = Some(text.into()); self }
}

impl From<&str> for ListItem {
    fn from(text: &str) -> Self { Self::new(text) }
}

impl From<String> for ListItem {
    fn from(text: String) -> Self { Self::new(text) }
}

fn zero() -> Val { px(0.0) }

/// Text insets on both sides.
fn insets(left: f32, right: f32) -> Rect { Rect::new(zero(), zero(), px(left), px(right)) }

/// A node laid out at the size it asks for, like immediate mode widgets.
fn fixed() -> Node { Node::new().shrink(0.0) }

/// A line of text.
pub fn label(text: impl Into<String>) -> UiBundle {
    UiBundle::new(fixed().height(px(ROW)).padding(insets(TEXT_INSET, TEXT_INSET))).with(Label::new(text))
}

/// A line of text in `color`.
pub fn colored_label(text: impl Into<String>, color: Vec4) -> UiBundle {
    UiBundle::new(fixed().height(px(ROW)).padding(insets(TEXT_INSET, TEXT_INSET))).with(Label::new(text).colored(color))
}

/// Right aligned text `width` logical pixels wide, for columns of numbers.
pub fn label_right(text: impl Into<String>, width: f32) -> UiBundle {
    UiBundle::new(fixed().size(px(width), px(ROW)).padding(insets(TEXT_INSET, TEXT_INSET))).with(Label::new(text).aligned(Align::End))
}

/// A dim name for the widgets after it, `width` logical pixels wide so a column of them lines up.
pub fn legend(text: impl Into<String>, width: f32) -> UiBundle {
    UiBundle::new(fixed().size(px(width), px(ROW))).with(Label::new(text)).with(Dim)
}

/// Text in the bold font.
pub fn strong(text: impl Into<String>) -> UiBundle {
    UiBundle::new(fixed().height(px(ROW))).with(Label::new(text).sized(TextSize::Normal, true))
}

/// Large bold text, for what a window is about.
pub fn heading(text: impl Into<String>) -> UiBundle {
    UiBundle::new(fixed().height(px(HEADING_FONT_SIZE * 1.4))).with(Label::new(text).sized(TextSize::Heading, true))
}

/// Small text in `color`, for notes under widgets.
pub fn caption(text: impl Into<String>, color: Vec4) -> UiBundle {
    UiBundle::new(fixed().height(px(SMALL_FONT_SIZE * 1.5))).with(Label::new(text).sized(TextSize::Small, false).colored(color))
}

/// Marks a label drawn in the theme's dim color.
#[derive(Clone, Copy, Debug, Component)]
struct Dim;

/// A line across a column.
pub fn separator() -> UiBundle { UiBundle::new(fixed().height(px(2.0))).with(Themed::Separator) }

/// A divider between the widgets of a row.
pub fn vseparator() -> UiBundle { UiBundle::new(fixed().size(px(2.0), px(ROW))).with(Themed::Separator) }

/// Empty room `px` logical pixels along a column.
pub fn space(height: f32) -> UiBundle { UiBundle::new(fixed().height(px(height))) }

/// Lays its children out left to right.
pub fn row() -> UiBundle { UiBundle::new(Node::row().gap(px(GAP)).align(AlignItems::Center).shrink(0.0)) }

/// Lays its children out top to bottom.
pub fn column() -> UiBundle { UiBundle::new(Node::column().gap(px(GAP)).shrink(0.0)) }

/// A window without a title, as wide as `width` logical pixels or its content. Anchor or
/// position it, and add widgets as children.
pub fn frame(width: Option<f32>) -> UiBundle {
    let mut node = Node::column().gap(px(GAP)).padding(Rect::single(px(PAD - 1.0))).border(1.0, NONE);
    if let Some(width) = width { node = node.width(px(width)); }
    UiBundle::new(node).with(Themed::Frame)
}

/// A window titled `title` at `pos`, dragged around by its title. Add widgets as children.
pub fn panel(title: impl Into<String>, pos: Vec2, width: f32) -> UiBundle {
    let title = UiBundle::new(fixed().height(px(ROW)).grow(1.0).padding(Rect::new_left(px(2.0))))
        .with(Label::new(title).sized(TextSize::Normal, true));
    let bar = UiBundle::new(Node::row().height(px(ROW)).align(AlignItems::Center).shrink(0.0))
        .interactive()
        .with(DragHandle::default())
        .child(title);
    frame(Some(width))
        .style(|node| node.absolute(Rect::new_top_left(px(pos.y), px(pos.x))))
        .child(bar)
        .child(separator())
}

/// A key-like button, sending `Activate` when clicked.
pub fn button(text: impl Into<String>) -> UiBundle { button_tone(text, Tone::Normal) }

/// A `button` standing out as `tone` says.
pub fn button_tone(text: impl Into<String>, tone: Tone) -> UiBundle {
    let face = UiBundle::new(Node::new().height(px(ROW - LIP)).padding(insets(TEXT_INSET + 2.0, TEXT_INSET + 2.0)))
        .with(Label::new(text).aligned(Align::Center));
    UiBundle::new(fixed().height(px(ROW)))
        .with(Button::default())
        .interactive()
        .with(Themed::Button { tone })
        .child(face)
}

/// A checkbox ticked while `checked`, then `text`.
pub fn checkbox(text: impl Into<String>, checked: bool) -> UiBundle {
    let tick = UiBundle::new(fixed().size(px(CHECK / 2.0), px(CHECK / 2.0)));
    let check = UiBundle::new(fixed().size(px(CHECK), px(CHECK)).justify(JustifyContent::Center).align(AlignItems::Center).border(1.0, NONE))
        .child(tick);
    UiBundle::new(Node::row().height(px(ROW)).align(AlignItems::Center).shrink(0.0))
        .with(Checkbox::new(checked))
        .interactive()
        .with(Themed::Checkbox)
        .child(check)
        .child(label(text))
}

/// A sliding switch lit while `on`, then `text`. Uses a `Checkbox` for its state.
pub fn toggle(text: impl Into<String>, on: bool) -> UiBundle {
    let knob_size = SWITCH.y - 4.0;
    let knob = UiBundle::new(fixed().size(px(knob_size), px(knob_size)));
    let track = UiBundle::new(Node::row().size(px(SWITCH.x), px(SWITCH.y)).align(AlignItems::Center).padding(Rect::single(px(1.0))).border(1.0, NONE).shrink(0.0))
        .child(knob);
    UiBundle::new(Node::row().height(px(ROW)).align(AlignItems::Center).shrink(0.0))
        .with(Checkbox::new(on))
        .interactive()
        .with(Themed::Toggle)
        .child(track)
        .child(label(text))
}

fn slider_track(slider: Slider, width: f32, value_label: bool, suffix: String, decimals: usize) -> UiBundle {
    let groove = || UiBundle::new(fixed().height(px(GROOVE)));
    let knob = UiBundle::new(fixed().width(px(KNOB.x)).border(1.0, NONE));
    UiBundle::new(Node::row().size(px(width), px(ROW)).align(AlignItems::Center).shrink(0.0))
        .with(slider)
        .interactive()
        .with(Themed::Slider { value_label, suffix, decimals, width })
        .children([groove(), knob, groove()])
}

/// A slider followed by its value with `decimals` decimals and `suffix`.
pub fn slider(slider: Slider, suffix: impl Into<String>, decimals: usize) -> UiBundle {
    row().child(slider_track(slider, SLIDER, true, suffix.into(), decimals)).child(label(""))
}

/// Just the groove and knob of a `slider`, `width` logical pixels wide.
pub fn slider_bar(slider: Slider, width: f32) -> UiBundle { slider_track(slider, width, false, String::new(), 0) }

fn field(input: TextInput, width: f32) -> UiBundle {
    // the border sits inside the padding
    UiBundle::new(fixed().size(px(width), px(ROW)).padding(insets(TEXT_INSET - 1.0, TEXT_INSET - 1.0)).border(1.0, NONE))
        .with(input)
        .interactive()
        .with(Label::new(""))
        .with(Themed::TextInput { width })
}

/// A single line text field `width` logical pixels wide, showing `hint` while empty.
pub fn text_input(hint: impl Into<String>, width: f32) -> UiBundle { field(TextInput::default().hint(hint), width) }

/// A field showing `text` until clicked, then editing a fresh draft that Enter or a click
/// elsewhere hands back, for numbers. Read it with `Submit`, and set `TextInput::text` to fix
/// text that doesn't parse.
pub fn number_input(text: impl Into<String>, width: f32) -> UiBundle { field(TextInput::new(text).replace_on_focus(), width) }

/// One of `options` laid out as keys in a recessed strip, the selected one raised. A
/// `RadioGroup` holds the selection.
pub fn segmented(options: impl IntoIterator<Item = impl Into<String>>, selected: usize) -> UiBundle {
    let options = options.into_iter().enumerate().map(|(index, option)| {
        UiBundle::new(fixed().height(px(ROW - 4.0)))
            .with(RadioButton { index })
            .interactive()
            .with(Label::new(option).aligned(Align::Center))
    });
    UiBundle::new(Node::row().height(px(ROW)).align(AlignItems::Center).padding(Rect::single(px(1.0))).border(1.0, NONE).shrink(0.0))
        .with(RadioGroup { selected: Some(selected), disabled: false })
        .with(Themed::Segmented)
        .children(options)
}

/// A list row selecting `index` in the `RadioGroup` above it when clicked. Rows count for a
/// `scroll_rows` above them.
pub fn list_row(index: usize, item: impl Into<ListItem>) -> UiBundle {
    let item = item.into();
    let marker = UiBundle::new(fixed().size(px(MARKER), px(ROW - 8.0)));
    let mut row = UiBundle::new(Node::row().height(px(ROW)).align(AlignItems::Center).shrink(0.0))
        .with(RadioButton { index })
        .with(ScrollItem)
        .interactive()
        .with(Themed::ListRow)
        .child(marker);
    if let Some(color) = item.swatch {
        let chip = Node::new().size(px(SWATCH), px(SWATCH)).margin(Rect::new_left(px(5.0))).background(color)
            .radius(px(2.0)).border(1.0, Theme::default().border).shrink(0.0);
        row = row.child(UiBundle::new(chip).with(Swatch));
    }
    row = row.child(UiBundle::new(Node::new().grow(1.0).padding(Rect::new_left(px(6.0)))).with(Label::new(item.text)));
    if let Some(trailing) = item.trailing {
        let tail = UiBundle::new(fixed().padding(Rect::new_right(px(TEXT_INSET))))
            .with(Label::new(trailing).sized(TextSize::Small, false).aligned(Align::End))
            .with(Dim);
        row = row.child(tail);
    }
    row
}

/// Marks a list row's color chip.
#[derive(Clone, Copy, Debug, Component)]
struct Swatch;

fn header(title: String, count: Option<usize>) -> UiBundle {
    let sign = UiBundle::new(fixed().width(px(14.0))).with(Label::new("+").sized(TextSize::Normal, true).aligned(Align::Center)).with(Dim);
    let title = UiBundle::new(Node::new().grow(1.0).padding(Rect::new_left(px(4.0)))).with(Label::new(title).sized(TextSize::Normal, true));
    let mut header = UiBundle::new(Node::row().height(px(ROW)).align(AlignItems::Center).shrink(0.0))
        .interactive()
        .with(ScrollItem)
        .with(Themed::Header)
        .children([sign, title]);
    if let Some(count) = count {
        header = header.child(UiBundle::new(fixed().padding(Rect::new_right(px(TEXT_INSET))))
            .with(Label::new(count.to_string()).sized(TextSize::Small, false).aligned(Align::End))
            .with(Dim));
    }
    header
}

/// A collapsing header over `children`, shown indented while open.
pub fn collapsible(title: impl Into<String>, open: bool, children: impl IntoIterator<Item = UiBundle>) -> UiBundle {
    collapsing(title.into(), open, children.into_iter().collect(), false)
}

/// A `collapsible` showing how many children it holds at its right end, for sections of a list.
pub fn section(title: impl Into<String>, open: bool, children: impl IntoIterator<Item = UiBundle>) -> UiBundle {
    collapsing(title.into(), open, children.into_iter().collect(), true)
}

fn collapsing(title: String, open: bool, children: Vec<UiBundle>, counted: bool) -> UiBundle {
    let count = counted.then_some(children.len());
    let content = UiBundle::new(Node::column().gap(px(GAP)).shrink(0.0)).children(children);
    UiBundle::new(Node::column().gap(px(GAP)).shrink(0.0))
        .with(Collapsible { open })
        .child(header(title, count))
        .child(content)
}

/// Shows up to `visible` of the list rows and headers in `children` at a time, scrolled by the
/// wheel, with a scrollbar while there are more. Add a `RadioGroup` with `.with(..)` to select
/// its rows.
pub fn scroll_rows(visible: usize, children: impl IntoIterator<Item = UiBundle>) -> UiBundle {
    let content = UiBundle::new(Node::column().gap(px(GAP)).grow(1.0)).with(ScrollRows::new(visible)).children(children);
    let thumb = UiBundle::new(Node::new());
    let track = UiBundle::new(fixed().width(px(SCROLLBAR)).margin(Rect::new_left(px(GAP)))).child(thumb);
    UiBundle::new(Node::row().align(AlignItems::Start).shrink(0.0))
        .with(Themed::Scroll)
        .children([content, track])
}

/// An indicator light in `color` while `lit`, with `text` after it. Change its `Led` to change it.
pub fn led(text: impl Into<String>, lit: bool, color: Vec4) -> UiBundle {
    let halo = LED * 1.9;
    let dot = UiBundle::new(fixed().size(px(LED), px(LED)).border(1.0, NONE));
    let glow = UiBundle::new(fixed().size(px(halo), px(halo)).justify(JustifyContent::Center).align(AlignItems::Center)).child(dot);
    let label = UiBundle::new(Node::new().padding(Rect::new_left(px(4.0)))).with(Label::new(text));
    UiBundle::new(Node::row().height(px(ROW)).align(AlignItems::Center).shrink(0.0))
        .with(Led { lit, color })
        .with(Themed::Led)
        .children([glow, label])
}

/// A key cap reading `text`, for showing which input does what.
pub fn keycap(text: impl Into<String>) -> UiBundle {
    let height = SMALL_FONT_SIZE + 7.0;
    let face = UiBundle::new(Node::new().height(px(height - LIP)).padding(insets(5.0, 5.0)))
        .with(Label::new(text).sized(TextSize::Small, true).aligned(Align::Center));
    let cap = UiBundle::new(fixed().height(px(height))).child(face);
    UiBundle::new(Node::column().height(px(ROW)).justify(JustifyContent::Center).align(AlignItems::Start).shrink(0.0))
        .with(Themed::Keycap)
        .child(cap)
}

/// A seven segment display `digits` wide showing `text` right aligned, lit in the accent while
/// `lit`. Change its `SevenSegment` to change it.
pub fn seven_segment(text: impl Into<String>, digits: usize, lit: bool) -> UiBundle {
    let (t, gap, inset) = (SEGMENT, 2.0, 6.0);
    let half = (DIGIT.y - 3.0 * t) / 2.0;
    let segment = |w: f32, h: f32, left: f32| UiBundle::new(fixed().size(px(w), px(h)).margin(Rect::new_left(px(left))).radius(px(t / 2.0)));
    let bar = || segment(DIGIT.x - 2.0 * t, t, t);
    let sides = || UiBundle::new(Node::row().size(px(DIGIT.x), px(half)).align(AlignItems::Start).shrink(0.0))
        .children([segment(t, half, 0.0), segment(t, half, DIGIT.x - 2.0 * t)]);
    let cells = (0..digits).flat_map(|_| {
        let digit = UiBundle::new(fixed().size(px(DIGIT.x), px(DIGIT.y))).children([bar(), sides(), bar(), sides(), bar()]);
        let dot = segment(t, t, gap / 2.0);
        let point = UiBundle::new(Node::column().size(px(t + gap), px(DIGIT.y)).justify(JustifyContent::End).align(AlignItems::Center).shrink(0.0)).child(dot);
        [digit, point]
    });
    let width = 2.0 * inset - gap + digits as f32 * (DIGIT.x + t + gap);
    UiBundle::new(Node::row().size(px(width), px(ROW)).justify(JustifyContent::End).align(AlignItems::Center)
        .padding(insets(inset - 1.0, inset - gap - 1.0)).border(1.0, NONE).shrink(0.0))
        .with(SevenSegment { text: text.into(), lit })
        .with(Themed::SevenSegment { digits })
        .children(cells)
}

/// The window `restyle_themed` shows hints in. Spawned once by the `WidgetPlugin`.
pub(crate) fn tooltip_window() -> UiBundle {
    let node = Node::column().padding(Rect::single(px(PAD - 1.0))).border(1.0, NONE)
        .absolute(Rect::new_top_left(zero(), zero())).z_index(i32::MAX).display(Display::None);
    UiBundle::new(node).with(Themed::Tooltip).child(label(""))
}

// ── restyling ──────────────────────────────────────────────────────────────────

/// The child at `path` below `id`.
fn child(world: &World, id: EntityID, path: &[usize]) -> Option<EntityID> {
    path.iter().try_fold(id, |id, &idx| world.hierarchy().children(id)?.get(idx).copied())
}

fn children(world: &World, id: EntityID) -> Vec<EntityID> {
    world.hierarchy().children(id).map(|children| children.to_vec()).unwrap_or_default()
}

fn get<T: ComponentMeta + Clone>(world: &World, id: EntityID) -> Option<T> {
    world.handle(id).get::<&T>().ok().flatten().map(|found| found.clone())
}

fn has<T: ComponentMeta>(world: &World, id: EntityID) -> bool {
    world.handle(id).get::<&T>().ok().flatten().is_some()
}

/// Changes the style of `id`'s node, if it has one.
fn style(world: &World, id: Option<EntityID>, change: impl FnOnce(&mut Style)) {
    if let Some(id) = id && let Ok(Some(mut node)) = world.handle(id).get::<&mut Node>() { change(node.style_mut()); }
}

fn label_of(world: &World, id: Option<EntityID>, change: impl FnOnce(&mut Label)) {
    if let Some(id) = id && let Ok(Some(mut label)) = world.handle(id).get::<&mut Label>() { change(&mut label); }
}

/// Fills `style` with `color` rounded by `radius`, `NONE` leaving it unfilled.
fn fill(style: &mut Style, color: Vec4, radius: f32) {
    if color == NONE {
        style.set_background(Background::Empty);
    } else {
        style.set_background(Background::Color(color));
        style.set_border_radius(RectCorners::single(px(radius)));
    }
}

/// How many `Collapsible`s `id` is inside.
fn depth(world: &World, id: EntityID) -> usize {
    world.hierarchy().ancestors(id).filter(|ancestor| has::<Collapsible>(world, *ancestor)).count()
}

fn measure(theme: &UiTheme, text: &str, size: f32, bold: bool) -> f32 {
    theme.font(bold).map_or(0.0, |font| font.measure_text(text, size).x)
}

/// What a themed widget's state looks like this frame.
struct Restyle<'a> {
    world: &'a World,
    theme: &'a UiTheme,
    input: &'a UiInput,
    focus: &'a UiFocus,
}

impl Restyle<'_> {
    fn hot(&self, id: EntityID) -> bool { self.input.hovered == Some(id) }

    fn held(&self, id: EntityID) -> bool { get::<Interaction>(self.world, id) == Some(Interaction::Pressed) }

    fn widget(&self, id: EntityID, themed: &Themed) {
        let local = local_theme(self.world, id, self.theme);
        let (world, theme) = (self.world, &local.theme);
        match themed {
            Themed::Frame => style(world, Some(id), |style| {
                fill(style, theme.panel, RADIUS + 1.0);
                style.set_border_color(Some(theme.border));
            }),
            Themed::Separator => style(world, Some(id), |style| { style.set_background(Background::Color(theme.separator())); }),
            Themed::Button { tone } => {
                let hot = self.hot(id);
                let (top, ink, lip) = theme.button(*tone, hot);
                style(world, Some(id), |style| fill(style, lip, RADIUS));
                let face = child(world, id, &[0]);
                let held = hot && self.held(id);
                style(world, face, |style| {
                    fill(style, top, RADIUS);
                    style.set_margin(Rect::new_top(px(if held { LIP } else { 0.0 })));
                });
                label_of(world, face, |label| label.color = Some(ink));
            }
            Themed::Checkbox => {
                let checked = get::<Checkbox>(world, id).is_some_and(|checkbox| checkbox.checked);
                style(world, child(world, id, &[0]), |style| {
                    fill(style, theme.well, RADIUS);
                    style.set_border_color(Some(theme.check_border(self.hot(id))));
                });
                style(world, child(world, id, &[0, 0]), |style| {
                    fill(style, theme.accent, 1.0);
                    style.set_display(if checked { Display::Flex } else { Display::None });
                });
            }
            Themed::Toggle => {
                let on = get::<Checkbox>(world, id).is_some_and(|checkbox| checkbox.checked);
                let (track, border, knob) = theme.switch(on, self.hot(id));
                style(world, child(world, id, &[0]), |style| {
                    fill(style, track, SWITCH.y / 2.0);
                    style.set_border_color(Some(border));
                    style.set_justify_content(if on { JustifyContent::End } else { JustifyContent::Start });
                });
                style(world, child(world, id, &[0, 0]), |style| fill(style, knob, (SWITCH.y - 4.0) / 2.0));
            }
            Themed::Slider { value_label, suffix, decimals, width } => {
                let Some(slider) = get::<Slider>(world, id) else { return };
                let enabled = !slider.disabled;
                let lit = enabled && (self.hot(id) || self.held(id));
                let (filled, knob) = theme.slider(enabled, lit);
                let before = (slider.fraction() as f32 * (width - KNOB.x)).max(0.01);
                let after = (width - KNOB.x - before).max(0.01);
                let groove = |style: &mut Style, color: Vec4, length: f32| {
                    style.set_width(px(length));
                    fill(style, color, GROOVE / 2.0);
                };
                style(world, child(world, id, &[0]), |style| groove(style, filled, before));
                style(world, child(world, id, &[1]), |style| {
                    style.set_height(px(if enabled { KNOB.y } else { KNOB.y * 0.75 }));
                    fill(style, knob, 2.0);
                    style.set_border_color(Some(theme.border));
                });
                style(world, child(world, id, &[2]), |style| groove(style, theme.well, after));
                if *value_label && let Some(parent) = world.hierarchy().parent(id) {
                    label_of(world, child(world, parent, &[1]), |label| {
                        label.text = format!("{:.*}{suffix}", *decimals, slider.value);
                        label.color = Some(if enabled { theme.text } else { theme.dim });
                    });
                }
            }
            Themed::TextInput { width } => {
                let Some(field) = get::<TextInput>(world, id) else { return };
                let focused = self.focus.focused == Some(id);
                let (shown, color) = match (focused, field.has_draft()) {
                    (true, true) if field.editing().is_empty() => (format!("{}|", field.text), theme.dim),
                    (true, _) => (format!("{}|", field.editing()), theme.text),
                    (false, _) if field.text.is_empty() => (field.hint.clone(), theme.dim),
                    (false, _) => (field.text.clone(), theme.text),
                };
                // the end of long text, where typing happens
                let room = width - 2.0 * TEXT_INSET;
                let mut start = 0;
                while measure(&local, &shown[start..], FONT_SIZE, false) > room && let Some(c) = shown[start..].chars().next() {
                    start += c.len_utf8();
                }
                style(world, Some(id), |style| {
                    fill(style, theme.well, RADIUS);
                    style.set_border_color(Some(theme.field_border(focused, self.hot(id))));
                });
                label_of(world, Some(id), |label| {
                    label.text = shown[start..].to_string();
                    label.color = Some(color);
                });
            }
            Themed::Segmented => {
                let selected = get::<RadioGroup>(world, id).and_then(|group| group.selected);
                style(world, Some(id), |style| {
                    fill(style, theme.well, RADIUS);
                    style.set_border_color(Some(theme.border));
                });
                for option in children(world, id) {
                    let Some(button) = get::<RadioButton>(world, option) else { continue };
                    let on = selected == Some(button.index);
                    let (background, color) = theme.option(on, self.hot(option));
                    let text = get::<Label>(world, option).map(|label| label.text).unwrap_or_default();
                    let width = measure(&local, &text, FONT_SIZE, true) + 2.0 * TEXT_INSET;
                    style(world, Some(option), |style| {
                        style.set_width(px(width));
                        fill(style, background, RADIUS - 1.0);
                    });
                    label_of(world, Some(option), |label| {
                        label.bold = on;
                        label.color = Some(color);
                    });
                }
            }
            Themed::ListRow => {
                let Some(button) = get::<RadioButton>(world, id) else { return };
                let group = world.hierarchy().ancestors(id).find_map(|ancestor| get::<RadioGroup>(world, ancestor));
                let selected = group.and_then(|group| group.selected) == Some(button.index);
                let (background, marker, text) = theme.list_row(selected, self.hot(id));
                let indent = depth(world, id) as f32 * INDENT;
                style(world, Some(id), |style| fill(style, background, RADIUS));
                style(world, child(world, id, &[0]), |style| {
                    fill(style, marker, MARKER / 2.0);
                    style.set_margin(Rect::new_left(px(indent + 2.0)));
                });
                for part in children(world, id).into_iter().skip(1) {
                    if has::<Swatch>(world, part) {
                        style(world, Some(part), |style| { style.set_border_color(Some(theme.border)); });
                    } else if !has::<Dim>(world, part) {
                        label_of(world, Some(part), |label| {
                            label.color = Some(text);
                            label.bold = selected;
                        });
                    }
                }
            }
            Themed::Header => {
                let open = world.hierarchy().parent(id).and_then(|parent| get::<Collapsible>(world, parent)).is_some_and(|c| c.open);
                let (background, title) = theme.header(self.hot(id));
                let indent = depth(world, id).saturating_sub(1) as f32 * INDENT;
                style(world, Some(id), |style| fill(style, background, RADIUS));
                let sign = child(world, id, &[0]);
                style(world, sign, |style| { style.set_margin(Rect::new_left(px(indent + 1.0))); });
                label_of(world, sign, |label| label.text = if open { "\u{2212}" } else { "+" }.to_string());
                label_of(world, child(world, id, &[1]), |label| label.color = Some(title));
            }
            Themed::Scroll => {
                let Some(content) = child(world, id, &[0]) else { return };
                let Some(list) = get::<ScrollRows>(world, content) else { return };
                let hovered = get::<ComputedNode>(world, id).is_some_and(|node| node.contains(self.input.cursor));
                let height = get::<ComputedNode>(world, content).map_or(0.0, |node| node.size.y / self.input.scale.max(f32::EPSILON));
                let overflows = list.rows > list.visible;
                style(world, child(world, id, &[1]), |style| fill(style, if overflows { theme.well } else { NONE }, SCROLLBAR / 2.0));
                style(world, child(world, id, &[1, 0]), |style| {
                    let rows = list.rows.max(1) as f32;
                    style.set_height(px(height * list.visible.min(list.rows) as f32 / rows));
                    style.set_margin(Rect::new_top(px(height * list.first.floor() / rows)));
                    fill(style, if overflows { theme.scroll_thumb(hovered) } else { NONE }, SCROLLBAR / 2.0);
                });
            }
            Themed::Led => {
                let Some(led) = get::<Led>(world, id) else { return };
                let (glow, dot, border, text) = theme.led(led.lit, led.color);
                style(world, child(world, id, &[0]), |style| fill(style, glow, LED * 1.9 / 2.0));
                style(world, child(world, id, &[0, 0]), |style| {
                    fill(style, dot, LED / 2.0);
                    style.set_border_color(Some(border));
                });
                label_of(world, child(world, id, &[1]), |label| label.color = Some(text));
            }
            Themed::SevenSegment { digits } => {
                let Some(display) = get::<SevenSegment>(world, id) else { return };
                let (on, off) = theme.segments(display.lit);
                style(world, Some(id), |style| {
                    fill(style, theme.well, RADIUS);
                    style.set_border_color(Some(theme.border));
                });
                let mut cells: Vec<(char, bool)> = vec![];
                for c in display.text.chars() {
                    match (c, cells.last_mut()) {
                        ('.', Some((_, dp @ false))) => *dp = true,
                        ('.', _) => cells.push((' ', true)),
                        (c, _) => cells.push((c, false)),
                    }
                }
                let cells = cells.split_off(cells.len().saturating_sub(*digits));
                let shown = std::iter::repeat_n((' ', false), digits - cells.len()).chain(cells);
                for (idx, (c, dp)) in shown.enumerate() {
                    let bits = segments(c);
                    let digit = child(world, id, &[2 * idx]);
                    // bars and side pairs top to bottom, as `seven_segment` lays them out
                    let parts = [(&[0][..], SEG_A), (&[1, 0], SEG_F), (&[1, 1], SEG_B), (&[2], SEG_G), (&[3, 0], SEG_E), (&[3, 1], SEG_C), (&[4], SEG_D)];
                    for (path, bit) in parts {
                        let lit = bits & bit != 0;
                        style(world, digit.and_then(|digit| child(world, digit, path)), |style| fill(style, if lit { on } else { off }, SEGMENT / 2.0));
                    }
                    style(world, child(world, id, &[2 * idx + 1, 0]), |style| fill(style, if dp { on } else { off }, SEGMENT / 2.0));
                }
            }
            Themed::Keycap => {
                let cap = child(world, id, &[0]);
                let height = SMALL_FONT_SIZE + 7.0;
                let text = child(world, id, &[0, 0]).and_then(|face| get::<Label>(world, face)).map(|label| label.text).unwrap_or_default();
                let width = measure(&local, &text, SMALL_FONT_SIZE, true) + 10.0;
                style(world, cap, |style| {
                    style.set_width(px(width));
                    fill(style, theme.border, RADIUS);
                });
                style(world, child(world, id, &[0, 0]), |style| {
                    style.set_height(px(height - LIP));
                    fill(style, theme.widget, RADIUS);
                });
            }
            Themed::Tooltip => {
                let hint = self.input.hovered.and_then(|hovered| get::<Tooltip>(world, hovered));
                let scale = self.input.scale.max(f32::EPSILON);
                let at = self.input.cursor / scale + Vec2::splat(16.0);
                style(world, Some(id), |style| {
                    fill(style, theme.panel, RADIUS + 1.0);
                    style.set_border_color(Some(theme.border));
                    style.set_display(if hint.is_some() { Display::Flex } else { Display::None });
                    style.set_position_type(PositionType::Absolute(Rect::new_top_left(px(at.y), px(at.x))));
                });
                if let Some(Tooltip(hint)) = hint { label_of(world, child(world, id, &[0]), |label| label.text = hint); }
            }
        }
    }
}

/// Styles every themed widget for its state and the theme, then sets every `Label`'s text.
/// Runs in the `End` pin, before entity nodes are laid out.
pub(crate) fn restyle_themed(world: World, theme: Res<UiTheme>, input: Res<UiInput>, focus: Res<UiFocus>, mut widgets: Query<&'static Themed>) {
    let mut themed = vec![];
    while let Ok(Some((id, widget))) = widgets.next() { themed.push((id, widget.clone())); }
    drop(widgets);
    let restyle = Restyle { world: &world, theme: &theme, input: &input, focus: &focus };
    for (id, widget) in &themed { restyle.widget(*id, widget); }

    let mut labels = Query::<(&Label, &mut Node)>::new(&world);
    while let Ok(Some((id, (label, mut node)))) = labels.next() {
        let local = local_theme(&world, id, &theme);
        let color = if has::<Dim>(&world, id) { local.theme.dim } else { label.color.unwrap_or(local.theme.text) };
        node.style_mut().set_text(local.font(label.bold).cloned().map(|font| Text {
            font,
            content: label.text.clone(),
            color,
            font_size: label.size.px(),
            horizontal_align: label.align,
            vertical_align: Align::Center,
        }));
    }
}
