//! Behavior-only widgets. Put one on any entity node with an `Interaction` and style the node
//! however you like: the widget updates its own value as it's used and sends events. The
//! themed widgets in `themed` are built from these.

use anarchy::{Component, EntityID, Event, Query, Res, ResMut, SystemPin, World, after, and, before, pin};
use crate::{App, Plugin};
use serde::{Deserialize, Serialize};

use magician_vgpu::glam::{Vec2, Vec4};

use crate::{RenderDeltaTime, ui::{
    ComputedNode, Display, EditContext, EntityUiElements, KeyOutcome, Node, PositionType, Rect, TextCursor, TextEditor, UiClipboard, UiFocus,
    UiInput, Val, caret_shows, caret_width, field_shows, follow_caret, layout_entity_ui, pick_ui, remap_spans, text_change,
}};

/// Wheel notches scroll this many rows.
const ROWS_PER_NOTCH: f32 = 2.0;

/// Sent when a `Button` is clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Activate { pub entity: EntityID }

/// Sent when a widget's value changes through the UI, `entity` being the widget.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueChange<T> { pub entity: EntityID, pub value: T }

/// Sent when a `TextInput` hands back its text, on Enter or a click elsewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submit { pub entity: EntityID, pub text: String }

/// Clicked by pressing it, then letting go over it. Sends `Activate`.
#[derive(Clone, Copy, Debug, Default, Component)]
pub struct Button { pub disabled: bool }

/// Flips when clicked. Sends `ValueChange<bool>`.
#[derive(Clone, Copy, Debug, Default, Component)]
pub struct Checkbox { pub checked: bool, pub disabled: bool }

impl Checkbox {
    pub fn new(checked: bool) -> Self { Self { checked, disabled: false } }
}

/// How a `Slider` spreads its range along its node.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliderScale {
    #[default]
    Linear,
    Log,
}

/// Picks a value in `min..=max` from where it's pressed and dragged across its node. Sends
/// `ValueChange<f64>`.
#[derive(Clone, Copy, Debug, Component)]
pub struct Slider {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
    pub scale: SliderScale,
    pub disabled: bool,
}

impl Slider {
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Self { value, min, max, step: None, scale: SliderScale::Linear, disabled: false }
    }

    /// A logarithmic slider, see `SliderScale::Log`.
    pub fn log(min: f64, max: f64, value: f64) -> Self { Self { scale: SliderScale::Log, ..Self::new(min, max, value) } }

    pub fn step(mut self, step: f64) -> Self { self.step = Some(step); self }

    /// How far along the slider `value` is, from 0 to 1.
    pub fn fraction(&self) -> f64 {
        let (lo, hi, v) = match self.scale {
            SliderScale::Linear => (self.min, self.max, self.value),
            SliderScale::Log => (self.min.ln(), self.max.ln(), self.value.ln()),
        };
        if hi == lo { 0.0 } else { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) }
    }

    /// The value `t` (0 to 1) along the slider.
    pub fn value_at(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let value = match self.scale {
            SliderScale::Linear => self.min + (self.max - self.min) * t,
            SliderScale::Log => (self.min.ln() + (self.max.ln() - self.min.ln()) * t).exp(),
        };
        match self.step {
            Some(step) if step > 0.0 => ((value / step).round() * step).clamp(self.min, self.max),
            _ => value,
        }
    }
}

/// Holds which of its `RadioButton` descendants is selected. Sends `ValueChange<usize>`.
#[derive(Clone, Copy, Debug, Default, Component)]
pub struct RadioGroup { pub selected: Option<usize>, pub disabled: bool }

/// Selects `index` in the nearest `RadioGroup` above it when clicked.
#[derive(Clone, Copy, Debug, Component)]
pub struct RadioButton { pub index: usize }

