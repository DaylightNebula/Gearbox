//! Scrolls entity nodes with `Overflow::Scroll` or `Overflow::Auto` by the wheel and their
//! scrollbars.

use anarchy::{EntityID, Query, Res, ResMut, World};
use magician_vgpu::glam::Vec2;

use crate::ui::{ComputedNode, EntityUiElements, Node, ScrollDrag, Scrolled, TextInput, UiFocus, UiInput, input::SCROLL_PER_NOTCH};

/// Wheel scrolls the innermost node under the pointer that can still move that way, dragging a
/// thumb scrolls its node, and clicking a track pages toward the pointer. Offsets are written to
/// each node's `Style::scroll` and kept within what last frame's layout allowed. A focused
/// multiline `TextInput` under the pointer keeps the wheel for itself.
pub fn scroll_nodes(world: World, mut input: ResMut<UiInput>, layout: Res<EntityUiElements>, focus: Res<UiFocus>, mut nodes: Query<&'static ComputedNode>) {
    let input = &mut *input;
    if !input.down { input.scroll_drag = None; }
    let mut scrolling: Vec<(EntityID, u32, Scrolled, bool)> = vec![];
    while let Ok(Some((id, node))) = nodes.next() {
        if node.pass != layout.pass { continue }
        if let Some(scroll) = node.scroll { scrolling.push((id, node.stack, scroll, node.contains(input.cursor))); }
    }
    drop(nodes);

    let scale = input.scale.max(f32::EPSILON);
    let mut moved: Vec<(EntityID, Vec2)> = scrolling.iter().map(|(id, _, scroll, _)| (*id, scroll.offset)).collect();
    let mut set = |id: EntityID, axis: usize, offset: f32| {
        if let Some((_, at)) = moved.iter_mut().find(|(node, _)| *node == id) { at[axis] = offset; }
    };

    if input.pressed && let Some((id, axis)) = input.over_scrollbar
        && let Some((_, _, scroll, _)) = scrolling.iter().find(|(node, ..)| *node == id)
        && let Some(bar) = scroll.bars[axis] && let Some(thumb) = bar.thumb {
        let at = input.cursor[axis];
        if at >= thumb.min[axis] && at <= thumb.max[axis] {
            input.scroll_drag = Some(ScrollDrag { node: id, axis, grab: at - thumb.min[axis] });
        } else {
            // a page toward the pointer
            let page = scroll.view.size()[axis];
            let toward = if at < thumb.min[axis] { -page } else { page };
            set(id, axis, (scroll.offset[axis] + toward).clamp(0.0, scroll.max[axis]));
        }
    }
    if let Some(drag) = input.scroll_drag && let Some((_, _, scroll, _)) = scrolling.iter().find(|(node, ..)| *node == drag.node) {
        set(drag.node, drag.axis, scroll.offset_at(drag.axis, input.cursor[drag.axis] - drag.grab));
    }

    // Shift turns the wheel sideways
    let notches = if input.modifiers.shift_key() { Vec2::new(input.wheel + input.wheel_x, 0.0) } else { Vec2::new(input.wheel_x, input.wheel) };
    let field_scrolls = focus.focused.filter(|id| input.hovered == Some(*id))
        .is_some_and(|id| world.handle(id).get::<&TextInput>().ok().flatten().is_some_and(|field| field.multiline));
    if notches != Vec2::ZERO && !input.over_windows && !field_scrolls {
        let delta = -notches * SCROLL_PER_NOTCH * scale;
        let can_move = |scroll: &Scrolled| (0..2).any(|axis| {
            (delta[axis] < 0.0 && scroll.offset[axis] > 0.0) || (delta[axis] > 0.0 && scroll.offset[axis] < scroll.max[axis])
        });
        let target = scrolling.iter()
            .filter(|(_, _, scroll, under)| *under && can_move(scroll))
            .max_by_key(|(_, stack, ..)| *stack);
        if let Some((id, _, scroll, _)) = target {
            let to = (scroll.offset + delta).clamp(Vec2::ZERO, scroll.max);
            set(*id, 0, to.x);
            set(*id, 1, to.y);
        }
    }

    for (id, offset) in moved {
        let Ok(Some(mut node)) = world.handle(id).get::<&mut Node>() else { continue };
        let offset = offset / scale;
        if *node.style().scroll() != offset { node.style_mut().set_scroll(offset); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anarchy::{SingleThreadedExecutor, SystemGraph, SystemInstruction};
    use crate::ui::{ClipRect, ScrollbarRects, UiBundle};

    /// A node laid out at 0..100 square scrolling `max` down, scrolled `offset`, with a vertical bar
    /// whose thumb is 0..50 down the right edge at that offset.
    fn scrolling(world: &World, offset: f32, max: f32, stack: u32) -> EntityID {
        let id = world.spawn(UiBundle::new(Node::new()).into_entity_builder());
        let track = ClipRect::new(Vec2::new(90.0, 0.0), Vec2::new(100.0, 100.0));
        let start = 50.0 * offset / max;
        let thumb = ClipRect::new(Vec2::new(90.0, start), Vec2::new(100.0, start + 50.0));
        let scroll = Scrolled {
            view: ClipRect::new(Vec2::ZERO, Vec2::new(90.0, 100.0)),
            offset: Vec2::new(0.0, offset),
            max: Vec2::new(0.0, max),
            bars: [None, Some(ScrollbarRects { track, thumb: Some(thumb) })],
        };
        let computed = ComputedNode { size: Vec2::splat(100.0), stack, pass: 1, scroll: Some(scroll), ..Default::default() };
        *world.handle(id).get::<&mut ComputedNode>().unwrap().unwrap() = computed;
        id
    }

    /// Runs `scroll_nodes` with `input`, returning the drag it left.
    fn run(world: &World, input: UiInput) -> Option<ScrollDrag> {
        world.insert_resource(input);
        world.init_resource_with(|| EntityUiElements { pass: 1, ..Default::default() });
        world.init_resource_with(UiFocus::default);
        let mut graph = SystemGraph::new();
        graph.append_system(scroll_nodes, SystemInstruction::default(), std::iter::empty()).unwrap();
        SingleThreadedExecutor::new().run(world, &graph).unwrap();
        world.resource::<UiInput>().unwrap().scroll_drag
    }

    fn scroll(world: &World, id: EntityID) -> Vec2 { *world.handle(id).get::<&Node>().unwrap().unwrap().style().scroll() }

    #[test]
    fn wheel_scrolls_the_innermost_node_that_can_move() {
        let world = World::default();
        let outer = scrolling(&world, 0.0, 500.0, 0);
        let inner = scrolling(&world, 200.0, 200.0, 1);
        let input = UiInput { cursor: Vec2::splat(50.0), wheel: -1.0, scale: 2.0, ..Default::default() };
        // the inner node is at its end, so the outer one moves, in logical pixels
        run(&world, input);
        assert_eq!(scroll(&world, outer), Vec2::new(0.0, SCROLL_PER_NOTCH));
        assert_eq!(scroll(&world, inner), Vec2::new(0.0, 100.0));
    }

    #[test]
    fn thumbs_drag_and_tracks_page() {
        let world = World::default();
        let id = scrolling(&world, 0.0, 300.0, 0);
        let press = |id: EntityID, y: f32| UiInput { cursor: Vec2::new(95.0, y), pressed: true, down: true, scale: 1.0, over_scrollbar: Some((id, 1)), ..Default::default() };
        // grabbed 10px into the thumb, then dragged halfway down the free track
        let drag = run(&world, press(id, 10.0));
        assert_eq!(drag, Some(ScrollDrag { node: id, axis: 1, grab: 10.0 }));
        run(&world, UiInput { cursor: Vec2::new(95.0, 35.0), down: true, scale: 1.0, scroll_drag: drag, ..Default::default() });
        assert_eq!(scroll(&world, id), Vec2::new(0.0, 150.0));

        // a click under the thumb pages down by the view's height
        let world = World::default();
        let id = scrolling(&world, 0.0, 300.0, 0);
        assert_eq!(run(&world, press(id, 80.0)), None);
        assert_eq!(scroll(&world, id), Vec2::new(0.0, 100.0));
    }
}
