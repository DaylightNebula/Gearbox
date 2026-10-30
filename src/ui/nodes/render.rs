//! Lays `UINode` trees out with taffy, a CSS flexbox and grid implementation, and turns them
//! into positioned `SDFElement`s.

use std::sync::Arc;

use anarchy::World;
use magician_vgpu::glam::*;
use taffy::{
    AvailableSpace, LengthPercentage, LengthPercentageAuto, NodeId, TaffyTree,
    style_helpers::{auto, evenly_sized_tracks, length, percent, zero},
};

use crate::ui::{
    AlignContent, AlignItems, Anchor, Background, Display, FlexDirection, FlexWrap, Interaction,
    JustifyContent, LastState, Outline, Overflow, PositionType, Rect, SDFElement, SDFFont, SDFRectangle, SDFShape,
    SDFStyle, Scrollbar, Style, Text, TextLayout, UIInputTracker, UINode, UnderlineStyle, Val,
    nodes::Align,
};

const TRANSPARENT: Vec4 = Vec4::ZERO;

/// Lays `nodes` out as the roots of a window `display_size` (width, height) pixels big, stacked
/// as a column filling it unless they're anchored or absolute, and returns what to draw.
/// Updates each node's `last_state`, calling its listener and sending events as its interaction
/// changes.
pub fn layout_ui_nodes(
    world: &World,
    input_tracker: &UIInputTracker,
    nodes: &[UINode],
    display_size: [f32; 2],
) -> Vec<SDFElement> {
    layout_children(world, input_tracker, nodes, Vec2::ZERO, Vec2::from_array(display_size), display_size)
}

/// Lays `nodes` out as a column in the `available` pixels at `origin`, in a window
/// `display_size` big, without an element for their parent. Useful for laying out a sub-tree
/// on its own.
pub fn layout_children(
    world: &World,
    input_tracker: &UIInputTracker,
    nodes: &[UINode],
    origin: Vec2,
    available: Vec2,
    display_size: [f32; 2],
) -> Vec<SDFElement> {
    let cursor = input_tracker.cursor_position;
    layout_tree(nodes, origin, available, display_size, Some(cursor), &mut |node: &UINode, laid_out| {
        let (min, max) = (laid_out.top_left, laid_out.top_left + laid_out.size);
        let is_hover = ClipRect::new(min, max).contains(cursor) && laid_out.clip.contains(cursor);
        let interaction = match (is_hover, input_tracker.is_cursor_down) {
            (true, true) => Interaction::Pressed,
            (true, false) => Interaction::Hovered,
            (false, _) => Interaction::None,
        };
        let state = LastState { position: min + laid_out.size * 0.5, size: laid_out.size, interaction, clip: laid_out.clip, scroll: laid_out.scroll };
        node.set_last_state(world, state);
    })
}

/// A tree of nodes to lay out.
pub(crate) trait LayoutTree: Sized {
    fn style(&self) -> &Style;
    fn children(&self) -> &[Self];
}

impl LayoutTree for UINode {
    fn style(&self) -> &Style { UINode::style(self) }
    fn children(&self) -> &[Self] { UINode::children(self) }
}

/// Where a node was laid out.
#[derive(Clone, Debug)]
pub(crate) struct LaidOut {
    pub top_left: Vec2,
    pub size: Vec2,
    /// Paint order, later nodes being drawn over earlier ones.
    pub stack: u32,
    /// Where its text went.
    pub text: Option<TextPlacement>,
    /// The part of the window it shows in, where its ancestors don't cut it off.
    pub clip: ClipRect,
    /// Where its content scrolled, if it scrolls.
    pub scroll: Option<Scrolled>,
}

/// An axis aligned box in window pixels, empty when `min` passes `max` on either axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipRect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Default for ClipRect {
    fn default() -> Self { Self::EVERYWHERE }
}

impl ClipRect {
    /// Holds every point.
    pub const EVERYWHERE: Self = Self { min: Vec2::splat(f32::MIN), max: Vec2::splat(f32::MAX) };

    pub fn new(min: Vec2, max: Vec2) -> Self { Self { min, max } }

    pub fn contains(&self, point: Vec2) -> bool { point.cmpge(self.min).all() && point.cmple(self.max).all() }

    /// The part inside both.
    pub fn intersect(&self, other: ClipRect) -> ClipRect { ClipRect::new(self.min.max(other.min), self.max.min(other.max)) }

    /// Whether some of the box `min..max` is inside.
    pub fn overlaps(&self, min: Vec2, max: Vec2) -> bool { min.cmple(self.max).all() && max.cmpge(self.min).all() }

    pub fn size(&self) -> Vec2 { (self.max - self.min).max(Vec2::ZERO) }
}

/// Where a scrolling node's content went, in window pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scrolled {
    /// The box the content shows in: inside the border, less the scrollbars.
    pub view: ClipRect,
    /// How far the content is scrolled up and left, within `max`.
    pub offset: Vec2,
    /// Farthest the content can scroll, 0 along axes that don't scroll or fit.
    pub max: Vec2,
    /// The horizontal (index 0) and vertical (index 1) scrollbars, where shown.
    pub bars: [Option<ScrollbarRects>; 2],
}

/// Where a scrollbar was drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarRects {
    pub track: ClipRect,
    /// Missing while there's nothing to scroll.
    pub thumb: Option<ClipRect>,
}

impl Scrolled {
    /// The scroll offset along `axis` (0 for x, 1 for y) that puts the start of the thumb at
    /// `thumb_start`, for dragging it.
    pub fn offset_at(&self, axis: usize, thumb_start: f32) -> f32 {
        let Some(ScrollbarRects { track, thumb: Some(thumb) }) = self.bars[axis] else { return self.offset[axis] };
        let free = track.size()[axis] - thumb.size()[axis];
        if free <= 0.0 { return 0.0 }
        ((thumb_start - track.min[axis]) / free).clamp(0.0, 1.0) * self.max[axis]
    }

    /// These bars cut down to `clip`, where they can be grabbed.
    pub fn clipped(mut self, clip: ClipRect) -> Self {
        for bar in self.bars.iter_mut().flatten() {
            bar.track = bar.track.intersect(clip);
            if let Some(thumb) = &mut bar.thumb { *thumb = thumb.intersect(clip); }
        }
        self
    }

    /// The scrollbar along `axis` that holds `point`.
    pub fn bar_at(&self, point: Vec2) -> Option<(usize, ScrollbarRects)> {
        (0..2).find_map(|axis| self.bars[axis].filter(|bar| bar.track.contains(point)).map(|bar| (axis, bar)))
    }
}

