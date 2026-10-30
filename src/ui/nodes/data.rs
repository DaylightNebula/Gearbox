use std::{ops::Range, sync::Arc};

use anarchy::World;
use derive_more::{Deref, DerefMut};
use crate::TextureHandle;
use getset::{CopyGetters, Getters, Setters};
use magician_vgpu::glam::{Vec2, Vec4};
use mutual::{CowData, RefCowData, SharedData};

use crate::ui::{
    ClipRect, Outline, SDFFont, Scrolled, UINodeEndHoverEvent, UINodePressedEvent, UINodeReleasedEvent, UINodeStartHoverEvent,
};

/// Called with a node's new interaction whenever it changes.
pub type UINodeListener = Arc<dyn Fn(&World, Interaction) + Send + Sync>;

#[derive(Clone, Deref, DerefMut)]
pub struct UINode {
    id: Option<String>,
    #[deref]
    #[deref_mut]
    style: Style,
    last_state: CowData<LastState>,
    children: Vec<UINode>,
    listener: CowData<UINodeListener>,
}

impl Default for UINode {
    fn default() -> Self {
        Self {
            id: None,
            style: Style::default(),
            last_state: CowData::null(),
            children: Vec::new(),
            listener: CowData::null(),
        }
    }
}

impl std::fmt::Debug for UINode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UINode")
            .field("id", &self.id)
            .field("style", &self.style)
            .field("children", &self.children)
            .finish_non_exhaustive()
    }
}

impl UINode {
    /// Creates a new `UINode` with the given ID.  If you need to make
    /// a node without an ID, use the `default` function.
    pub fn new(id: String) -> Self {
        Self {
            id: Some(id),
            ..Default::default()
        }
    }

    /// Returns the ID of this node.
    pub fn id(&self) -> Option<&String> {
        self.id.as_ref()
    }

    /// Sets the ID of this node.  Returns true if this node had an ID previously.
    pub fn set_id(&mut self, id: String) -> bool {
        let had_id = self.id.is_some();
        self.id = Some(id);
        had_id
    }

    /// Returns the current interaction state of this node.
    pub fn last_state(&self) -> Option<RefCowData<LastState>> {
        if self.last_state.is_null() {
            return None;
        }
        Some(self.last_state.lock_ref())
    }

    /// Get the current listener assigned to this node.
    pub fn listener(&self) -> Option<RefCowData<UINodeListener>> {
        if self.listener.is_null() {
            return None;
        }
        Some(self.listener.get_ref())
    }

    /// Set the listener assigned to this node, called with its new interaction whenever it
    /// changes. Only nodes with an ID are listened to.
    pub fn set_listener(&self, listener: impl Fn(&World, Interaction) + Send + Sync + 'static) {
        self.listener.set(Arc::new(listener));
    }

    /// Set the listener assigned to this node from a shared listener.
    pub fn set_listener_arc(&self, listener: UINodeListener) {
        self.listener.set(listener);
    }

    /// Sets the interaction state of this node.
    pub(crate) fn set_last_state(&self, world: &World, last_state: LastState) {
        // make sure this node has a previous state and ID
        if !self.last_state.is_null() && let Some(id) = &self.id {
            let prev_interaction = self.last_state.get_ref().interaction;
            let new_interaction = last_state.interaction;

            if prev_interaction != new_interaction {
                if !self.listener.is_null() {
                    (self.listener.get_ref())(world, new_interaction);
                }

                let id = id.clone();
                match (prev_interaction, new_interaction) {
                    (Interaction::Hovered, Interaction::None) => world.send_event(UINodeEndHoverEvent { id }),
                    (_, Interaction::None) | (Interaction::Pressed, Interaction::Hovered) => world.send_event(UINodeReleasedEvent { id }),
                    (_, Interaction::Hovered) => world.send_event(UINodeStartHoverEvent { id }),
                    (_, Interaction::Pressed) => world.send_event(UINodePressedEvent { id }),
                }
            }
        }

        self.last_state.set(last_state);
    }

