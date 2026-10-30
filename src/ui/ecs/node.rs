//! The components of entity nodes, and spawning trees of them.

use anarchy::{Component, DynComponent, EntityBuilder, EntityID, World, next_entity_id};
use magician_vgpu::glam::{Vec2, Vec4};

use crate::ui::{
    AlignItems, Anchor, Background, Display, FlexWrap, Interaction, JustifyContent, Outline, PositionType, Rect, RectCorners,
    Style, Text, ThemeOverride, Val,
};

/// A UI node entity: its layout and look. Children are its children in the world's
/// `Hierarchy`, and a node without a `Node` parent is a root, laid out in the window. Sizes are
/// logical pixels, scaled to the window's scale factor when laid out.
#[derive(Debug, Default, Clone, Component)]
pub struct Node(pub Style);

/// `v` logical pixels.
pub fn px(v: f32) -> Val { Val::Px(v) }

/// A fraction of the parent's size, 0.5 being half.
pub fn percent(fraction: f32) -> Val { Val::Percent(fraction) }

impl Node {
    pub fn new() -> Self { Self::default() }

    pub fn style(&self) -> &Style { &self.0 }

    /// Changes the node through `Style`'s setters.
    pub fn style_mut(&mut self) -> &mut Style { &mut self.0 }

    /// Lays children out left to right.
    pub fn row() -> Self { Self::default().direction_row() }

    /// Lays children out top to bottom, the default.
    pub fn column() -> Self { Self::default() }

    fn direction_row(mut self) -> Self {
        self.0.set_flex_direction(crate::ui::FlexDirection::Row);
        self
    }

    pub fn width(mut self, width: Val) -> Self { self.0.set_width(width); self }

    pub fn height(mut self, height: Val) -> Self { self.0.set_height(height); self }

    pub fn size(self, width: Val, height: Val) -> Self { self.width(width).height(height) }

    pub fn min_width(mut self, width: Val) -> Self { self.0.set_min_width(width); self }

    pub fn min_height(mut self, height: Val) -> Self { self.0.set_min_height(height); self }

    pub fn max_width(mut self, width: Val) -> Self { self.0.set_max_width(width); self }

    pub fn max_height(mut self, height: Val) -> Self { self.0.set_max_height(height); self }

    pub fn margin(mut self, margin: Rect) -> Self { self.0.set_margin(margin); self }

    pub fn padding(mut self, padding: Rect) -> Self { self.0.set_padding(padding); self }

    /// Room between children.
    pub fn gap(mut self, gap: Val) -> Self { self.0.set_gap(gap); self }

    pub fn justify(mut self, justify: JustifyContent) -> Self { self.0.set_justify_content(justify); self }

    pub fn align(mut self, align: AlignItems) -> Self { self.0.set_align_items(align); self }

    pub fn align_self(mut self, align: AlignItems) -> Self { self.0.set_align_self(Some(align)); self }

    pub fn wrap(mut self) -> Self { self.0.set_flex_wrap(FlexWrap::Wrap); self }

    pub fn grow(mut self, grow: f32) -> Self { self.0.set_flex_grow(grow); self }

    pub fn shrink(mut self, shrink: f32) -> Self { self.0.set_flex_shrink(shrink); self }

    pub fn basis(mut self, basis: Val) -> Self { self.0.set_flex_basis(basis); self }

    pub fn background(mut self, color: Vec4) -> Self { self.0.set_background(Background::Color(color)); self }

    /// Draws `outline` in `color` over the content box instead of a background, see `Outline`.
    pub fn shape(mut self, outline: Outline, color: Vec4) -> Self { self.0.set_background(Background::Shape(outline, color)); self }

    /// A border `width` logical pixels wide in `color`, inside the node's size.
    pub fn border(mut self, width: f32, color: Vec4) -> Self {
        self.0.set_border(Val::Px(width));
        self.0.set_border_color(Some(color));
        self
    }

    /// Rounds every corner by `radius`.
    pub fn radius(mut self, radius: Val) -> Self { self.0.set_border_radius(RectCorners::single(radius)); self }

    /// Pins the node to `anchor` of its parent, or of the window for a root.
    pub fn anchored(mut self, anchor: Anchor) -> Self { self.0.set_anchored(anchor); self }

    /// Places the node by its offsets from the parent's edges, out of the flow.
    pub fn absolute(mut self, edges: Rect) -> Self { self.0.set_position_type(PositionType::Absolute(edges)); self }

    pub fn z_index(mut self, z_index: i32) -> Self { self.0.set_z_index(z_index); self }