/// The thumb in `track` along `axis` for content scrolled `offset` of `max`, shown in `view`
/// pixels, at least `min_length` long.
fn thumb_rect(track: ClipRect, axis: usize, view: f32, max: f32, offset: f32, min_length: f32) -> Option<ClipRect> {
    if max <= 0.0 { return None }
    let length = track.size()[axis];
    let thumb = (length * view / (view + max)).max(min_length).min(length);
    let start = track.min[axis] + (length - thumb) * (offset / max).clamp(0.0, 1.0);
    let mut rect = track;
    rect.min[axis] = start;
    rect.max[axis] = start + thumb;
    Some(rect)
}

/// Lays `nodes` out as a column in the `available` pixels at `origin` like `layout_children`,
/// calling `on_node` with where each shown node went.
pub(crate) fn layout_tree<T: LayoutTree>(
    nodes: &[T],
    origin: Vec2,
    available: Vec2,
    display_size: [f32; 2],
    cursor: Option<Vec2>,
    on_node: &mut dyn FnMut(&T, LaidOut),
) -> Vec<SDFElement> {
    // `Fixed` nodes are laid out and drawn as roots
    let mut lifted = vec![];
    let mut roots: Vec<Lifted<T>> = nodes.iter().map(|node| lift(node, &mut lifted, false)).collect();
    roots.extend(lifted);

    let mut layout = Layout { tree: TaffyTree::new(), display_size };
    // fractional positions, as the SDFs are antialiased anyway
    layout.tree.disable_rounding();
    let built = roots.iter().map(|node| layout.build(node)).collect::<Vec<_>>();
    let parent = taffy::Style {
        flex_direction: taffy::FlexDirection::Column,
        size: taffy::Size { width: length(available.x), height: length(available.y) },
        ..Default::default()
    };
    let Ok(viewport) = layout.tree.new_with_children(parent, &built.iter().map(Built::outer).collect::<Vec<_>>()) else { return vec![] };
    let space = taffy::Size { width: AvailableSpace::Definite(available.x), height: AvailableSpace::Definite(available.y) };
    // `Overflow::Auto` nodes get scrollbars where their content overflows, which can make more
    // content overflow, so lay out again until no more bars appear
    for _ in 0..3 {
        let computed = layout.tree.compute_layout_with_measure(viewport, space, |inputs, _, context, style| {
            taffy::compute_leaf_layout(inputs, style, |_, _| 0.0, |known, available| {
                let measured = context.map_or(Vec2::ZERO, |text| {
                    // a known width is the whole node's, available space is already inside it
                    let wrap = text.wrap.then(|| known.width.map(|width| width - text.inset).or(available.width.into_option())).flatten();
                    text.font.layout(&text.content, text.font_size, wrap.map(|width| width.max(0.0))).size()
                });
                taffy::Size { width: known.width.unwrap_or(measured.x), height: known.height.unwrap_or(measured.y) }
            })
        });
        if let Err(err) = computed {
            log::error!("Failed to lay out the UI: {err}");
            return vec![];
        }
        let mut added = false;
        for (node, built) in roots.iter().zip(&built) { added |= layout.add_auto_scrollbars(node, built); }
        if !added { break }
    }
    let mut emit = Emit { tree: &layout.tree, display_size, cursor, on_node, stack: 0 };
    let window = ClipRect::new(Vec2::ZERO, Vec2::from_array(display_size));
    paint_order(&roots, &built).into_iter().filter_map(|(node, built)| emit.element(node, built, origin, window)).collect()
}

/// A node to lay out, with its `Fixed` descendants moved to the roots.
struct Lifted<'a, T> {
    node: &'a T,
    children: Vec<Lifted<'a, T>>,
}

impl<T: LayoutTree> Lifted<'_, T> {
    fn style(&self) -> &Style { self.node.style() }
}

/// `node` with its `Fixed` descendants moved into `lifted`, or dropped if `hidden` (inside a
/// hidden node).
fn lift<'a, T: LayoutTree>(node: &'a T, lifted: &mut Vec<Lifted<'a, T>>, hidden: bool) -> Lifted<'a, T> {
    let hidden = hidden || *node.style().display() == Display::None;
    let mut children = vec![];
    for child in node.children() {
        // a lifted node goes before the ones lifted out of it, so it's drawn under them
        let mut inner = vec![];
        let child_lifted = lift(child, &mut inner, hidden);
        match child.style().position_type() {
            PositionType::Fixed(_) => if !hidden { lifted.push(child_lifted) },
            _ => children.push(child_lifted),
        }
        lifted.extend(inner);
    }
    Lifted { node, children }
}

/// Text a leaf is measured by.
struct TextMeasure {
    font: Arc<SDFFont>,
    content: String,
    font_size: f32,
    wrap: bool,
    /// Padding and border across the node, in pixels.
    inset: f32,
}

/// The taffy tree mirroring the nodes being laid out.
struct Layout {
    tree: TaffyTree<TextMeasure>,
    display_size: [f32; 2],
}

/// Where a `UINode` and its children went in the taffy tree.
struct Built {
    node: NodeId,
    /// Axes with `Overflow::Auto`, x then y.
    auto: [bool; 2],
    /// Fills the parent to place an anchored node in it.
    anchor: Option<NodeId>,
    children: Vec<Built>,
}

impl Built {
    /// The taffy node the parent lays out.
    fn outer(&self) -> NodeId { self.anchor.unwrap_or(self.node) }
}

impl Layout {
    /// Adds `node` and its children to the tree.
    fn build<T: LayoutTree>(&mut self, node: &Lifted<T>) -> Built {
        let children: Vec<Built> = node.children.iter().map(|child| self.build(child)).collect();
        let mut style = self.style(node.style(), &node.children);
        let anchor = *node.style().anchor();
        if let Some(anchor) = anchor { anchored(&mut style, anchor); }
        let id = if let Some(text) = node.style().text().as_ref().filter(|_| children.is_empty()) {
            let room = |val: Val| val.eval(&self.display_size).unwrap_or(0.0);
            let padding = node.style().padding();
            let inset = room(*padding.left()) + room(*padding.right()) + 2.0 * room(*node.style().border());
            let measure = TextMeasure { font: text.font.clone(), content: text.content.clone(), font_size: text.font_size, wrap: text.wrap, inset };
            self.tree.new_leaf_with_context(style, measure)
        } else {
            self.tree.new_with_children(style, &children.iter().map(Built::outer).collect::<Vec<_>>())
        };
        // taffy only fails on node ids it didn't hand out
        let id = id.expect("taffy rejected a new node");
        let anchor = anchor.map(|anchor| {
            self.tree.new_with_children(anchor_frame(anchor), &[id]).expect("taffy rejected a new node")
        });
        let auto = [*node.style().overflow_x() == Overflow::Auto, *node.style().overflow_y() == Overflow::Auto];
        Built { node: id, auto, children, anchor }
    }