    /// Returns a reference to the style of this node.
    pub fn style(&self) -> &Style {
        &self.style
    }

    /// Returns a mutable reference to the style of this node.
    pub fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    /// Sets the style of this node.
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Adds a child node to this node.
    pub fn add(&mut self, node: UINode) {
        self.children.push(node);
    }

    /// Adds all of nodes of an iterator as children to this node.
    pub fn add_all(&mut self, iter: impl Iterator<Item = UINode>) {
        self.children.extend(iter);
    }

    /// Attempts to remove a node of the given id from this nodes children.
    /// Returns an option to the removed node if it was found.
    pub fn remove(&mut self, id: &str) {
        self.children
            .retain(|node| node.id().map(|a| a != id).unwrap_or(false))
    }

    /// Retains only the nodes that the given callback returns true for.
    pub fn retain(&mut self, callback: fn(&UINode) -> bool) {
        self.children.retain(callback);
    }

    /// Drains all children from this node.
    /// Returns an iterator to all drained children nodes.
    pub fn drain(&mut self) -> impl Iterator<Item = UINode> {
        self.children.drain(..)
    }

    /// Get all children
    pub fn children(&self) -> &[UINode] {
        &self.children
    }
}

/// How a node is sized, placed and drawn, following CSS flexbox. Unlike CSS, `flex_direction`
/// defaults to `Column`, and `Val::Auto` margins, padding and borders are zero.
#[derive(Setters, Getters, Debug, Clone)]
#[getset(get = "pub", set = "pub")]
pub struct Style {
    display: Display,
    position_type: PositionType,
    /// Takes the node out of the flow and pins it to part of its parent, or of the window for
    /// a root. Its margins keep it in from the edges it's pinned to.
    anchor: Option<Anchor>,
    background: Background,
    /// What happens to content wider than the node.
    overflow_x: Overflow,
    /// What happens to content taller than the node.
    overflow_y: Overflow,
    /// How far scrolled content is moved up and left, for `Overflow::Scroll` and `Overflow::Auto`
    /// axes. Laying out keeps it within what the content allows.
    scroll: Vec2,
    /// How the scrollbars look.
    scrollbar: Scrollbar,
    text: Option<Text>,
    /// Width over height, used when only one of them is set. 0 matches the window's.
    aspect_ratio: Option<f32>,
    width: Val,
    height: Val,
    min_width: Val,
    min_height: Val,
    max_width: Val,
    max_height: Val,
    margin: Rect,
    padding: Rect,
    /// Width of the border, inside the node's size like padding.
    border: Val,
    border_radius: RectCorners,
    border_color: Option<Vec4>,
    flex_direction: FlexDirection,
    flex_wrap: FlexWrap,
    /// Places children along the main axis.
    justify_content: JustifyContent,
    /// Places children across the main axis.
    align_items: AlignItems,
    /// Overrides the parent's `align_items` for this node.
    align_self: Option<AlignItems>,
    /// Places wrapped lines across the main axis.
    align_content: AlignContent,
    /// Room between rows, and between wrapped lines of a row.
    row_gap: Val,
    /// Room between columns, and between wrapped lines of a column.
    column_gap: Val,
    /// Share of the parent's leftover room along its main axis this node grows into.
    flex_grow: f32,
    /// Share of the parent's missing room this node gives up, never below its content.
    flex_shrink: f32,
    /// Size along the parent's main axis before growing or shrinking, `Auto` uses
    /// `width` or `height`.
    flex_basis: Val,
    /// Columns of a `Display::Grid`, `None` for a square-ish grid.
    grid_columns: Option<u16>,
    /// Drawn over siblings, and for entity roots over other roots, with a lower `z_index`.
    z_index: i32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::default(),
            position_type: PositionType::default(),
            anchor: None,
            background: Background::default(),
            overflow_x: Overflow::default(),
            overflow_y: Overflow::default(),
            scroll: Vec2::ZERO,
            scrollbar: Scrollbar::default(),
            text: None,
            aspect_ratio: None,
            width: Val::Auto,
            height: Val::Auto,
            min_width: Val::Auto,
            min_height: Val::Auto,
            max_width: Val::Auto,
            max_height: Val::Auto,
            margin: Rect::default(),
            padding: Rect::default(),
            border: Val::Auto,
            border_radius: RectCorners::default(),
            border_color: None,
            flex_direction: FlexDirection::default(),
            flex_wrap: FlexWrap::default(),
            justify_content: JustifyContent::default(),
            align_items: AlignItems::default(),
            align_self: None,
            align_content: AlignContent::default(),
            row_gap: Val::Auto,
            column_gap: Val::Auto,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Val::Auto,
            grid_columns: None,
            z_index: 0,
        }
    }
}

