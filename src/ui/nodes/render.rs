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
    JustifyContent, LastState, PositionType, Rect, SDFElement, SDFFont, SDFRectangle, SDFShape,
    SDFStyle, Style, Text, UIInputTracker, UINode, Val,
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
    layout_tree(nodes, origin, available, display_size, &mut |node: &UINode, laid_out| {
        let (min, max) = (laid_out.top_left, laid_out.top_left + laid_out.size);
        let cursor = input_tracker.cursor_position;
        let is_hover = cursor.cmpge(min).all() && cursor.cmple(max).all();
        let interaction = match (is_hover, input_tracker.is_cursor_down) {
            (true, true) => Interaction::Pressed,
            (true, false) => Interaction::Hovered,
            (false, _) => Interaction::None,
        };
        node.set_last_state(world, LastState { position: min + laid_out.size * 0.5, size: laid_out.size, interaction });
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
#[derive(Clone, Copy, Debug)]
pub(crate) struct LaidOut {
    pub top_left: Vec2,
    pub size: Vec2,
    /// Paint order, later nodes being drawn over earlier ones.
    pub stack: u32,
}

/// Lays `nodes` out as a column in the `available` pixels at `origin` like `layout_children`,
/// calling `on_node` with where each shown node went.
pub(crate) fn layout_tree<T: LayoutTree>(
    nodes: &[T],
    origin: Vec2,
    available: Vec2,
    display_size: [f32; 2],
    on_node: &mut dyn FnMut(&T, LaidOut),
) -> Vec<SDFElement> {
    let mut layout = Layout { tree: TaffyTree::new(), display_size };
    // fractional positions, as the SDFs are antialiased anyway
    layout.tree.disable_rounding();
    let roots = nodes.iter().map(|node| layout.build(node)).collect::<Vec<_>>();
    let parent = taffy::Style {
        flex_direction: taffy::FlexDirection::Column,
        size: taffy::Size { width: length(available.x), height: length(available.y) },
        ..Default::default()
    };
    let Ok(viewport) = layout.tree.new_with_children(parent, &roots.iter().map(Built::outer).collect::<Vec<_>>()) else { return vec![] };
    let space = taffy::Size { width: AvailableSpace::Definite(available.x), height: AvailableSpace::Definite(available.y) };
    let computed = layout.tree.compute_layout_with_measure(viewport, space, |inputs, _, context, style| {
        taffy::compute_leaf_layout(inputs, style, |_, _| 0.0, |known, _| {
            let measured = context.map_or(Vec2::ZERO, |text| text.font.measure_text(&text.content, text.font_size));
            taffy::Size { width: known.width.unwrap_or(measured.x), height: known.height.unwrap_or(measured.y) }
        })
    });
    if let Err(err) = computed {
        log::error!("Failed to lay out the UI: {err}");
        return vec![];
    }
    let mut emit = Emit { tree: &layout.tree, display_size, on_node, stack: 0 };
    paint_order(nodes, &roots).into_iter().filter_map(|(node, built)| emit.element(node, built, origin)).collect()
}

/// Text a leaf is measured by.
struct TextMeasure {
    font: Arc<SDFFont>,
    content: String,
    font_size: f32,
}

/// The taffy tree mirroring the nodes being laid out.
struct Layout {
    tree: TaffyTree<TextMeasure>,
    display_size: [f32; 2],
}

/// Where a `UINode` and its children went in the taffy tree.
struct Built {
    node: NodeId,
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
    fn build<T: LayoutTree>(&mut self, node: &T) -> Built {
        let children: Vec<Built> = node.children().iter().map(|child| self.build(child)).collect();
        let mut style = self.style(node.style(), node.children());
        let anchor = *node.style().anchor();
        if let Some(anchor) = anchor { anchored(&mut style, anchor); }
        let id = if let Some(text) = node.style().text().as_ref().filter(|_| children.is_empty()) {
            let measure = TextMeasure { font: text.font.clone(), content: text.content.clone(), font_size: text.font_size };
            self.tree.new_leaf_with_context(style, measure)
        } else {
            self.tree.new_with_children(style, &children.iter().map(Built::outer).collect::<Vec<_>>())
        };
        // taffy only fails on node ids it didn't hand out
        let id = id.expect("taffy rejected a new node");
        let anchor = anchor.map(|anchor| {
            self.tree.new_with_children(anchor_frame(anchor), &[id]).expect("taffy rejected a new node")
        });
        Built { node: id, anchor, children }
    }

    /// `style` in taffy's terms, `children` being the node's.
    fn style<T: LayoutTree>(&self, style: &Style, children: &[T]) -> taffy::Style {
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
            PositionType::Absolute(edges) => (taffy::Position::Absolute, rect(edges, &offset)),
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
fn paint_order<'a, T: LayoutTree>(nodes: &'a [T], built: &'a [Built]) -> Vec<(&'a T, &'a Built)> {
    let mut order: Vec<(&T, &Built)> = nodes.iter().zip(built).collect();
    // stable, so siblings keep their order otherwise
    order.sort_by_key(|(node, _)| (*node.style().z_index(), !in_flow(node.style())));
    order
}

/// Turns laid out nodes into elements.
struct Emit<'a, T> {
    tree: &'a TaffyTree<TextMeasure>,
    display_size: [f32; 2],
    on_node: &'a mut dyn FnMut(&T, LaidOut),
    /// Paint order of the next node.
    stack: u32,
}

impl <T: LayoutTree> Emit<'_, T> {
    /// The element of `node`, whose parent's top left corner is at `parent`, or `None` if it's
    /// hidden.
    fn element(&mut self, node: &T, built: &Built, parent: Vec2) -> Option<SDFElement> {
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
        (self.on_node)(node, LaidOut { top_left, size: dimensions, stack: self.stack });
        self.stack += 1;

        let mut children: Vec<SDFElement> = paint_order(node.children(), &built.children)
            .into_iter()
            .filter_map(|(child, built)| self.element(child, built, top_left))
            .collect();

        if let Some(text) = style.text() {
            let insets = [
                padding.top + border.top,
                padding.bottom + border.bottom,
                padding.left + border.left,
                padding.right + border.right,
            ];
            children.extend(build_text_element(text, insets, top_left, dimensions));
        }

        let shape = match style.background() {
            Background::Empty => SDFShape::Empty,
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
    };
    SDFStyle {
        primary_color,
        border_color: style.border_color().unwrap_or(TRANSPARENT),
        border_width,
        texture,
    }
}

/// The glyphs of `text` placed in the box at `top_left` sized `dimensions`, `insets` (top,
/// bottom, left, right) in from its edges.
fn build_text_element(text: &Text, insets: [f32; 4], top_left: Vec2, dimensions: Vec2) -> Option<SDFElement> {
    let line = text.font.render_glyph_line(&text.content, text.font_size).ok()?;
    let (total_w, total_h) = (line.dimensions.x, line.dimensions.y);
    let [top, bottom, left, right] = insets;
    let place = |align: Align, start: f32, size: f32, before: f32, after: f32, total: f32| match align {
        Align::Start => start + before,
        Align::End => start + size - after - total,
        Align::Center => start + before + (size - before - after - total) * 0.5,
    };
    let x_start = place(text.horizontal_align, top_left.x, dimensions.x, left, right, total_w);
    let y_start = place(text.vertical_align, top_left.y, dimensions.y, top, bottom, total_h);

    let glyph_style = SDFStyle { primary_color: text.color, border_color: TRANSPARENT, border_width: 0.0, texture: None };
    let mut cur_x = x_start;
    let glyph_y = y_start + total_h * 0.5;
    let glyphs: Vec<SDFElement> = line
        .chars
        .into_iter()
        .filter_map(|entry| {
            let advance = entry.advance;
            let center = Vec2::new(cur_x + advance * 0.5, glyph_y);
            cur_x += advance;
            entry.shape.map(|shape| SDFElement {
                center,
                dimensions: entry.dimensions,
                style: glyph_style.clone(),
                shape,
                ..Default::default()
            })
        })
        .collect();

    Some(SDFElement {
        center: Vec2::new(x_start + total_w * 0.5, y_start + total_h * 0.5),
        dimensions: Vec2::new(total_w, total_h),
        style: glyph_style,
        shape: SDFShape::Empty,
        children: glyphs,
        ..Default::default()
    })
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
    fn text_sizes_its_node() {
        let font = Arc::new(SDFFont::new(include_bytes!("../../../examples/LiberationSans-Regular.ttf")).unwrap());
        let mut row = sized(400.0, 100.0);
        row.set_row(JustifyContent::Start, AlignItems::Start);
        let mut label = UINode::default();
        label.set_padding(Rect::single(Val::Px(4.0)));
        label.set_text(Some(Text {
            font: font.clone(),
            content: "Hello".into(),
            color: Vec4::ONE,
            font_size: 20.0,
            horizontal_align: Align::Start,
            vertical_align: Align::Start,
        }));
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
}