    /// Gives `Overflow::Auto` axes of `node` and its children whose content overflows a
    /// scrollbar, returning whether any got one.
    fn add_auto_scrollbars<T: LayoutTree>(&mut self, node: &Lifted<T>, built: &Built) -> bool {
        let mut added = false;
        for (child, child_built) in node.children.iter().zip(&built.children) { added |= self.add_auto_scrollbars(child, child_built); }
        if !built.auto.contains(&true) { return added }
        let (Ok(layout), Ok(style)) = (self.tree.layout(built.node), self.tree.style(built.node)) else { return added };
        let overflowing = content_overflow(layout, style);
        let mut style = style.clone();
        let mut changed = false;
        for (auto, overflows, axis) in [(built.auto[0], overflowing.x, &mut style.overflow.x), (built.auto[1], overflowing.y, &mut style.overflow.y)] {
            if auto && overflows > 0.5 && *axis != taffy::Overflow::Scroll {
                *axis = taffy::Overflow::Scroll;
                changed = true;
            }
        }
        if changed && self.tree.set_style(built.node, style).is_ok() { added = true; }
        added
    }

    /// `style` in taffy's terms, `children` being the node's.
    fn style<T: LayoutTree>(&self, style: &Style, children: &[Lifted<T>]) -> taffy::Style {
        let display_size = self.display_size;
        let dimension = |val: Val| match val {
            Val::Auto => auto(),
            Val::Px(px) => length(px),
            Val::Percent(fraction) => percent(fraction),
            other => length(other.eval(&display_size).unwrap_or(0.0)),
        };
        let offset = |val: Val| -> LengthPercentageAuto {
            match val {
                Val::Auto => auto(),
                Val::Px(px) => length(px),
                Val::Percent(fraction) => percent(fraction),
                other => length(other.eval(&display_size).unwrap_or(0.0)),
            }
        };
        let room = |val: Val| -> LengthPercentage {
            match val {
                Val::Auto => zero(),
                Val::Px(px) => length(px),
                Val::Percent(fraction) => percent(fraction),
                other => length(other.eval(&display_size).unwrap_or(0.0)),
            }
        };
        let margin = |val: Val| -> LengthPercentageAuto { if val == Val::Auto { zero() } else { offset(val) } };
        let rect = |r: &Rect, f: &dyn Fn(Val) -> LengthPercentageAuto| taffy::Rect {
            left: f(*r.left()), right: f(*r.right()), top: f(*r.top()), bottom: f(*r.bottom()),
        };
        let (position, inset) = match style.position_type() {
            PositionType::Relative => (taffy::Position::Relative, taffy::Rect::auto()),
            PositionType::Absolute(edges) | PositionType::Fixed(edges) => (taffy::Position::Absolute, rect(edges, &offset)),
        };
        // `Auto` axes start without a scrollbar, see `add_auto_scrollbars`
        let overflow = |overflow: Overflow| match overflow {
            Overflow::Clip => taffy::Overflow::Visible,
            Overflow::Scroll => taffy::Overflow::Scroll,
            Overflow::Auto => taffy::Overflow::Hidden,
        };
        let border = room(*style.border());
        let padding = style.padding();

        let mut out = taffy::Style {
            display: match style.display() {
                Display::Flex => taffy::Display::Flex,
                Display::Grid => taffy::Display::Grid,
                Display::None => taffy::Display::None,
            },
            position,
            inset,
            size: taffy::Size { width: dimension(*style.width()), height: dimension(*style.height()) },
            min_size: taffy::Size { width: offset(*style.min_width()), height: offset(*style.min_height()) },
            max_size: taffy::Size { width: offset(*style.max_width()), height: offset(*style.max_height()) },
            aspect_ratio: style.aspect_ratio().map(|ratio| if ratio == 0.0 { display_size[0] / display_size[1].max(0.001) } else { ratio }),
            margin: rect(style.margin(), &margin),
            padding: taffy::Rect { left: room(*padding.left()), right: room(*padding.right()), top: room(*padding.top()), bottom: room(*padding.bottom()) },
            border: taffy::Rect { left: border, right: border, top: border, bottom: border },
            flex_direction: match style.flex_direction() {
                FlexDirection::Row => taffy::FlexDirection::Row,
                FlexDirection::Column => taffy::FlexDirection::Column,
                FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
                FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
            },
            flex_wrap: match style.flex_wrap() {
                FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
                FlexWrap::Wrap => taffy::FlexWrap::Wrap,
                FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
            },
            justify_content: Some(justify(*style.justify_content())),
            align_items: Some(align(*style.align_items())),
            align_self: style.align_self().map(align),
            align_content: Some(align_content(*style.align_content())),
            gap: taffy::Size { width: room(*style.column_gap()), height: room(*style.row_gap()) },
            flex_grow: *style.flex_grow(),
            flex_shrink: *style.flex_shrink(),
            flex_basis: dimension(*style.flex_basis()),
            overflow: taffy::Point { x: overflow(*style.overflow_x()), y: overflow(*style.overflow_y()) },
            scrollbar_width: style.scrollbar().room(),
            ..Default::default()
        };
        if *style.display() == Display::Grid {
            let cells = children.iter().filter(|child| in_flow(child.style())).count().max(1);
            let columns = style.grid_columns().map_or_else(|| (cells as f32).sqrt().ceil() as usize, usize::from).max(1);
            out.grid_template_columns = evenly_sized_tracks(columns as u16);
            out.grid_template_rows = evenly_sized_tracks(cells.div_ceil(columns) as u16);
        }
        out
    }
}

/// Whether a node takes part in its parent's flexbox or grid.
fn in_flow(style: &Style) -> bool {
    style.anchor().is_none() && matches!(style.position_type(), PositionType::Relative)
}