impl Style {
    /// Lays children out left to right, `justify` placing them along the row and `align`
    /// across it.
    pub fn set_row(&mut self, justify: JustifyContent, align: AlignItems) -> &mut Self {
        self.flex_direction = FlexDirection::Row;
        self.justify_content = justify;
        self.align_items = align;
        self
    }

    /// Lays children out top to bottom, `justify` placing them along the column and `align`
    /// across it.
    pub fn set_column(&mut self, justify: JustifyContent, align: AlignItems) -> &mut Self {
        self.flex_direction = FlexDirection::Column;
        self.justify_content = justify;
        self.align_items = align;
        self
    }

    /// Sets both `row_gap` and `column_gap`.
    pub fn set_gap(&mut self, gap: Val) -> &mut Self {
        self.row_gap = gap;
        self.column_gap = gap;
        self
    }

    /// Sets `flex_grow`, `flex_shrink` and `flex_basis`, like CSS's `flex` shorthand.
    pub fn set_flex(&mut self, grow: f32, shrink: f32, basis: Val) -> &mut Self {
        self.flex_grow = grow;
        self.flex_shrink = shrink;
        self.flex_basis = basis;
        self
    }

    /// Sets `overflow_x` and `overflow_y`.
    pub fn set_overflow(&mut self, overflow: Overflow) -> &mut Self {
        self.overflow_x = overflow;
        self.overflow_y = overflow;
        self
    }

    /// Whether content can scroll along either axis.
    pub fn scrolls(&self) -> bool { self.overflow_x.scrolls() || self.overflow_y.scrolls() }

    /// The text to change in place, if the node has any.
    pub fn text_mut(&mut self) -> Option<&mut Text> { self.text.as_mut() }

    /// Pins the node to `anchor` of its parent, see `Anchor`.
    pub fn set_anchored(&mut self, anchor: Anchor) -> &mut Self {
        self.anchor = Some(anchor);
        self
    }

    /// This style with every `Val::Px` and font size multiplied by `factor`.
    pub fn scaled(&self, factor: f32) -> Style {
        let mut style = self.clone();
        for val in [
            &mut style.width, &mut style.height, &mut style.min_width, &mut style.min_height,
            &mut style.max_width, &mut style.max_height, &mut style.border, &mut style.row_gap,
            &mut style.column_gap, &mut style.flex_basis,
        ] {
            *val = val.scaled(factor);
        }
        style.margin = style.margin.scaled(factor);
        style.padding = style.padding.scaled(factor);
        style.border_radius = style.border_radius.scaled(factor);
        if let PositionType::Absolute(edges) | PositionType::Fixed(edges) = &mut style.position_type { *edges = edges.scaled(factor); }
        style.scroll *= factor;
        style.scrollbar.width *= factor;
        style.scrollbar.gap *= factor;
        if let Some(text) = &mut style.text {
            text.font_size *= factor;
            text.scroll *= factor;
        }
        style
    }
}

/// Where in the flow a node goes.
#[derive(Default, Debug, Clone, Copy)]
pub enum PositionType {
    /// Laid out by its parent's flexbox or grid.
    #[default]
    Relative,
    /// Taken out of the flow and placed by its offsets from the parent's edges, `Val::Auto`
    /// edges left unset. Setting both opposite edges stretches the node between them.
    Absolute(Rect),
    /// Placed by its offsets from the window's edges like `Absolute`, and drawn over the other
    /// roots like one, so its parents don't cut it off: for menus and popups that belong to a
    /// widget. Like CSS `position: fixed`.
    Fixed(Rect),
}