/// Takes typed text after it's clicked. Sends `ValueChange<String>` as it's edited and
/// `Submit` when Enter (Command or Control with Enter if `multiline`) or a click elsewhere hands
/// the text back.
///
/// It shows itself through its node's `Text`, which the bundle spawns it with: each frame its
/// content, caret, selection and scroll are written there, while the font, size, color, wrap
/// and `spans` stay the bundle's. Spans follow the text as it's edited, so set them again on
/// `ValueChange` to recolor it. Clicks place the caret, double and triple clicks select words
/// and lines, and dragging selects.
#[derive(Clone, Debug, Default, Component)]
pub struct TextInput {
    pub text: String,
    pub hint: String,
    pub replace_on_focus: bool,
    pub disabled: bool,
    /// Enter types newlines and Tab tabs, and the wheel scrolls it.
    pub multiline: bool,
    /// The caret, selection and undo history.
    pub editor: TextEditor,
    /// How far the text is scrolled, in logical pixels.
    pub scroll: Vec2,
    /// Colors of the caret, the selection and the hint, from the text's color if `None`.
    pub caret_color: Option<Vec4>,
    pub selection_color: Option<Vec4>,
    pub hint_color: Option<Vec4>,
    draft: Option<String>,
    /// Seconds since the caret last moved, for blinking.
    blink: f32,
    /// The caret moved, so the scroll should follow it.
    follow: bool,
    /// The node's text color while the hint's color replaces it.
    text_color: Option<Vec4>,
}

impl TextInput {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self { editor: TextEditor::at_end(&text), text, ..Default::default() }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self { self.hint = hint.into(); self }

    pub fn replace_on_focus(mut self) -> Self { self.replace_on_focus = true; self }

    pub fn multiline(mut self) -> Self { self.multiline = true; self }

    /// The text being edited: the draft while one is open, otherwise `text`.
    pub fn editing(&self) -> &str { self.draft.as_deref().unwrap_or(&self.text) }

    /// Whether a `replace_on_focus` draft is open.
    pub fn has_draft(&self) -> bool { self.draft.is_some() }

    /// Whether it shows its hint or replaced text rather than what's typed, given whether it
    /// has the keyboard.
    pub fn shows_placeholder(&self, focused: bool) -> bool {
        field_shows(&self.text, self.draft.as_deref(), &self.hint, focused).1
    }

    /// Restarts the caret's blink and scrolls to it.
    fn moved(&mut self) {
        self.blink = 0.0;
        self.follow = true;
    }
}

/// Shows its second child while open, clicking its first child opens or closes it. The header
/// child needs an `Interaction`. Sends `ValueChange<bool>`.
#[derive(Clone, Copy, Debug, Default, Component)]
pub struct Collapsible { pub open: bool }

/// Shows `visible` of the `ScrollItem`s below it at a time, starting at row `first`, scrolled
/// by the wheel while the pointer is over it. Rows inside hidden nodes (a closed `Collapsible`)
/// don't count.
#[derive(Clone, Debug, Default, Component)]
pub struct ScrollRows {
    /// First shown row, fractional between wheel notches.
    pub first: f32,
    pub visible: usize,
    /// Rows below it, counted each frame.
    pub rows: usize,
    /// Nodes it hid last frame.
    hidden: Vec<EntityID>,
}

impl ScrollRows {
    pub fn new(visible: usize) -> Self { Self { visible, ..Default::default() } }
}

/// A row of the `ScrollRows` above it.
#[derive(Clone, Copy, Debug, Default, Component)]
pub struct ScrollItem;

/// Dragging it moves `target`, or its parent if `None`, with the pointer. The moved node must
/// be absolutely positioned.
#[derive(Clone, Copy, Debug, Default, Component)]
pub struct DragHandle { pub target: Option<EntityID> }

/// Text shown next to the pointer while the node is hovered, by themed widgets.
#[derive(Clone, Debug, Component)]
pub struct Tooltip(pub String);

/// Runs the behavior-only widgets, in the `Start` pin after picking, so systems in the `Normal`
/// pin see this frame's values and events. `TextInput`s also write what they show into their
/// `Text` in the `End` pin. Added by `WidgetPlugin`, or on its own to style
/// every widget yourself.
pub struct UiWidgetsPlugin;

impl Plugin for UiWidgetsPlugin {
    fn build(self, app: App) -> App {
        let start = || and(pin(SystemPin::Start), after(pick_ui));
        app.on_render_update_with(press_buttons, start())
            .on_render_update_with(flip_checkboxes, start())
            .on_render_update_with(drag_sliders, start())
            .on_render_update_with(select_radios, start())
            .on_render_update_with(edit_text_inputs, start())
            .on_render_update_with(unhide_scrolled, and(start(), before(toggle_collapsibles)))
            .on_render_update_with(toggle_collapsibles, start())
            .on_render_update_with(scroll_rows, and(start(), after(toggle_collapsibles)))
            .on_render_update_with(drag_handles, start())
            .on_render_update_with(show_text_inputs, and(pin(SystemPin::End), before(layout_entity_ui)))
    }
}