/// The frame filling an anchored node's parent, placing the node at `anchor` of it.
fn anchor_frame(anchor: Anchor) -> taffy::Style {
    use taffy::{AlignContent as J, AlignItems as A};
    let (direction, justify, align) = match anchor {
        Anchor::TopLeft => (taffy::FlexDirection::Column, J::START, A::START),
        Anchor::Top => (taffy::FlexDirection::Column, J::START, A::CENTER),
        Anchor::TopRight => (taffy::FlexDirection::Column, J::START, A::END),
        Anchor::Left => (taffy::FlexDirection::Column, J::CENTER, A::START),
        Anchor::Center => (taffy::FlexDirection::Column, J::CENTER, A::CENTER),
        Anchor::Right => (taffy::FlexDirection::Column, J::CENTER, A::END),
        Anchor::BottomLeft => (taffy::FlexDirection::Column, J::END, A::START),
        Anchor::Bottom => (taffy::FlexDirection::Column, J::END, A::CENTER),
        Anchor::BottomRight => (taffy::FlexDirection::Column, J::END, A::END),
        Anchor::FillTop | Anchor::Fill => (taffy::FlexDirection::Column, J::START, A::STRETCH),
        Anchor::FillBottom => (taffy::FlexDirection::Column, J::END, A::STRETCH),
        Anchor::FillLeft => (taffy::FlexDirection::Row, J::START, A::STRETCH),
        Anchor::FillRight => (taffy::FlexDirection::Row, J::END, A::STRETCH),
    };
    taffy::Style {
        position: taffy::Position::Absolute,
        inset: taffy::Rect { left: zero(), right: zero(), top: zero(), bottom: zero() },
        flex_direction: direction,
        justify_content: Some(justify),
        align_items: Some(align),
        ..Default::default()
    }
}

/// Fits an anchored node's own style to its frame.
fn anchored(style: &mut taffy::Style, anchor: Anchor) {
    style.position = taffy::Position::Relative;
    style.inset = taffy::Rect::auto();
    // the frame stretches it across, growing stretches it along
    if anchor == Anchor::Fill {
        style.flex_grow = 1.0;
        style.flex_basis = auto();
    }
}

fn justify(justify: JustifyContent) -> taffy::JustifyContent {
    match justify {
        JustifyContent::Start => taffy::JustifyContent::START,
        JustifyContent::End => taffy::JustifyContent::END,
        JustifyContent::Center => taffy::JustifyContent::CENTER,
        JustifyContent::SpaceBetween => taffy::JustifyContent::SPACE_BETWEEN,
        JustifyContent::SpaceAround => taffy::JustifyContent::SPACE_AROUND,
        JustifyContent::SpaceEvenly => taffy::JustifyContent::SPACE_EVENLY,
    }
}

fn align(align: AlignItems) -> taffy::AlignItems {
    match align {
        AlignItems::Start => taffy::AlignItems::START,
        AlignItems::End => taffy::AlignItems::END,
        AlignItems::Center => taffy::AlignItems::CENTER,
        AlignItems::Stretch => taffy::AlignItems::STRETCH,
        AlignItems::Baseline => taffy::AlignItems::BASELINE,
    }
}

fn align_content(align: AlignContent) -> taffy::AlignContent {
    match align {
        AlignContent::Start => taffy::AlignContent::START,
        AlignContent::End => taffy::AlignContent::END,
        AlignContent::Center => taffy::AlignContent::CENTER,
        AlignContent::Stretch => taffy::AlignContent::STRETCH,
        AlignContent::SpaceBetween => taffy::AlignContent::SPACE_BETWEEN,
        AlignContent::SpaceAround => taffy::AlignContent::SPACE_AROUND,
        AlignContent::SpaceEvenly => taffy::AlignContent::SPACE_EVENLY,
    }
}

/// `nodes` paired with where they were built in paint order: by `z_index`, and among equal ones
/// nodes in the flow first, so anchored and absolute ones go on top.
fn paint_order<'a, 'n, T: LayoutTree>(nodes: &'a [Lifted<'n, T>], built: &'a [Built]) -> Vec<(&'a Lifted<'n, T>, &'a Built)> {
    let mut order: Vec<(&Lifted<T>, &Built)> = nodes.iter().zip(built).collect();
    // stable, so siblings keep their order otherwise
    order.sort_by_key(|(node, _)| (*node.style().z_index(), !in_flow(node.style())));
    order
}

/// Room taken by a node's scrollbars: across x by the vertical one, across y by the horizontal
/// one.
fn bar_room(style: &taffy::Style) -> Vec2 {
    let room = |overflow: taffy::Overflow| if overflow == taffy::Overflow::Scroll { style.scrollbar_width } else { 0.0 };
    Vec2::new(room(style.overflow.y), room(style.overflow.x))
}

/// How far the content of a node laid out as `layout` passes its padding box, less its
/// scrollbars, along each axis.
fn content_overflow(layout: &taffy::Layout, style: &taffy::Style) -> Vec2 {
    let border = Vec2::new(layout.border.left + layout.border.right, layout.border.top + layout.border.bottom);
    let view = Vec2::new(layout.size.width, layout.size.height) - border - bar_room(style);
    let content = Vec2::new(layout.scrollable_overflow_rect.right, layout.scrollable_overflow_rect.bottom);
    (content - view).max(Vec2::ZERO)
}

/// Turns laid out nodes into elements.
struct Emit<'a, T> {
    tree: &'a TaffyTree<TextMeasure>,
    display_size: [f32; 2],
    /// Lights the scrollbar thumbs of nodes under it.
    cursor: Option<Vec2>,
    on_node: &'a mut dyn FnMut(&T, LaidOut),
    /// Paint order of the next node.
    stack: u32,
}