/// Part of its parent a node is pinned to, taking it out of the flow. Roots are pinned to the
/// window. The node's margins keep it in from the edges it touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    /// Stretched across the top edge, as tall as it is.
    FillTop,
    /// Stretched across the bottom edge, as tall as it is.
    FillBottom,
    /// Stretched down the left edge, as wide as it is.
    FillLeft,
    /// Stretched down the right edge, as wide as it is.
    FillRight,
    /// Covering the whole parent, or the whole window for a root.
    Fill,
}

/// What happens to a node's content where it doesn't fit, along one axis.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Overflow {
    /// Cut off at the node's edges.
    #[default]
    Clip,
    /// Cut off inside the border and scrolled, with room always kept for a scrollbar, like CSS
    /// `overflow: scroll`. The node can shrink below its content.
    Scroll,
    /// Like `Scroll`, but the scrollbar and its room only appear while the content doesn't fit,
    /// like CSS `overflow: auto`.
    Auto,
}

impl Overflow {
    /// Whether content can scroll along this axis.
    pub fn scrolls(self) -> bool { matches!(self, Overflow::Scroll | Overflow::Auto) }
}

/// How a node's scrollbars look. Each takes `width + gap` from the node's padding box along its
/// edge: right for the vertical bar, bottom for the horizontal one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scrollbar {
    /// Thickness of the track and thumb.
    pub width: f32,
    /// Room between the content and the track.
    pub gap: f32,
    pub track: Vec4,
    pub thumb: Vec4,
    /// The thumb while the pointer is over the node or dragging it.
    pub thumb_hovered: Vec4,
}

impl Scrollbar {
    /// Room a bar takes across its edge.
    pub fn room(&self) -> f32 { self.width + self.gap }
}

impl Default for Scrollbar {
    fn default() -> Self {
        Self {
            width: 8.0,
            gap: 0.0,
            track: Vec4::new(0.0, 0.0, 0.0, 0.25),
            thumb: Vec4::new(0.3, 0.3, 0.3, 1.0),
            thumb_hovered: Vec4::new(0.5, 0.5, 0.5, 1.0),
        }
    }
}

/// How a node lays out its children.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Display {
    /// A flexbox, see `Style::flex_direction`.
    #[default]
    Flex,
    /// A grid of equal cells, `Style::grid_columns` wide, filled row by row.
    Grid,
    /// Hidden, taking no room.
    None,
}

/// The main axis children of a flexbox are laid out along.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    Row,
    #[default]
    Column,
    RowReverse,
    ColumnReverse,
}

/// Whether children of a flexbox that don't fit wrap onto more lines.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

/// Placement of children along the main axis.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JustifyContent {
    #[default]
    Start,
    End,
    Center,
    /// Room only between children.
    SpaceBetween,
    /// Equal room around each child.
    SpaceAround,
    /// Equal room between children and at both ends.
    SpaceEvenly,
}

/// Placement of children across the main axis.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignItems {
    Start,
    End,
    Center,
    /// Children without a cross size fill the line.
    #[default]
    Stretch,
    Baseline,
}

/// Placement of wrapped lines across the main axis.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignContent {
    Start,
    End,
    Center,
    #[default]
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Text alignment within its node.
#[derive(Default, Debug, Clone, Copy, Hash)]
pub enum Align {
    #[default]
    Start,
    End,
    Center,
}