fn press_buttons(input: Res<UiInput>, mut buttons: Query<&'static Button>, sent: Event<Activate>) {
    while let Ok(Some((id, button))) = buttons.next() {
        if !button.disabled && input.activated(id) { sent.send(Activate { entity: id }); }
    }
}

fn flip_checkboxes(input: Res<UiInput>, mut checkboxes: Query<&'static mut Checkbox>, sent: Event<ValueChange<bool>>) {
    while let Ok(Some((id, mut checkbox))) = checkboxes.next() {
        if !checkbox.disabled && input.activated(id) {
            checkbox.checked = !checkbox.checked;
            sent.send(ValueChange { entity: id, value: checkbox.checked });
        }
    }
}

fn drag_sliders(input: Res<UiInput>, mut sliders: Query<(&'static mut Slider, &'static ComputedNode)>, sent: Event<ValueChange<f64>>) {
    while let Ok(Some((id, (mut slider, node)))) = sliders.next() {
        if slider.disabled || !input.down || input.captured != Some(id) || node.size.x <= 0.0 { continue }
        let value = slider.value_at(((input.cursor.x - node.position.x) / node.size.x) as f64);
        if value != slider.value {
            slider.value = value;
            sent.send(ValueChange { entity: id, value });
        }
    }
}

fn select_radios(world: World, input: Res<UiInput>, mut buttons: Query<&'static RadioButton>, sent: Event<ValueChange<usize>>) {
    let mut clicked = vec![];
    while let Ok(Some((id, button))) = buttons.next() {
        if input.activated(id) { clicked.push((id, button.index)); }
    }
    drop(buttons);
    for (id, index) in clicked {
        for group_id in world.hierarchy().ancestors(id) {
            let Ok(Some(mut group)) = world.handle(group_id).get::<&mut RadioGroup>() else { continue };
            if !group.disabled && group.selected != Some(index) {
                group.selected = Some(index);
                sent.send(ValueChange { entity: group_id, value: index });
            }
            break
        }
    }
}

/// Hands `input`'s text back, sending what changed.
fn commit(id: EntityID, input: &mut TextInput, changed: &Event<ValueChange<String>>, submitted: &Event<Submit>) {
    // an empty draft keeps the text it replaced
    if let Some(draft) = input.draft.take().filter(|draft| !draft.is_empty()) {
        input.text = draft;
        changed.send(ValueChange { entity: id, value: input.text.clone() });
    }
    input.editor.clamp(&input.text);
    submitted.send(Submit { entity: id, text: input.text.clone() });
}

/// The index under the pointer in `id`'s text as drawn last frame, if that was `text`.
fn index_under(texts: &EntityUiElements, id: EntityID, text: &str, cursor: Vec2) -> Option<usize> {
    let placement = texts.texts.get(&id).filter(|placement| placement.layout.len == text.len())?;
    Some(placement.layout.index_at(placement.local(cursor)))
}

fn edit_text_inputs(
    world: World,
    input: Res<UiInput>,
    texts: Res<EntityUiElements>,
    clipboard: Res<UiClipboard>,
    mut focus: ResMut<UiFocus>,
    changed: Event<ValueChange<String>>,
    submitted: Event<Submit>,
) {
    let text_input = |id: EntityID| world.handle(id).get::<&mut TextInput>().ok().flatten();
    if input.pressed {
        let clicked = input.hovered.filter(|id| text_input(*id).is_some_and(|field| !field.disabled));
        if clicked != focus.focused {
            if let Some(old) = focus.focused.take() && let Some(mut field) = text_input(old) {
                commit(old, &mut field, &changed, &submitted);
            }
            if let Some(new) = clicked && let Some(mut field) = text_input(new) {
                if field.replace_on_focus { field.draft = Some(String::new()); }
                let at = field.editing().len();
                field.editor.set_caret(at, false);
                field.moved();
                focus.focused = Some(new);
            }
        }
    }
    let Some(id) = focus.focused else { return };
    let Some(mut field) = text_input(id) else { focus.focused = None; return };
    let field = &mut *field;

    // the pointer places the caret and drags out selections
    if input.captured == Some(id) && input.down {
        let editing = field.editing().to_string();
        if let Some(at) = index_under(&texts, id, &editing, input.cursor) {
            if input.pressed { field.editor.click(&editing, at, input.modifiers.shift_key(), input.clicks); } else { field.editor.drag(&editing, at); }
            field.moved();
        }
    }
    if field.multiline && input.hovered == Some(id) && input.wheel != 0.0 && let Some(placement) = texts.texts.get(&id) {
        field.scroll.y -= input.wheel * placement.layout.line_height / input.scale.max(f32::EPSILON) * ROWS_PER_NOTCH;
    }

    let layout = texts.texts.get(&id).map(|placement| placement.layout.clone());
    let multiline = field.multiline;
    let mut edits = vec![];
    for (key, typed) in &input.keys {
        let context = EditContext { modifiers: input.modifiers, multiline, layout: layout.as_deref(), clipboard: &clipboard };
        // drafts aren't the text spans belong to
        let drafting = field.draft.is_some();
        let editor = &mut field.editor;
        let text = field.draft.as_mut().unwrap_or(&mut field.text);
        let before = (!drafting).then(|| text.clone());
        let outcome = editor.apply_key(text, key, typed.as_deref(), context);
        field.blink = 0.0;
        field.follow = true;
        match outcome {
            KeyOutcome::Edited => if let Some(before) = before {
                edits.extend(text_change(&before, &field.text));
                changed.send(ValueChange { entity: id, value: field.text.clone() });
            },
            KeyOutcome::Unchanged => {}
            KeyOutcome::Submit => {
                commit(id, field, &changed, &submitted);
                focus.focused = None;
                break
            }
            KeyOutcome::Cancel => {
                field.draft = None;
                field.editor.clamp(&field.text);
                focus.focused = None;
                break
            }
        }
    }
    if !edits.is_empty() && let Ok(Some(mut node)) = world.handle(id).get::<&mut Node>() && let Some(text) = node.style_mut().text_mut() {
        for (replaced, inserted) in edits { remap_spans(&mut text.spans, replaced, inserted); }
    }
}