impl <T: LayoutTree> Emit<'_, T> {
    /// The element of `node`, whose parent's top left corner is at `parent`, or `None` if it's
    /// hidden. Parts outside `clip` are left out.
    fn element(&mut self, lifted: &Lifted<T>, built: &Built, parent: Vec2, clip: ClipRect) -> Option<SDFElement> {
        let node = lifted.node;
        let style = node.style();
        if *style.display() == Display::None { return None }
        let parent = match built.anchor {
            Some(frame) => parent + location(self.tree, frame),
            None => parent,
        };
        let layout = self.tree.layout(built.node).ok()?;
        let top_left = parent + Vec2::new(layout.location.x, layout.location.y);
        let dimensions = Vec2::new(layout.size.width, layout.size.height);
        let center = top_left + dimensions * 0.5;
        let (padding, border) = (layout.padding, layout.border);
        // the content box's distance in from each edge: top, bottom, left, right
        let insets = [
            padding.top + border.top,
            padding.bottom + border.bottom,
            padding.left + border.left,
            padding.right + border.right,
        ];
        let own = ClipRect::new(top_left, top_left + dimensions);
        let scrolled = self.scrolled(style, built, layout, own);
        let offset = scrolled.map_or(Vec2::ZERO, |scrolled| scrolled.offset);
        let inner = clip.intersect(scrolled.map_or(own, |scrolled| scrolled.view));
        let (text, placement) = match style.text() {
            Some(text) => {
                let (element, placement) = build_text_element(text, insets, top_left - offset, dimensions, self.display_size, inner);
                (element, Some(placement))
            }
            None => (None, None),
        };
        let scroll = scrolled.map(|scrolled| scrolled.clipped(clip));
        (self.on_node)(node, LaidOut { top_left, size: dimensions, stack: self.stack, text: placement, clip, scroll });
        self.stack += 1;

        let mut children: Vec<SDFElement> = paint_order(&lifted.children, &built.children)
            .into_iter()
            .filter_map(|(child, built)| self.element(child, built, top_left - offset, inner))
            // what's cut off doesn't need drawing, which keeps long scrolled lists cheap
            .filter(|child| inner.overlaps(child.center - child.dimensions * 0.5, child.center + child.dimensions * 0.5))
            .collect();
        children.extend(text);
        if let Some(scrolled) = scrolled {
            // the content shows only inside the view, and the bars draw over it
            let view = flat(scrolled.view.min, scrolled.view.size(), TRANSPARENT, SDFShape::Empty);
            children = vec![SDFElement { children, ..view }];
            children.extend(scrollbar_elements(&scrolled, style.scrollbar(), self.cursor.is_some_and(|cursor| own.intersect(clip).contains(cursor))));
        }

        let shape = match style.background() {
            Background::Empty => SDFShape::Empty,
            Background::Shape(outline, _) => shape_in_content_box(outline, dimensions, insets),
            _ => SDFShape::Rectangle(SDFRectangle { radii: radii(style, dimensions, self.display_size) }),
        };
        Some(SDFElement {
            center,
            dimensions,
            style: background_to_style(style, border.top),
            shape,
            children,
            ..Default::default()
        })
    }

    /// Where the content of a node with `style`, laid out at `own`, scrolled, if it scrolls.
    fn scrolled(&self, style: &Style, built: &Built, layout: &taffy::Layout, own: ClipRect) -> Option<Scrolled> {
        if !style.scrolls() { return None }
        let taffy_style = self.tree.style(built.node).ok()?;
        let border = layout.border;
        let inner = ClipRect::new(own.min + Vec2::new(border.left, border.top), own.max - Vec2::new(border.right, border.bottom));
        let room = bar_room(taffy_style);
        let view = ClipRect::new(inner.min, (inner.max - room).max(inner.min));
        let overflow = content_overflow(layout, taffy_style);
        let max = Vec2::new(
            if style.overflow_x().scrolls() { overflow.x } else { 0.0 },
            if style.overflow_y().scrolls() { overflow.y } else { 0.0 },
        );
        let offset = style.scroll().clamp(Vec2::ZERO, max);
        let bar = style.scrollbar();
        let min_thumb = 2.0 * bar.width;
        let mut bars = [None, None];
        if room.y > 0.0 {
            let track = ClipRect::new(Vec2::new(view.min.x, inner.max.y - bar.width), Vec2::new(view.max.x, inner.max.y));
            bars[0] = Some(ScrollbarRects { track, thumb: thumb_rect(track, 0, view.size().x, max.x, offset.x, min_thumb) });
        }
        if room.x > 0.0 {
            let track = ClipRect::new(Vec2::new(inner.max.x - bar.width, view.min.y), Vec2::new(inner.max.x, view.max.y));
            bars[1] = Some(ScrollbarRects { track, thumb: thumb_rect(track, 1, view.size().y, max.y, offset.y, min_thumb) });
        }
        Some(Scrolled { view, offset, max, bars })
    }
}

/// The tracks and thumbs of `scrolled`'s bars, drawn as `bar` says, the thumbs lit if `hovered`.
fn scrollbar_elements(scrolled: &Scrolled, bar: &Scrollbar, hovered: bool) -> Vec<SDFElement> {
    let pill = || SDFShape::Rectangle(SDFRectangle { radii: Vec4::splat(bar.width / 2.0) });
    let thumb_color = if hovered { bar.thumb_hovered } else { bar.thumb };
    let mut elements = vec![];
    for rects in scrolled.bars.iter().flatten() {
        if bar.track.w > 0.0 { elements.push(flat(rects.track.min, rects.track.size(), bar.track, pill())); }
        if let Some(thumb) = rects.thumb && thumb_color.w > 0.0 { elements.push(flat(thumb.min, thumb.size(), thumb_color, pill())); }
    }
    elements
}

fn location(tree: &TaffyTree<TextMeasure>, node: NodeId) -> Vec2 {
    tree.layout(node).map_or(Vec2::ZERO, |layout| Vec2::new(layout.location.x, layout.location.y))
}

/// Corner radii as (bottom right, bottom left, top right, top left), `Percent` being of the
/// node's shorter side.
fn radii(style: &Style, dimensions: Vec2, display_size: [f32; 2]) -> Vec4 {
    let corners = style.border_radius();
    let resolve = |val: Val| match val {
        Val::Percent(fraction) => fraction * dimensions.min_element(),
        other => other.eval(&display_size).unwrap_or(0.0),
    };
    Vec4::new(
        resolve(*corners.bottom_right()),
        resolve(*corners.bottom_left()),
        resolve(*corners.top_right()),
        resolve(*corners.top_left()),
    )
}

fn background_to_style(style: &Style, border_width: f32) -> SDFStyle {
    let (primary_color, texture) = match style.background() {
        Background::Empty => (TRANSPARENT, None),
        Background::Color(c) => (*c, None),
        Background::Image(h) => (Vec4::ONE, Some(h.clone())),
        Background::Shape(_, c) => (*c, None),
    };
    SDFStyle {
        primary_color,
        border_color: style.border_color().unwrap_or(TRANSPARENT),
        border_width,
        texture,
    }
}

/// `outline` stretched over the content box of an element `dimensions` big, `insets` (top,
/// bottom, left, right) in from its edges.
fn shape_in_content_box(outline: &Outline, dimensions: Vec2, insets: [f32; 4]) -> SDFShape {
    let [top, bottom, left, right] = insets;
    let size = (dimensions - Vec2::new(left + right, top + bottom)).max(Vec2::ZERO);
    let offset = Vec2::new(left - right, top - bottom) / 2.0;
    SDFShape::Glyph(outline.placed(size, offset))
}

/// Where a node's text was drawn, in window pixels, for hit testing it.
#[derive(Clone, Debug)]
pub struct TextPlacement {
    pub layout: Arc<TextLayout>,
    /// Where the layout's top left corner went.
    pub origin: Vec2,
    /// The box the text shows in, its content box when it clips.
    pub view_min: Vec2,
    pub view_max: Vec2,
    /// The part of the window its node shows in.
    pub clip: ClipRect,
}

impl TextPlacement {
    /// `point` in the layout's coordinates.
    pub fn local(&self, point: Vec2) -> Vec2 { point - self.origin }