#[derive(Default, Debug, Clone)]
pub enum Background {
    #[default]
    Empty,
    Color(Vec4),
    Image(TextureHandle),
    /// `Outline` filled in the color, stretched over the content box, inside the padding.
    Shape(Outline, Vec4),
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Val {
    /// Sized by content and the layout, zero for margins, padding, borders and gaps.
    #[default]
    Auto,
    Px(f32),
    /// A fraction of the parent's size along the same axis, 0.5 being half.
    Percent(f32),
    /// A fraction of the window's width.
    PercentWidth(f32),
    /// A fraction of the window's height.
    PercentHeight(f32),
}

impl Val {
    /// This value with `Px` multiplied by `factor`.
    pub fn scaled(self, factor: f32) -> Val {
        match self {
            Val::Px(px) => Val::Px(px * factor),
            other => other,
        }
    }

    /// Pixels this resolves to in a window `display_size` big, `None` for `Auto` and `Percent`,
    /// which depend on the layout.
    pub fn eval(&self, display_size: &[f32; 2]) -> Option<f32> {
        match self {
            Val::Auto | Val::Percent(_) => None,
            Val::Px(px) => Some(*px),
            Val::PercentWidth(percent) => Some(*percent * display_size[0] as f32),
            Val::PercentHeight(percent) => Some(*percent * display_size[1] as f32),
        }
    }
}

#[derive(Setters, Getters, Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
#[getset(get = "pub", set = "pub")]
pub struct Rect {
    top: Val,
    bottom: Val,
    left: Val,
    right: Val,
}

impl Rect {
    pub fn new(top: Val, bottom: Val, left: Val, right: Val) -> Self {
        Self {
            top,
            bottom,
            left,
            right,
        }
    }

    pub fn new_top(top: Val) -> Self {
        Self {
            top,
            ..Default::default()
        }
    }

    pub fn new_bottom(bottom: Val) -> Self {
        Self {
            bottom,
            ..Default::default()
        }
    }

    pub fn new_left(left: Val) -> Self {
        Self {
            left,
            ..Default::default()
        }
    }

    pub fn new_right(right: Val) -> Self {
        Self {
            right,
            ..Default::default()
        }
    }

    pub fn new_top_left(top: Val, left: Val) -> Self {
        Self {
            top,
            left,
            ..Default::default()
        }
    }

    pub fn new_top_right(top: Val, right: Val) -> Self {
        Self {
            top,
            right,
            ..Default::default()
        }
    }

    pub fn new_bottom_left(bottom: Val, left: Val) -> Self {
        Self {
            bottom,
            left,
            ..Default::default()
        }
    }

    pub fn new_bottom_right(bottom: Val, right: Val) -> Self {
        Self {
            bottom,
            right,
            ..Default::default()
        }
    }

    pub fn single(val: Val) -> Self {
        Self::new(val, val, val, val)
    }

    /// These edges with `Px` multiplied by `factor`.
    pub fn scaled(self, factor: f32) -> Self {
        Self::new(self.top.scaled(factor), self.bottom.scaled(factor), self.left.scaled(factor), self.right.scaled(factor))
    }
}

#[derive(Setters, Getters, Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
#[getset(get = "pub", set = "pub")]
pub struct RectCorners {
    top_left: Val,
    top_right: Val,
    bottom_left: Val,
    bottom_right: Val,
}

impl RectCorners {
    pub fn new(top_left: Val, top_right: Val, bottom_left: Val, bottom_right: Val) -> Self {
        Self {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        }
    }

    pub fn single(val: Val) -> Self {
        Self::new(val, val, val, val)
    }