/// Writes each `TextInput`'s text, caret, selection and scroll into its node's `Text`, before
/// entity nodes are laid out.
pub(crate) fn show_text_inputs(
    focus: Res<UiFocus>,
    delta: Res<RenderDeltaTime>,
    input: Res<UiInput>,
    texts: Res<EntityUiElements>,
    mut fields: Query<(&'static mut TextInput, &'static mut Node)>,
) {
    while let Ok(Some((id, (mut field, mut node)))) = fields.next() {
        let field = &mut *field;
        let Some(text) = node.style_mut().text_mut() else { continue };
        let focused = focus.focused == Some(id);
        let (shown, placeholder) = field_shows(&field.text, field.draft.as_deref(), &field.hint, focused);
        if text.content != shown { text.content = shown.to_string(); }

        // the hint's color stands in for the text's while it shows
        if placeholder {
            let color = *field.text_color.get_or_insert(text.color);
            text.color = field.hint_color.unwrap_or(color * Vec4::new(1.0, 1.0, 1.0, 0.5));
        } else if let Some(color) = field.text_color.take() {
            text.color = color;
        }
        let color = field.text_color.unwrap_or(text.color);

        field.blink += delta.0;
        text.clip = true;
        text.cursor = focused.then(|| TextCursor {
            caret: if placeholder { 0 } else { field.editor.caret },
            selection: field.editor.selection().filter(|_| !placeholder),
            show_caret: caret_shows(field.blink),
            caret_color: field.caret_color.unwrap_or(color),
            selection_color: field.selection_color.unwrap_or(color * Vec4::new(1.0, 1.0, 1.0, 0.3)),
        });

        // scroll in logical pixels, in a view sized like last frame's
        let Some(placement) = texts.texts.get(&id) else { continue };
        let scale = input.scale.max(f32::EPSILON);
        let view = (placement.view_max - placement.view_min) / scale - Vec2::new(2.0 * caret_width(placement.layout.font_size) / scale, 0.0);
        let follow = std::mem::take(&mut field.follow) && !placeholder;
        field.scroll = if !focused && !field.multiline {
            Vec2::ZERO
        } else if follow {
            // what shows now, which the caret moved in
            let mut layout = text.font.layout(&text.content, text.font_size, text.wrap.then_some(view.x));
            layout.align(text.horizontal_align, view.x);
            follow_caret(&layout, field.editor.caret, view, field.scroll)
        } else {
            // last frame's layout is as big as this frame's while nothing's typed
            let size = placement.layout.size() / scale;
            let room = (size + Vec2::new(caret_width(text.font_size), 0.0) - view).max(Vec2::ZERO);
            field.scroll.clamp(Vec2::ZERO, room)
        };
        if text.scroll != field.scroll { text.scroll = field.scroll; }
    }
}