    /// Whether the text shows at `point`.
    pub fn shows(&self, point: Vec2) -> bool {
        point.cmpge(self.view_min).all() && point.cmple(self.view_max).all() && self.clip.contains(point)
    }
}

/// A flat colored box.
fn flat(min: Vec2, size: Vec2, color: Vec4, shape: SDFShape) -> SDFElement {
    let style = SDFStyle { primary_color: color, border_color: TRANSPARENT, border_width: 0.0, texture: None };
    SDFElement { center: min + size * 0.5, dimensions: size, style, shape, ..Default::default() }
}

fn rectangle() -> SDFShape { SDFShape::Rectangle(SDFRectangle::default()) }

/// `text` laid out in the box at `top_left` sized `dimensions`, `insets` (top, bottom, left,
/// right) in from its edges: span backgrounds and the selection, then the lines of glyphs, then
/// underlines and the caret. Lines outside the window, outside `clip`, or outside the content
/// box of clipped text, are left out.
fn build_text_element(text: &Text, insets: [f32; 4], top_left: Vec2, dimensions: Vec2, display_size: [f32; 2], clip: ClipRect) -> (Option<SDFElement>, TextPlacement) {
    let [top, bottom, left, right] = insets;
    let content_min = top_left + Vec2::new(left, top);
    let content_size = (dimensions - Vec2::new(left + right, top + bottom)).max(Vec2::ZERO);
    let mut layout = text.font.layout(&text.content, text.font_size, text.wrap.then_some(content_size.x));
    layout.align(text.horizontal_align, content_size.x);
    let size = layout.size();
    let y = match text.vertical_align {
        Align::Start => 0.0,
        Align::End => content_size.y - size.y,
        Align::Center => (content_size.y - size.y) * 0.5,
    };
    let origin = content_min + Vec2::new(0.0, y) - text.scroll;
    let caret_width = crate::ui::caret_width(text.font_size);
    // a caret at either end of clipped text still shows
    let (view_min, view_max) = if text.clip {
        (content_min - Vec2::new(caret_width, 0.0), content_min + content_size + Vec2::new(caret_width, 0.0))
    } else {
        (Vec2::ZERO, Vec2::from_array(display_size))
    };
    // what's drawn: the view, less what an ancestor cuts off
    let shown = clip.intersect(ClipRect::new(view_min, view_max));

    let len = text.content.len();
    let clamp = |range: &std::ops::Range<usize>| range.start.min(len)..range.end.min(len);
    let line_height = layout.line_height;
    let mut children = vec![];
    let boxes = |rects: Vec<(Vec2, Vec2)>, color: Vec4, children: &mut Vec<SDFElement>| {
        for (min, size) in rects {
            let min = origin + min;
            if min.y > shown.max.y || min.y + size.y < shown.min.y { continue }
            children.push(flat(min, size, color, rectangle()));
        }
    };
    for span in &text.spans {
        if let Some(background) = span.background { boxes(layout.range_rects(clamp(&span.range)), background, &mut children); }
    }
    if let Some(cursor) = &text.cursor && let Some(selection) = &cursor.selection {
        boxes(layout.range_rects(clamp(selection)), cursor.selection_color, &mut children);
    }

    // the span coloring each byte, later spans winning
    let mut colored = vec![];
    if text.spans.iter().any(|span| span.color.is_some()) {
        colored = vec![u32::MAX; len];
        for (idx, span) in text.spans.iter().enumerate().filter(|(_, span)| span.color.is_some()) {
            for at in clamp(&span.range) { colored[at] = idx as u32; }
        }
    }
    let first = layout.line_at(shown.min.y - origin.y);
    let last = layout.line_at(shown.max.y - origin.y);
    for (row, line) in layout.lines.iter().enumerate().take(last + 1).skip(first) {
        let line_y = origin.y + row as f32 * line_height + line_height * 0.5;
        if line_y + line_height * 0.5 < shown.min.y || line_y - line_height * 0.5 > shown.max.y { continue }
        let glyphs: Vec<SDFElement> = line.glyphs.iter()
            .filter(|glyph| {
                let x = origin.x + line.x + glyph.x;
                x + glyph.advance >= shown.min.x && x <= shown.max.x
            })
            .filter_map(|glyph| {
                let shape = text.font.glyph(glyph.ch, text.font_size)?;
                let color = colored.get(glyph.index).and_then(|idx| text.spans.get(*idx as usize)).and_then(|span| span.color).unwrap_or(text.color);
                let center = Vec2::new(origin.x + line.x + glyph.x + glyph.advance * 0.5, line_y);
                Some(flat(center - Vec2::new(glyph.advance, line_height) * 0.5, Vec2::new(glyph.advance, line_height), color, shape))
            })
            .collect();
        if glyphs.is_empty() { continue }
        let mut element = flat(Vec2::new(origin.x + line.x, line_y - line_height * 0.5), Vec2::new(line.width, line_height), text.color, SDFShape::Empty);
        element.children = glyphs;
        children.push(element);
    }

    for span in &text.spans {
        let Some(underline) = span.underline else { continue };
        let (offset, height, shape) = match underline.style {
            UnderlineStyle::Solid => (text.font_size * 0.08, (text.font_size / 14.0).max(2.0), rectangle()),
            UnderlineStyle::Wavy => (text.font_size * 0.06, (text.font_size * 0.25).max(4.0), SDFShape::Wave),
        };
        for (min, size) in layout.range_rects(clamp(&span.range)) {
            let mut min = origin + Vec2::new(min.x, min.y + layout.baseline + offset);
            // thin lines centered on a pixel draw crisp
            if underline.style == UnderlineStyle::Solid { min.y = (min.y + height * 0.5).floor() + 0.5 - height * 0.5; }
            if min.y > shown.max.y || min.y + height < shown.min.y { continue }
            children.push(flat(min, Vec2::new(size.x, height), underline.color, shape.clone()));
        }
    }
    if let Some(cursor) = text.cursor.as_ref().filter(|cursor| cursor.show_caret) {
        let at = origin + layout.caret_at(cursor.caret.min(len));
        // centered on a pixel to draw crisp
        let x = at.x.floor() + 0.5;
        children.push(flat(Vec2::new(x - caret_width * 0.5, at.y), Vec2::new(caret_width, line_height), cursor.caret_color, rectangle()));
    }

    let placement = TextPlacement { layout: Arc::new(layout), origin, view_min, view_max, clip };
    if children.is_empty() { return (None, placement) }
    let (min, max) = if text.clip {
        (view_min, view_max)
    } else {
        children.iter().fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), child| {
            (min.min(child.center - child.dimensions * 0.5), max.max(child.center + child.dimensions * 0.5))
        })
    };
    let element = SDFElement {
        center: (min + max) * 0.5,
        dimensions: max - min,
        style: SDFStyle { primary_color: text.color, border_color: TRANSPARENT, border_width: 0.0, texture: None },
        shape: SDFShape::Empty,
        children,
        ..Default::default()
    };
    (Some(element), placement)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: [f32; 2] = [800.0, 600.0];

    fn sized(width: f32, height: f32) -> UINode {
        let mut node = UINode::default();
        node.set_width(Val::Px(width));
        node.set_height(Val::Px(height));
        node
    }

    fn layout(nodes: &[UINode]) -> Vec<SDFElement> {
        layout_ui_nodes(&World::default(), &UIInputTracker::default(), nodes, SCREEN)
    }

    /// Top left corner and size of `element`.
    fn bounds(element: &SDFElement) -> (Vec2, Vec2) { (element.center - element.dimensions / 2.0, element.dimensions) }

    #[test]
    fn grow_takes_leftover_room() {
        let mut row = sized(300.0, 40.0);
        row.set_row(JustifyContent::Start, AlignItems::Stretch);
        row.add(sized(50.0, 20.0));
        let mut grow = UINode::default();
        grow.set_flex_grow(1.0);
        row.add(grow);
        let elements = layout(&[row]);
        let (pos, size) = bounds(&elements[0].children[1]);
        assert_eq!((pos, size), (Vec2::new(50.0, 0.0), Vec2::new(250.0, 40.0)));
    }

    #[test]
    fn justify_and_gap() {
        let mut row = sized(300.0, 40.0);
        row.set_row(JustifyContent::SpaceBetween, AlignItems::Center);
        row.add_all((0..3).map(|_| sized(50.0, 20.0)));
        let elements = layout(&[row]);
        let xs: Vec<f32> = elements[0].children.iter().map(|child| bounds(child).0.x).collect();
        assert_eq!(xs, vec![0.0, 125.0, 250.0]);
        assert_eq!(bounds(&elements[0].children[0]).0.y, 10.0);

        let mut column = sized(100.0, 300.0);
        column.set_gap(Val::Px(5.0));
        column.add_all((0..2).map(|_| sized(50.0, 20.0)));
        let elements = layout(&[column]);
        assert_eq!(bounds(&elements[0].children[1]).0.y, 25.0);
    }

    #[test]
    fn wraps_and_percent() {
        let mut row = sized(400.0, 200.0);
        row.set_row(JustifyContent::Start, AlignItems::Start);
        row.set_flex_wrap(FlexWrap::Wrap);
        row.set_align_content(AlignContent::Start);
        row.add_all((0..3).map(|_| sized(150.0, 20.0)));
        let mut half = UINode::default();
        half.set_width(Val::Percent(0.5));
        half.set_height(Val::Px(10.0));
        row.add(half);
        let elements = layout(&[row]);
        let children = &elements[0].children;
        assert_eq!(bounds(&children[2]).0, Vec2::new(0.0, 20.0));
        assert_eq!(bounds(&children[3]).1.x, 200.0);
    }

    #[test]
    fn border_and_padding_are_inside() {
        let mut node = sized(100.0, 100.0);
        node.set_padding(Rect::single(Val::Px(9.0)));
        node.set_border(Val::Px(1.0));
        node.add(UINode::default());
        let elements = layout(&[node]);
        assert_eq!(bounds(&elements[0].children[0]), (Vec2::splat(10.0), Vec2::new(80.0, 0.0)));
    }

    #[test]
    fn shape_fills_the_content_box() {
        let triangle = Outline::builder().polygon(&[Vec2::new(-0.5, -0.5), Vec2::new(0.5, 0.0), Vec2::new(-0.5, 0.5)]).build();
        let mut node = sized(100.0, 60.0);
        node.set_padding(Rect::new(Val::Px(10.0), Val::Px(10.0), Val::Px(20.0), Val::Px(0.0)));
        node.set_background(Background::Shape(triangle, Vec4::ONE));
        let elements = layout(&[node]);
        let SDFShape::Glyph(curves) = &elements[0].shape else { panic!("expected a glyph, got {:?}", elements[0].shape) };
        assert_eq!(elements[0].style.primary_color, Vec4::ONE);
        // the 80x40 content box is centered 10px right of the node's center
        assert_eq!(curves[0].a_offset, Vec2::new(-30.0, -20.0));
        assert_eq!(curves[0].c_offset, Vec2::new(50.0, 0.0));
        assert_eq!(curves[1].c_offset, Vec2::new(-30.0, 20.0));
    }

    #[test]
    fn text_sizes_its_node() {
        let font = Arc::new(SDFFont::new(include_bytes!("../../../examples/LiberationSans-Regular.ttf")).unwrap());
        let mut row = sized(400.0, 100.0);
        row.set_row(JustifyContent::Start, AlignItems::Start);
        let mut label = UINode::default();
        label.set_padding(Rect::single(Val::Px(4.0)));
        label.set_text(Some(Text::new(font.clone(), "Hello", Vec4::ONE, 20.0)));
        row.add(label);
        let elements = layout(&[row]);
        let expected = font.measure_text("Hello", 20.0) + Vec2::splat(8.0);
        assert!((bounds(&elements[0].children[0]).1 - expected).abs().max_element() < 0.01);
    }

    #[test]
    fn hidden_nodes_take_no_room() {
        let mut column = sized(100.0, 100.0);
        let mut hidden = sized(50.0, 50.0);
        hidden.set_display(Display::None);
        column.add(hidden);
        column.add(sized(50.0, 20.0));
        let elements = layout(&[column]);
        assert_eq!(elements[0].children.len(), 1);
        assert_eq!(bounds(&elements[0].children[0]).0, Vec2::ZERO);
    }

    #[test]
    fn anchors_place_roots_in_the_window() {
        let anchored = |anchor: Anchor| {
            let mut node = sized(100.0, 50.0);
            node.set_anchored(anchor);
            node.set_margin(Rect::single(Val::Px(10.0)));
            bounds(&layout(&[node])[0])
        };
        let size = Vec2::new(100.0, 50.0);
        assert_eq!(anchored(Anchor::TopLeft), (Vec2::new(10.0, 10.0), size));
        assert_eq!(anchored(Anchor::Top), (Vec2::new(350.0, 10.0), size));
        assert_eq!(anchored(Anchor::Center), (Vec2::new(350.0, 275.0), size));
        assert_eq!(anchored(Anchor::BottomRight), (Vec2::new(690.0, 540.0), size));
        assert_eq!(anchored(Anchor::Left), (Vec2::new(10.0, 275.0), size));

        let filled = |anchor: Anchor| {
            let mut node = UINode::default();
            node.set_anchored(anchor);
            node.set_width(Val::Px(100.0));
            node.set_height(Val::Px(50.0));
            if matches!(anchor, Anchor::FillTop | Anchor::FillBottom) { node.set_width(Val::Auto); }
            if matches!(anchor, Anchor::FillLeft | Anchor::FillRight) { node.set_height(Val::Auto); }
            if anchor == Anchor::Fill { node.set_width(Val::Auto); node.set_height(Val::Auto); }
            bounds(&layout(&[node])[0])
        };
        assert_eq!(filled(Anchor::FillTop), (Vec2::ZERO, Vec2::new(800.0, 50.0)));
        assert_eq!(filled(Anchor::FillBottom), (Vec2::new(0.0, 550.0), Vec2::new(800.0, 50.0)));
        assert_eq!(filled(Anchor::FillLeft), (Vec2::ZERO, Vec2::new(100.0, 600.0)));
        assert_eq!(filled(Anchor::FillRight), (Vec2::new(700.0, 0.0), Vec2::new(100.0, 600.0)));
        assert_eq!(filled(Anchor::Fill), (Vec2::ZERO, Vec2::new(800.0, 600.0)));
    }

    #[test]
    fn anchors_inside_a_parent_and_paint_on_top() {
        let mut panel = sized(200.0, 200.0);
        panel.set_margin(Rect::new_left(Val::Px(100.0)));
        panel.add(sized(20.0, 20.0));
        let mut badge = sized(20.0, 20.0);
        badge.set_anchored(Anchor::BottomRight);
        panel.add(badge);
        panel.add(sized(20.0, 20.0));
        let elements = layout(&[panel]);
        let children = &elements[0].children;
        // flow children first, then the anchored one, which takes no room
        assert_eq!(bounds(&children[1]).0, Vec2::new(100.0, 20.0));
        assert_eq!(bounds(&children[2]).0, Vec2::new(280.0, 180.0));
    }

    /// A column 100 wide and up to 100 tall holding `rows` rows 30 tall, scrolled `scroll`.
    fn list(rows: usize, overflow: Overflow, scroll: f32) -> UINode {
        let mut list = UINode::default();
        list.set_width(Val::Px(100.0));
        list.set_max_height(Val::Px(100.0));
        list.set_overflow_y(overflow);
        list.set_scroll(Vec2::new(0.0, scroll));
        list.set_scrollbar(Scrollbar { width: 6.0, gap: 4.0, ..Default::default() });
        list.add_all((0..rows).map(|_| { let mut row = UINode::default(); row.set_height(Val::Px(30.0)); row.set_flex_shrink(0.0); row }));
        list
    }

    /// Where each node went, in paint order.
    fn placed(nodes: &[UINode]) -> Vec<LaidOut> {
        let mut placed = vec![];
        layout_tree(nodes, Vec2::ZERO, Vec2::from_array(SCREEN), SCREEN, None, &mut |_: &UINode, laid_out| placed.push(laid_out));
        placed
    }

    #[test]
    fn auto_overflow_scrolls_only_when_it_overflows() {
        // fits: no bar, the rows fill the width
        let fits = placed(&[list(3, Overflow::Auto, 0.0)]);
        let scroll = fits[0].scroll.unwrap();
        assert_eq!((fits[0].size, scroll.max, scroll.bars), (Vec2::new(100.0, 90.0), Vec2::ZERO, [None, None]));
        assert_eq!(fits[1].size.x, 100.0);

        // overflows: capped height, a bar's room taken from the rows, scrolled within range
        let overflows = placed(&[list(10, Overflow::Auto, 1000.0)]);
        let scroll = overflows[0].scroll.unwrap();
        assert_eq!((overflows[0].size, scroll.max, scroll.offset), (Vec2::new(100.0, 100.0), Vec2::new(0.0, 200.0), Vec2::new(0.0, 200.0)));
        assert_eq!(overflows[1].size.x, 90.0);
        // the last row ends at the bottom, the first is scrolled out of view
        assert_eq!(overflows[10].top_left.y, 70.0);
        assert!(!overflows[1].clip.overlaps(overflows[1].top_left, overflows[1].top_left + overflows[1].size));
        let bar = scroll.bars[1].unwrap();
        assert_eq!(bar.track, ClipRect::new(Vec2::new(94.0, 0.0), Vec2::new(100.0, 100.0)));
        // a third of the content shows, at the end
        let thumb = bar.thumb.unwrap();
        assert!((thumb.min - Vec2::new(94.0, 200.0 / 3.0)).abs().max_element() < 1e-3 && thumb.max == Vec2::splat(100.0));
        // halfway down the free track is halfway through the content
        assert_eq!(scroll.offset_at(1, 0.0), 0.0);
        assert!((scroll.offset_at(1, 100.0 / 3.0) - 100.0).abs() < 1e-3);

        // scroll keeps the bar's room even when it fits
        let always = placed(&[list(3, Overflow::Scroll, 0.0)]);
        assert_eq!(always[1].size.x, 90.0);
        assert_eq!(always[0].scroll.unwrap().bars[1].unwrap().thumb, None);
    }

    #[test]
    fn scrolled_out_children_are_not_drawn() {
        let elements = layout(&[list(10, Overflow::Auto, 45.0)]);
        // the view, then the track and thumb
        assert_eq!(elements[0].children.len(), 3);
        let shown: Vec<f32> = elements[0].children[0].children.iter().map(|row| bounds(row).0.y).collect();
        assert_eq!(shown, vec![-15.0, 15.0, 45.0, 75.0]);
    }

    #[test]
    fn fixed_nodes_escape_their_parents() {
        let mut parent = sized(50.0, 50.0);
        parent.set_margin(Rect::new_left(Val::Px(100.0)));
        let mut menu = sized(80.0, 80.0);
        menu.set_position_type(PositionType::Fixed(Rect::new_top_left(Val::Px(300.0), Val::Px(400.0))));
        parent.add(menu);
        let mut hidden = sized(10.0, 10.0);
        hidden.set_display(Display::None);
        let mut inner = sized(10.0, 10.0);
        inner.set_position_type(PositionType::Fixed(Rect::default()));
        hidden.add(inner);
        parent.add(hidden);
        let elements = layout(&[parent]);
        // a root of its own placed against the window, and dropped inside a hidden node
        assert_eq!(elements.len(), 2);
        assert!(elements[0].children.is_empty());
        assert_eq!(bounds(&elements[1]), (Vec2::new(400.0, 300.0), Vec2::splat(80.0)));
    }
}
