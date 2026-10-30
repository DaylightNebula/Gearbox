//! Finds the entity node under the pointer.

use anarchy::{EntityID, Event, Query, Res, ResMut, World};
use magician_vgpu::glam::Vec2;

use crate::ui::{ComputedNode, EntityUiElements, Interaction, Node, TextPlacement, TextSpan, UiInput};

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

/// The `TextSpan` with a `hover` the pointer is over, sent as an event when it changes.
#[derive(Clone, Debug, PartialEq)]
pub struct TextHover {
    /// The node whose text it's in.
    pub entity: EntityID,
    /// Index of the span in the node's `Text::spans`.
    pub span: usize,
    /// Byte offset of the character under the pointer.
    pub index: usize,
    /// The span's `hover`.
    pub text: String,
}

/// The span with a `hover` drawn under `point`, in the topmost of `targets` (id, stack,
/// placement, spans).
pub fn span_under<'a>(targets: impl IntoIterator<Item = (EntityID, u32, &'a TextPlacement, &'a [TextSpan])>, point: Vec2) -> Option<TextHover> {
    targets.into_iter()
        .filter(|(_, _, placement, _)| placement.shows(point))
        .filter_map(|(id, stack, placement, spans)| {
            let index = placement.layout.glyph_at(placement.local(point))?;
            let (span, found) = spans.iter().enumerate().rev().find(|(_, span)| span.hover.is_some() && span.range.contains(&index))?;
            Some((stack, TextHover { entity: id, span, index, text: found.hover.clone()? }))
        })
        .max_by_key(|(stack, _)| *stack)
        .map(|(_, hover)| hover)
}

/// Finds the hoverable span under the pointer in last frame's entity text, see `TextHover`.
pub(crate) fn hover_text_spans(world: World, mut input: ResMut<UiInput>, layout: Res<EntityUiElements>, hovered: Event<TextHover>) {
    let mut found = None;
    if !input.over_windows {
        let mut targets = vec![];
        for (id, placement) in &layout.texts {
            let Ok(Some(node)) = world.handle(*id).get::<&Node>() else { continue };
            let Some(text) = node.style().text() else { continue };
            if !text.spans.iter().any(|span| span.hover.is_some()) { continue }
            let Ok(Some(computed)) = world.handle(*id).get::<&ComputedNode>() else { continue };
            targets.push((*id, computed.stack, placement, text.spans.clone()));
        }
        found = span_under(targets.iter().map(|(id, stack, placement, spans)| (*id, *stack, *placement, spans.as_slice())), input.cursor);
    }
    let changed = match (&found, &input.hovered_span) {
        (Some(now), Some(before)) => (now.entity, now.span) != (before.entity, before.span),
        (now, before) => now.is_some() != before.is_some(),
    };
    if changed && let Some(now) = &found { hovered.send(now.clone()); }
    input.hovered_span = found;
}

#[cfg(test)]
mod tests {
    use super::*;
    use magician_vgpu::glam::Vec4;

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
    fn finds_the_hovered_span() {
        use std::sync::Arc;
        let font = crate::ui::SDFFont::new(include_bytes!("../../../examples/LiberationSans-Regular.ttf")).unwrap();
        let layout = Arc::new(font.layout("let value", 20.0, None));
        let placement = TextPlacement { layout: layout.clone(), origin: Vec2::new(10.0, 10.0), view_min: Vec2::ZERO, view_max: Vec2::splat(500.0) };
        let spans = [TextSpan::new(0..3).hover("keyword"), TextSpan::new(4..9).color(Vec4::ONE), TextSpan::new(4..6).hover("first")];
        let over = |index: usize| Vec2::new(10.0 + layout.lines[0].x_of(index) + 1.0, 15.0);
        let found = span_under([(1, 0, &placement, &spans[..])], over(1)).unwrap();
        assert_eq!((found.entity, found.span, found.index, found.text.as_str()), (1, 0, 1, "keyword"));
        assert_eq!(span_under([(1, 0, &placement, &spans[..])], over(5)).map(|found| found.span), Some(2));
        // the second span has no hover, the space none at all
        assert_eq!(span_under([(1, 0, &placement, &spans[..])], over(7)), None);
        assert_eq!(span_under([(1, 0, &placement, &spans[..])], over(3)), None);
        // the topmost text wins
        assert_eq!(span_under([(1, 0, &placement, &spans[..]), (2, 5, &placement, &spans[..])], over(1)).map(|found| found.entity), Some(2));
    }

    #[test]
    fn captured_stays_pressed() {
        assert_eq!(interaction_of(1, Some(2), Some(1), true), Interaction::Pressed);
        assert_eq!(interaction_of(2, Some(2), Some(1), true), Interaction::Hovered);
        assert_eq!(interaction_of(1, Some(1), Some(1), false), Interaction::Hovered);
        assert_eq!(interaction_of(1, None, None, false), Interaction::None);
    }
}