    /// These corners with `Px` multiplied by `factor`.
    pub fn scaled(self, factor: f32) -> Self {
        Self::new(self.top_left.scaled(factor), self.top_right.scaled(factor), self.bottom_left.scaled(factor), self.bottom_right.scaled(factor))
    }
}

#[derive(CopyGetters, Default, Debug, Clone, Copy, PartialEq)]
#[getset(get_copy = "pub")]
pub struct LastState {
    pub position: Vec2,
    pub size: Vec2,
    pub interaction: Interaction,
    /// The part of the window it shows in, where its ancestors don't cut it off.
    pub clip: ClipRect,
    /// Where its content scrolled, if it scrolls.
    pub scroll: Option<Scrolled>,
}

/// How the pointer is interacting with a node. As a component it makes an entity node
/// pickable, see `pick_ui`.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, anarchy::Component)]
pub enum Interaction {
    #[default]
    None,
    Hovered,
    Pressed,
}

/// Text a node draws in its content box. Newlines start new lines. Build one with `Text::new`.
#[derive(Clone, Getters, Setters, Debug)]
#[getset(get = "pub", set = "pub")]
pub struct Text {
    pub font: Arc<SDFFont>,
    pub content: String,
    pub color: Vec4,
    pub font_size: f32,
    /// Places each line across the content box.
    pub horizontal_align: Align,
    /// Places the block of lines down the content box.
    pub vertical_align: Align,
    /// Runs drawn differently, later spans winning where they overlap.
    pub spans: Vec<TextSpan>,
    /// Wraps words onto new lines to fit the content box's width.
    pub wrap: bool,
    /// Hides what falls outside the content box, rather than the node's whole box.
    pub clip: bool,
    /// Moves the text up and left by this much, for scrolling a clipped box.
    pub scroll: Vec2,
    /// A caret and selection to draw, for text being edited.
    pub cursor: Option<TextCursor>,
}

impl Text {
    /// `content` in `font` at `font_size` logical pixels, aligned to the top left.
    pub fn new(font: Arc<SDFFont>, content: impl Into<String>, color: Vec4, font_size: f32) -> Self {
        Self {
            font,
            content: content.into(),
            color,
            font_size,
            horizontal_align: Align::Start,
            vertical_align: Align::Start,
            spans: vec![],
            wrap: false,
            clip: false,
            scroll: Vec2::ZERO,
            cursor: None,
        }
    }

    pub fn with_align(mut self, horizontal: Align, vertical: Align) -> Self {
        self.horizontal_align = horizontal;
        self.vertical_align = vertical;
        self
    }

    pub fn with_spans(mut self, spans: impl IntoIterator<Item = TextSpan>) -> Self { self.spans = spans.into_iter().collect(); self }

    /// Adds one span.
    pub fn with_span(mut self, span: TextSpan) -> Self { self.spans.push(span); self }

    pub fn with_wrap(mut self) -> Self { self.wrap = true; self }

    pub fn with_clip(mut self) -> Self { self.clip = true; self }
}

/// A run of a `Text`'s bytes drawn differently from the rest, like a highlighted keyword or an
/// error. Ranges past the end of the text are cut off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextSpan {
    pub range: Range<usize>,
    /// Overrides the text's color.
    pub color: Option<Vec4>,
    /// Fills the box behind the run.
    pub background: Option<Vec4>,
    pub underline: Option<Underline>,
    /// Shown by themed tooltips while the pointer is over the run, which also sends
    /// `TextHover` events.
    pub hover: Option<String>,
}

impl TextSpan {
    pub fn new(range: Range<usize>) -> Self { Self { range, ..Default::default() } }

    pub fn color(mut self, color: Vec4) -> Self { self.color = Some(color); self }

    pub fn background(mut self, color: Vec4) -> Self { self.background = Some(color); self }

    pub fn underline(mut self, color: Vec4) -> Self { self.underline = Some(Underline { color, style: UnderlineStyle::Solid }); self }

    /// A squiggly underline, like a code editor's error marker.
    pub fn wavy(mut self, color: Vec4) -> Self { self.underline = Some(Underline { color, style: UnderlineStyle::Wavy }); self }

    pub fn hover(mut self, hover: impl Into<String>) -> Self { self.hover = Some(hover.into()); self }
}

/// A line under a `TextSpan`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underline {
    pub color: Vec4,
    pub style: UnderlineStyle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnderlineStyle {
    #[default]
    Solid,
    Wavy,
}

/// The caret and selection of text being edited, as byte offsets.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCursor {
    pub caret: usize,
    pub selection: Option<Range<usize>>,
    /// Draws the caret, off between blinks.
    pub show_caret: bool,
    pub caret_color: Vec4,
    pub selection_color: Vec4,
}
