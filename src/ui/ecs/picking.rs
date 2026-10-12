//! Finds the entity node under the pointer.

use anarchy::{EntityID, Query, Res, ResMut, World};
use magician_vgpu::glam::Vec2;

use crate::ui::{ComputedNode, EntityUiElements, Interaction, UiInput};

/// The topmost of `targets` (id, node) shown in layout pass `pass` and holding `point`.
pub fn topmost(targets: impl IntoIterator<Item = (EntityID, ComputedNode)>, point: Vec2, pass: u64) -> Option<EntityID> {
    targets.into_iter()
        .filter(|(_, node)| node.pass == pass && node.contains(point))
        .max_by_key(|(_, node)| node.stack)
        .map(|(id, _)| id)
}

/// The `Interaction` of `id` given who's hovered and captured.
pub fn interaction_of(id: EntityID, hovered: Option<EntityID>, captured: Option<EntityID>, down: bool) -> Interaction {
    if down && captured == Some(id) { Interaction::Pressed }
    else if hovered == Some(id) { Interaction::Hovered }
    else { Interaction::None }
}

/// Hit tests the entity nodes with an `Interaction` against the pointer, using last frame's
/// layout. The topmost one under it is hovered, and the one a press lands on is captured until
/// the button is let go. Immediate mode windows under the pointer hide the nodes beneath them.
pub fn pick_ui(world: World, mut input: ResMut<UiInput>, layout: Res<EntityUiElements>, mut nodes: Query<&'static ComputedNode>) {
    let mut opaque = false;
    while let Ok(Some((_, node))) = nodes.next() {
        opaque |= node.opaque && node.pass == layout.pass && node.contains(input.cursor);
    }
    input.over_nodes = opaque && !input.over_windows;

    let mut targets = vec![];
    let mut query = Query::<(&ComputedNode, &mut Interaction)>::new(&world);
    while let Ok(Some((id, (node, _)))) = query.next() { targets.push((id, *node)); }
    drop(query);
    input.hovered = if input.over_windows { None } else { topmost(targets, input.cursor, layout.pass) };
    if input.pressed { input.captured = input.hovered; }

    let mut query = Query::<&mut Interaction>::new(&world);
    while let Ok(Some((id, mut interaction))) = query.next() {
        let now = interaction_of(id, input.hovered, input.captured, input.down);
        if *interaction != now { *interaction = now; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: f32, stack: u32, pass: u64) -> ComputedNode {
        ComputedNode { position: Vec2::new(x, 0.0), size: Vec2::splat(10.0), stack, pass, opaque: true }
    }

    #[test]
    fn picks_the_topmost_shown_node() {
        let targets = [(1, node(0.0, 1, 2)), (2, node(5.0, 3, 2)), (3, node(5.0, 9, 1))];
        assert_eq!(topmost(targets, Vec2::new(7.0, 5.0), 2), Some(2));
        assert_eq!(topmost(targets, Vec2::new(2.0, 5.0), 2), Some(1));
        assert_eq!(topmost(targets, Vec2::new(50.0, 5.0), 2), None);
    }

    #[test]
    fn captured_stays_pressed() {
        assert_eq!(interaction_of(1, Some(2), Some(1), true), Interaction::Pressed);
        assert_eq!(interaction_of(2, Some(2), Some(1), true), Interaction::Hovered);
        assert_eq!(interaction_of(1, Some(1), Some(1), false), Interaction::Hovered);
        assert_eq!(interaction_of(1, None, None, false), Interaction::None);
    }
}