    pub fn display(mut self, display: Display) -> Self { self.0.set_display(display); self }

    pub fn text(mut self, text: Text) -> Self { self.0.set_text(Some(text)); self }
}

/// Where an entity node was last laid out, in window pixels. Added by `spawn_ui`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Component)]
pub struct ComputedNode {
    pub position: Vec2,
    pub size: Vec2,
    /// Paint order, later nodes being drawn over earlier ones.
    pub stack: u32,
    /// The layout pass that placed it, behind `EntityUiElements::pass` while it's hidden.
    pub pass: u64,
    /// It has a background, so the pointer over it is over the UI.
    pub opaque: bool,
}

impl ComputedNode {
    pub fn contains(&self, point: Vec2) -> bool {
        point.cmpge(self.position).all() && point.cmple(self.position + self.size).all()
    }
}

/// A name to find an entity by, see `find_named`.
#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct Name(pub String);

/// An entity node waiting to be spawned, with the components and children to spawn with it.
#[derive(Debug)]
pub struct UiBundle {
    pub node: Node,
    components: Vec<DynComponent>,
    children: Vec<UiBundle>,
    id: Option<EntityID>,
}

impl UiBundle {
    /// Create a new UI bundle with a given `Node`.
    pub fn new(node: Node) -> Self { Self { node, components: vec![], children: vec![], id: None } }

    /// Spawns `component` on the node too.
    pub fn with(mut self, component: impl Component) -> Self { self.components.push(Box::new(component)); self }

    /// Makes the node pickable, see `Interaction`.
    pub fn interactive(self) -> Self { self.with(Interaction::None) }

    /// Apply the given name to this bundle.
    pub fn named(self, name: impl Into<String>) -> Self { self.with(Name(name.into())) }

    /// Draws the node and everything below it with `theme`'s changes to the `UiTheme`, see
    /// `ThemeOverride`.
    pub fn theme(self, theme: ThemeOverride) -> Self { self.with(theme) }

    /// Add a child to the children of this bundle.
    pub fn child(mut self, child: UiBundle) -> Self { self.children.push(child); self }

    /// Append an iterator of children to this bundle.
    pub fn children(mut self, children: impl IntoIterator<Item = UiBundle>) -> Self { self.children.extend(children); self }

    /// Spawns the node under `id`, for widgets that need their parts' ids before spawning.
    pub fn with_id(mut self, id: EntityID) -> Self { self.id = Some(id); self }

    /// Changes the node's layout and look.
    pub fn style(mut self, change: impl FnOnce(Node) -> Node) -> Self { self.node = change(self.node); self }

    /// Convert this bundle to an `EntityBuilder`.
    pub fn into_entity_builder(self) -> EntityBuilder {
        let id = self.id.unwrap_or_else(|| next_entity_id());
        let builder = EntityBuilder::with_id_and_capacity(id, 2)
            .push(self.node)
            .push(ComputedNode::default())
            .extend(self.components.into_iter())
            .extend_children(self.children.into_iter().map(|child| child.into_entity_builder()));
        return builder;
    }
}

// /// Spawns `bundle` and its children, returning the root's id.
// pub fn spawn_ui(world: &World, bundle: UiBundle) -> EntityID {
//     let UiBundle { node, mut components, children, id } = bundle;
//     let id = id.unwrap_or_else(|| next_entity_id());
//     components.push(Box::new(node));
//     components.push(Box::new(ComputedNode::default()));
//     world.insert(id, components.into_iter().map(RelaxedMutex::new).collect());
//     for child in children { spawn_ui_under(world, id, child); }
//     id
// }
//
// /// Spawns `bundle` as the last child of `parent`, returning its id.
// pub fn spawn_ui_under(world: &World, parent: EntityID, bundle: UiBundle) -> EntityID {
//     let id = spawn_ui(world, bundle);
//     if let Err(err) = world.hierarchy().add_child(parent, id) { log::error!("Failed to parent a UI node: {err:#}"); }
//     id
// }

/// Allocates an id for an entity spawned later, see `UiBundle::with_id`.
pub fn reserve_id() -> EntityID { next_entity_id() }

/// The first entity under `root`, or `root` itself, with the `Name` `name`.
pub fn find_named(world: &World, root: EntityID, name: &str) -> Option<EntityID> {
    let named = |id: EntityID| world.handle(id).get::<&Name>().ok().flatten().is_some_and(|found| found.0 == name);
    std::iter::once(root).chain(world.hierarchy().descendants(root)).find(|id| named(*id))
}