/// Sets `id`'s node to `display` if it isn't already.
fn set_display(world: &World, id: EntityID, display: Display) {
    if let Ok(Some(mut node)) = world.handle(id).get::<&mut Node>() && *node.style().display() != display {
        node.style_mut().set_display(display);
    }
}

fn toggle_collapsibles(world: World, input: Res<UiInput>, mut collapsibles: Query<&'static mut Collapsible>, sent: Event<ValueChange<bool>>) {
    while let Ok(Some((id, mut collapsible))) = collapsibles.next() {
        let children = world.hierarchy().children(id).map(|children| children.to_vec()).unwrap_or_default();
        let [header, content, ..] = children[..] else { continue };
        if input.activated(header) {
            collapsible.open = !collapsible.open;
            sent.send(ValueChange { entity: id, value: collapsible.open });
        }
        set_display(&world, content, if collapsible.open { Display::Flex } else { Display::None });
    }
}

/// Shows what `scroll_rows` hid last frame, before collapsibles decide what's open.
fn unhide_scrolled(world: World, mut lists: Query<&'static mut ScrollRows>) {
    while let Ok(Some((_, mut list))) = lists.next() {
        for id in std::mem::take(&mut list.hidden) { set_display(&world, id, Display::Flex); }
    }
}

/// The rows under `id` in order, and its shown containers with the rows each holds.
fn rows_under(world: &World, id: EntityID, rows: &mut Vec<EntityID>, containers: &mut Vec<(EntityID, std::ops::Range<usize>)>) {
    for child in world.hierarchy().children(id).map(|children| children.to_vec()).unwrap_or_default() {
        if world.handle(child).get::<&ScrollItem>().ok().flatten().is_some() {
            rows.push(child);
            continue
        }
        let shown = world.handle(child).get::<&Node>().ok().flatten().is_some_and(|node| *node.style().display() != Display::None);
        if !shown { continue }
        let start = rows.len();
        rows_under(world, child, rows, containers);
        if rows.len() > start { containers.push((child, start..rows.len())); }
    }
}

fn scroll_rows(world: World, input: Res<UiInput>, mut lists: Query<(&'static mut ScrollRows, &'static ComputedNode)>) {
    while let Ok(Some((id, (mut list, node)))) = lists.next() {
        let (mut rows, mut containers) = (vec![], vec![]);
        rows_under(&world, id, &mut rows, &mut containers);
        list.rows = rows.len();
        let wheel = if node.contains(input.cursor) { input.wheel } else { 0.0 };
        let last = rows.len().saturating_sub(list.visible) as f32;
        list.first = (list.first - wheel * ROWS_PER_NOTCH).clamp(0.0, last);
        let shown = list.first as usize..list.first as usize + list.visible;
        let mut hidden = vec![];
        for (idx, row) in rows.iter().enumerate() {
            if !shown.contains(&idx) { hidden.push(*row); }
        }
        // containers whose rows are all scrolled away take no room either
        for (container, range) in containers {
            if range.end <= shown.start || range.start >= shown.end { hidden.push(container); }
        }
        for row in &hidden { set_display(&world, *row, Display::None); }
        list.hidden = hidden;
    }
}

