//! Lays entity nodes out and hands them to the render pass.

use ahash::AHashMap;
use anarchy::{EntityID, Query, Res, ResMut, Resource, World};
use crate::WindowDimensions;
use magician_vgpu::glam::Vec2;

use crate::ui::{Background, ComputedNode, Node, SDFElement, Style, UiInput, nodes::render::{LayoutTree, layout_tree}};

/// The entity nodes laid out last, drawn under `UINodeSDFRoot` trees.
#[derive(Default, Resource)]
pub struct EntityUiElements {
    pub elements: Vec<SDFElement>,
    /// Counts layout passes, see `ComputedNode::pass`.
    pub pass: u64,
}

impl std::fmt::Debug for EntityUiElements {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityUiElements").field("elements", &self.elements.len()).field("pass", &self.pass).finish()
    }
}

/// A snapshot of an entity node and its node children, scaled to window pixels.
struct EntityNode {
    id: EntityID,
    style: Style,
    children: Vec<EntityNode>,
}

impl LayoutTree for EntityNode {
    fn style(&self) -> &Style { &self.style }
    fn children(&self) -> &[Self] { &self.children }
}

/// Snapshots every node tree in `world`, roots in spawn order.
fn snapshot(world: &World, styles: &mut AHashMap<EntityID, Style>, scale: f32) -> Vec<EntityNode> {
    fn build(world: &World, id: EntityID, styles: &mut AHashMap<EntityID, Style>, scale: f32) -> Option<EntityNode> {
        let style = styles.remove(&id)?.scaled(scale);
        let child_ids = world.hierarchy().children(id).map(|children| children.to_vec()).unwrap_or_default();
        let children = child_ids.into_iter().filter_map(|child| build(world, child, styles, scale)).collect();
        Some(EntityNode { id, style, children })
    }
    let mut roots: Vec<EntityID> = styles.keys()
        .copied()
        .filter(|id| world.hierarchy().parent(*id).is_none_or(|parent| !styles.contains_key(&parent)))
        .collect();
    roots.sort_unstable();
    roots.into_iter().filter_map(|id| build(world, id, styles, scale)).collect()
}

/// Lays every entity node out over the window, writing where each went to its `ComputedNode`.
pub(crate) fn layout_entity_ui(
    world: World,
    input: Res<UiInput>,
    window: Res<WindowDimensions>,
    mut out: ResMut<EntityUiElements>,
    mut nodes: Query<&'static Node>,
) {
    let mut styles = AHashMap::new();
    while let Ok(Some((id, node))) = nodes.next() { styles.insert(id, node.0.clone()); }
    drop(nodes);
    let roots = snapshot(&world, &mut styles, input.scale.max(f32::EPSILON));

    let screen = Vec2::new(window.x as f32, window.y as f32);
    let mut placed = vec![];
    out.elements = layout_tree(&roots, Vec2::ZERO, screen, [screen.x, screen.y], &mut |node: &EntityNode, laid_out| {
        let opaque = !matches!(node.style.background(), Background::Empty);
        placed.push((node.id, laid_out, opaque));
    });
    out.pass += 1;
    for (id, laid_out, opaque) in placed {
        if let Ok(Some(mut computed)) = world.handle(id).get::<&mut ComputedNode>() {
            *computed = ComputedNode { position: laid_out.top_left, size: laid_out.size, stack: laid_out.stack, pass: out.pass, opaque };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{AlignItems, Anchor, JustifyContent, Rect, UIInputTracker, UINode, UiBundle, Val, find_named, layout_ui_nodes, px, spawn_ui};

    fn styles(world: &World) -> AHashMap<EntityID, Style> {
        let mut styles = AHashMap::new();
        let mut nodes = Query::<&Node>::new(world);
        while let Ok(Some((id, node))) = nodes.next() { styles.insert(id, node.0.clone()); }
        styles
    }

    fn panel() -> Node {
        Node::row().size(px(300.0), px(100.0)).padding(Rect::single(px(5.0))).gap(px(4.0))
            .justify(JustifyContent::SpaceBetween).align(AlignItems::Center).anchored(Anchor::Center)
    }

    #[test]
    fn spawns_children_in_order() {
        let world = World::default();
        let root = spawn_ui(&world, UiBundle::new(Node::column()).children([
            UiBundle::new(Node::new()).named("a"),
            UiBundle::new(Node::new()).named("b").child(UiBundle::new(Node::new()).named("c")),
        ]));
        let (a, b, c) = (find_named(&world, root, "a").unwrap(), find_named(&world, root, "b").unwrap(), find_named(&world, root, "c").unwrap());
        assert_eq!(world.hierarchy().children(root).unwrap().to_vec(), vec![a, b]);
        assert_eq!(world.hierarchy().parent(c), Some(b));
        assert!(world.handle(c).get::<&ComputedNode>().unwrap().is_some());
        assert_eq!(find_named(&world, root, "d"), None);
    }

    #[test]
    fn entities_lay_out_like_node_trees() {
        let world = World::default();
        spawn_ui(&world, UiBundle::new(panel()).children([
            UiBundle::new(Node::new().size(px(40.0), px(20.0))),
            UiBundle::new(Node::new().width(px(60.0)).grow(1.0).height(Val::Percent(0.5))),
            UiBundle::new(Node::new().size(px(10.0), px(30.0))),
        ]));
        let roots = snapshot(&world, &mut styles(&world), 2.0);
        let entity = layout_tree(&roots, Vec2::ZERO, Vec2::new(800.0, 600.0), [800.0, 600.0], &mut |_, _| {});

        let mut tree = UINode::default();
        tree.set_style(panel().0.scaled(2.0));
        for style in [Node::new().size(px(40.0), px(20.0)), Node::new().width(px(60.0)).grow(1.0).height(Val::Percent(0.5)), Node::new().size(px(10.0), px(30.0))] {
            let mut child = UINode::default();
            child.set_style(style.0.scaled(2.0));
            tree.add(child);
        }
        let nodes = layout_ui_nodes(&World::default(), &UIInputTracker::default(), &[tree], [800.0, 600.0]);
        let rects = |elements: &[SDFElement]| elements[0].children.iter().map(|child| (child.center, child.dimensions)).collect::<Vec<_>>();
        assert_eq!(rects(&entity), rects(&nodes));
        assert_eq!(entity[0].dimensions, Vec2::new(600.0, 200.0));
    }

    #[test]
    fn records_where_nodes_went() {
        let world = World::default();
        let root = spawn_ui(&world, UiBundle::new(Node::new().size(px(100.0), px(50.0)).anchored(Anchor::BottomRight))
            .child(UiBundle::new(Node::new().size(px(10.0), px(10.0))).named("child")));
        let roots = snapshot(&world, &mut styles(&world), 1.0);
        let mut placed = vec![];
        layout_tree(&roots, Vec2::ZERO, Vec2::new(800.0, 600.0), [800.0, 600.0], &mut |node: &EntityNode, laid_out| placed.push((node.id, laid_out)));
        let child = find_named(&world, root, "child").unwrap();
        assert_eq!(placed.len(), 2);
        assert_eq!((placed[0].0, placed[0].1.top_left, placed[0].1.stack), (root, Vec2::new(700.0, 550.0), 0));
        assert_eq!((placed[1].0, placed[1].1.stack), (child, 1));
    }
}