fn drag_handles(world: World, input: Res<UiInput>, mut handles: Query<&'static DragHandle>) {
    if !input.down || input.delta == Default::default() { return }
    let mut dragged = vec![];
    while let Ok(Some((id, handle))) = handles.next() {
        if input.captured == Some(id) && let Some(target) = handle.target.or_else(|| world.hierarchy().parent(id)) { dragged.push(target); }
    }
    drop(handles);
    let delta = input.delta / input.scale.max(f32::EPSILON);
    for target in dragged {
        let Ok(Some(mut node)) = world.handle(target).get::<&mut Node>() else { continue };
        if let PositionType::Absolute(edges) = *node.style().position_type() {
            let moved = |val: Val, by: f32| if let Val::Px(px) = val { Val::Px(px + by) } else { val };
            let edges = Rect::new(moved(*edges.top(), delta.y), *edges.bottom(), moved(*edges.left(), delta.x), *edges.right());
            node.style_mut().set_position_type(PositionType::Absolute(edges));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_values() {
        let linear = Slider::new(0.0, 10.0, 5.0);
        assert_eq!(linear.fraction(), 0.5);
        assert_eq!(linear.value_at(0.25), 2.5);
        assert_eq!(linear.value_at(2.0), 10.0);
        assert_eq!(linear.step(2.0).value_at(0.25), 2.0);

        let log = Slider::log(1.0, 100.0, 10.0);
        assert!((log.fraction() - 0.5).abs() < 1e-9);
        assert!((log.value_at(0.5) - 10.0).abs() < 1e-9);
        assert_eq!(Slider::new(3.0, 3.0, 3.0).fraction(), 0.0);
    }

    #[test]
    fn text_input_drafts() {
        let mut field = TextInput::new("7").replace_on_focus();
        field.draft = Some(String::new());
        assert_eq!(field.editing(), "");
        field.draft.as_mut().unwrap().push('9');
        assert_eq!(field.editing(), "9");
        assert_eq!(field.text, "7");
    }

    use anarchy::{Resource, SingleThreadedExecutor, SystemGraph, SystemInstruction};
    use magician_vgpu::glam::{Vec2, Vec4};
    use winit::keyboard::{Key, NamedKey};

    use crate::ui::{UiBundle};

    /// Events the systems under test sent.
    #[derive(Debug, Default, Resource)]
    struct Seen { activated: Vec<EntityID>, submitted: Vec<String>, selected: Vec<usize> }

    fn collect(mut seen: anarchy::ResMut<Seen>, activated: Event<Activate>, submitted: Event<Submit>, selected: Event<ValueChange<usize>>) {
        seen.activated.extend(activated.read().map(|event| event.entity));
        seen.submitted.extend(submitted.read().map(|event| event.text));
        seen.selected.extend(selected.read().map(|event| event.value));
    }

    /// Runs `system` then `collect` once over `world`.
    fn run<M: 'static>(world: &World, system: impl anarchy::IntoSystem<(), (), M> + Copy + 'static) {
        world.init_resource_with(Seen::default);
        world.init_resource_with(UiFocus::default);
        world.init_resource_with(EntityUiElements::default);
        world.init_resource_with(UiClipboard::default);
        world.init_resource_with(RenderDeltaTime::default);
        world.init_resource_with(UiInput::default);
        let mut graph = SystemGraph::new();
        graph.append_system(system, SystemInstruction::default(), std::iter::empty()).unwrap();
        graph.append_system(collect, after(system), std::iter::empty()).unwrap();
        SingleThreadedExecutor::new().run(world, &graph).unwrap();
    }

    /// Input for a click on `id` that lets go this frame.
    fn click(id: EntityID) -> UiInput {
        UiInput { released: true, hovered: Some(id), captured: Some(id), scale: 1.0, ..Default::default() }
    }

    fn node(world: &World, bundle: UiBundle) -> EntityID { world.spawn(bundle.interactive().into_entity_builder()) }

    #[test]
    fn buttons_and_checkboxes_click() {
        let world = World::default();
        let button = node(&world, UiBundle::new(Node::new()).with(Button::default()));
        let disabled = node(&world, UiBundle::new(Node::new()).with(Button { disabled: true }));
        world.insert_resource(click(button));
        run(&world, press_buttons);
        world.insert_resource(click(disabled));
        run(&world, press_buttons);
        assert_eq!(world.resource::<Seen>().unwrap().activated, vec![button]);

        let checkbox = node(&world, UiBundle::new(Node::new()).with(Checkbox::new(false)));
        world.insert_resource(click(checkbox));
        run(&world, flip_checkboxes);
        assert!(world.handle(checkbox).get::<&Checkbox>().unwrap().unwrap().checked);
        // a press let go somewhere else isn't a click
        world.insert_resource(UiInput { hovered: None, ..click(checkbox) });
        run(&world, flip_checkboxes);
        assert!(world.handle(checkbox).get::<&Checkbox>().unwrap().unwrap().checked);
    }

    #[test]
    fn radio_buttons_select_in_their_group() {
        let world = World::default();
        let options = (0..3).map(|index| UiBundle::new(Node::new()).with(RadioButton { index }).interactive());
        let group = world.spawn(UiBundle::new(Node::new()).with(RadioGroup::default()).child(UiBundle::new(Node::row()).children(options)).into_entity_builder());
        let row = world.hierarchy().children(group).unwrap()[0];
        let third = world.hierarchy().children(row).unwrap()[2];
        world.insert_resource(click(third));
        run(&world, select_radios);
        assert_eq!(world.handle(group).get::<&RadioGroup>().unwrap().unwrap().selected, Some(2));
        assert_eq!(world.resource::<Seen>().unwrap().selected, vec![2]);
    }

    #[test]
    fn text_inputs_take_typing() {
        let world = World::default();
        let field = node(&world, UiBundle::new(Node::new()).with(TextInput::new("ab")));
        world.insert_resource(UiInput { pressed: true, down: true, hovered: Some(field), ..Default::default() });
        run(&world, edit_text_inputs);
        assert_eq!(world.resource::<UiFocus>().unwrap().focused, Some(field));

        let keys = vec![(Key::Character("c".into()), Some("c".to_string())), (Key::Named(NamedKey::Enter), Some("\r".to_string()))];
        world.insert_resource(UiInput { keys, ..Default::default() });
        run(&world, edit_text_inputs);
        assert_eq!(world.handle(field).get::<&TextInput>().unwrap().unwrap().text, "abc");
        assert_eq!(world.resource::<Seen>().unwrap().submitted, vec!["abc".to_string()]);
        assert_eq!(world.resource::<UiFocus>().unwrap().focused, None);
    }

    #[test]
    fn drafts_only_replace_when_not_empty() {
        let world = World::default();
        let field = node(&world, UiBundle::new(Node::new()).with(TextInput::new("7").replace_on_focus()));
        let other = node(&world, UiBundle::new(Node::new()));
        world.insert_resource(UiInput { pressed: true, down: true, hovered: Some(field), ..Default::default() });
        run(&world, edit_text_inputs);
        assert!(world.handle(field).get::<&TextInput>().unwrap().unwrap().has_draft());
        // clicking elsewhere with an empty draft keeps the text
        world.insert_resource(UiInput { pressed: true, down: true, hovered: Some(other), ..Default::default() });
        run(&world, edit_text_inputs);
        let text = world.handle(field).get::<&TextInput>().unwrap().unwrap().text.clone();
        assert_eq!(text, "7");
        assert_eq!(world.resource::<Seen>().unwrap().submitted, vec!["7".to_string()]);
    }

    fn font() -> std::sync::Arc<crate::ui::SDFFont> {
        std::sync::Arc::new(crate::ui::SDFFont::new(include_bytes!("../../../examples/LiberationSans-Regular.ttf")).unwrap())
    }

    /// Records `id`'s text as drawn at `origin`, as if laid out last frame.
    fn place(world: &World, id: EntityID, text: &str, origin: Vec2) -> std::sync::Arc<crate::ui::TextLayout> {
        let layout = std::sync::Arc::new(font().layout(text, 20.0, None));
        let placement = crate::ui::TextPlacement { layout: layout.clone(), origin, view_min: origin, view_max: origin + Vec2::new(300.0, 30.0) };
        world.resource_mut::<EntityUiElements>().unwrap().texts.insert(id, placement);
        layout
    }

    #[test]
    fn clicks_place_the_caret_and_drags_select() {
        let world = World::default();
        let field = node(&world, UiBundle::new(Node::new()).with(TextInput::new("hello world")));
        run(&world, edit_text_inputs);
        let layout = place(&world, field, "hello world", Vec2::new(10.0, 10.0));
        let at = |index: usize| Vec2::new(10.0 + layout.caret_at(index).x + 0.5, 15.0);

        world.insert_resource(UiInput { pressed: true, down: true, clicks: 1, hovered: Some(field), captured: Some(field), cursor: at(2), ..Default::default() });
        run(&world, edit_text_inputs);
        let caret = |world: &World| world.handle(field).get::<&TextInput>().unwrap().unwrap().editor.clone();
        assert_eq!(caret(&world).caret, 2);
        world.insert_resource(UiInput { down: true, hovered: Some(field), captured: Some(field), cursor: at(8), ..Default::default() });
        run(&world, edit_text_inputs);
        assert_eq!(caret(&world).selection(), Some(2..8));
        // a double click selects the word
        world.insert_resource(UiInput { pressed: true, down: true, clicks: 2, hovered: Some(field), captured: Some(field), cursor: at(7), ..Default::default() });
        run(&world, edit_text_inputs);
        assert_eq!(caret(&world).selection(), Some(6..11));
    }

    #[test]
    fn spans_follow_typing() {
        use crate::ui::{Text, TextSpan};
        let world = World::default();
        let text = Text::new(font(), "", Vec4::ONE, 20.0).with_span(TextSpan::new(0..2).color(Vec4::X)).with_span(TextSpan::new(3..5).wavy(Vec4::X));
        let field = node(&world, UiBundle::new(Node::new().text(text)).with(TextInput::new("ab cd")));
        world.insert_resource(UiInput { pressed: true, down: true, hovered: Some(field), ..Default::default() });
        run(&world, edit_text_inputs);
        // the caret starts at the end; go home and type before both spans
        let keys = vec![(Key::Named(NamedKey::Home), None), (Key::Character("x".into()), Some("x".to_string()))];
        world.insert_resource(UiInput { keys, ..Default::default() });
        run(&world, edit_text_inputs);
        let node = world.handle(field).get::<&Node>().unwrap().unwrap().clone();
        let spans = &node.style().text().as_ref().unwrap().spans;
        assert_eq!(spans.iter().map(|span| span.range.clone()).collect::<Vec<_>>(), vec![1..3, 4..6]);
    }

    #[test]
    fn inputs_show_through_their_text() {
        use crate::ui::Text;
        let world = World::default();
        let field = node(&world, UiBundle::new(Node::new().text(Text::new(font(), "", Vec4::ONE, 20.0))).with(TextInput::default().hint("search")));
        run(&world, show_text_inputs);
        let text = |world: &World| world.handle(field).get::<&Node>().unwrap().unwrap().style().text().clone().unwrap();
        let shown = text(&world);
        assert_eq!((shown.content.as_str(), shown.color.w, shown.cursor.is_none(), shown.clip), ("search", 0.5, true, true));

        world.resource_mut::<UiFocus>().unwrap().focused = Some(field);
        world.handle(field).get::<&mut TextInput>().unwrap().unwrap().text = "abc".into();
        world.handle(field).get::<&mut TextInput>().unwrap().unwrap().editor.select_all("abc");
        run(&world, show_text_inputs);
        let shown = text(&world);
        assert_eq!((shown.content.as_str(), shown.color), ("abc", Vec4::ONE));
        let cursor = shown.cursor.unwrap();
        assert_eq!((cursor.caret, cursor.selection, cursor.show_caret), (3, Some(0..3), true));
    }

    #[test]
    fn collapsibles_show_content_while_open() {
        let world = World::default();
        let collapsible = world.spawn(
            UiBundle::new(Node::column()).with(Collapsible { open: false })
                .children([UiBundle::new(Node::new()).interactive(), UiBundle::new(Node::new())])
                .into_entity_builder()
        );
        let [header, content] = world.hierarchy().children(collapsible).unwrap()[..] else { panic!() };
        let display = |world: &World| *world.handle(content).get::<&Node>().unwrap().unwrap().style().display();
        world.insert_resource(UiInput::default());
        run(&world, toggle_collapsibles);
        assert_eq!(display(&world), Display::None);
        world.insert_resource(click(header));
        run(&world, toggle_collapsibles);
        assert_eq!(display(&world), Display::Flex);
        assert!(world.handle(collapsible).get::<&Collapsible>().unwrap().unwrap().open);
    }

    #[test]
    fn drag_handles_move_their_parent() {
        let world = World::default();
        let window = world.spawn(
            UiBundle::new(Node::new().absolute(Rect::new_top_left(Val::Px(10.0), Val::Px(20.0))))
                .child(UiBundle::new(Node::new()).interactive().with(DragHandle::default()))
                .into_entity_builder()
        );
        let handle = world.hierarchy().children(window).unwrap()[0];
        world.insert_resource(UiInput { down: true, captured: Some(handle), delta: magician_vgpu::glam::Vec2::new(6.0, 4.0), scale: 2.0, ..Default::default() });
        run(&world, drag_handles);
        let position = *world.handle(window).get::<&Node>().unwrap().unwrap().style().position_type();
        let PositionType::Absolute(edges) = position else { panic!() };
        assert_eq!((*edges.top(), *edges.left()), (Val::Px(12.0), Val::Px(23.0)));
    }

    #[test]
    fn scrolling_hides_rows_and_empty_containers() {
        let world = World::default();
        let row = || UiBundle::new(Node::new()).with(ScrollItem);
        let list = world.spawn(
            UiBundle::new(Node::column()).with(ScrollRows::new(2)).children([
                UiBundle::new(Node::column()).children([row(), row()]),
                UiBundle::new(Node::column()).children([row(), row()]),
                UiBundle::new(Node::column().display(Display::None)).children([row()]),
            ]).into_entity_builder()
        );
        let (mut rows, mut containers) = (vec![], vec![]);
        rows_under(&world, list, &mut rows, &mut containers);
        assert_eq!(rows.len(), 4);
        assert_eq!(containers.iter().map(|(_, range)| range.clone()).collect::<Vec<_>>(), vec![0..2, 2..4]);
    }
}
