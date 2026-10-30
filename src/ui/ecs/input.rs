//! Pointer, keyboard, focus, clipboard and theme state shared by immediate mode and entity
//! widgets.

use std::sync::{Arc, Mutex};

use anarchy::{Component, EntityID, Event, Res, ResMut, Resource, World};
use crate::{Graphics, WindowDimensions, WindowEvent};
use magician_vgpu::glam::{Vec2, Vec4};
use web_time::Instant;
use winit::{event::{ElementState, MouseButton, MouseScrollDelta}, keyboard::{Key, ModifiersState, NamedKey}, raw_window_handle::RawDisplayHandle};

use crate::ui::{SDFFont, TextHover, Theme};

/// Wheel pixels that count as one notch.
const PX_PER_NOTCH: f32 = 50.0;

/// Longest gap between the presses of a double click.
const MULTI_CLICK_SECONDS: f32 = 0.4;

/// Farthest the pointer can move between the presses of a double click.
const MULTI_CLICK_PX: f32 = 4.0;

/// This frame's pointer and keyboard input, read in the `Start` pin. Positions are window
/// pixels.
#[derive(Debug, Default, Resource)]
pub struct UiInput {
    pub cursor: Vec2,
    /// How far the cursor moved this frame.
    pub delta: Vec2,
    /// The left button is held.
    pub down: bool,
    /// The left button went down this frame.
    pub pressed: bool,
    /// The left button went up this frame.
    pub released: bool,
    /// The right button went down this frame.
    pub right_pressed: bool,
    /// Wheel notches this frame, positive scrolls up.
    pub wheel: f32,
    /// Window pixels per logical pixel.
    pub scale: f32,
    /// Window size in pixels.
    pub screen: Vec2,
    /// Keys pressed this frame, with the text each typed.
    pub keys: Vec<(Key, Option<String>)>,
    /// Topmost entity node with an `Interaction` under the pointer.
    pub hovered: Option<EntityID>,
    /// Entity node the held left button went down on, kept until the frame after it's let go.
    pub captured: Option<EntityID>,
    /// The pointer is over an immediate mode window, which draws over entity nodes.
    pub over_windows: bool,
    /// The pointer is over an entity node with a background.
    pub over_nodes: bool,
    /// Modifier keys held.
    pub modifiers: ModifiersState,
    /// Presses in quick succession the last press ends, 2 for a double click.
    pub clicks: u32,
    /// When and where the last press was.
    pub last_press: Option<(Instant, Vec2)>,
    /// The text span with a `hover` under the pointer, found after picking.
    pub hovered_span: Option<TextHover>,
}

impl UiInput {
    /// Whether the pointer is over any UI.
    pub fn over_ui(&self) -> bool { self.over_windows || self.over_nodes }

    /// Whether clicks and drags belong to the UI rather than what's behind it.
    pub fn wants_pointer(&self) -> bool { self.over_ui() || self.captured.is_some() }

    /// Logical pixels to window pixels.
    pub fn px(&self, v: f32) -> f32 { v * self.scale }

    /// Whether the key that goes with letters for shortcuts is held: Command on macOS, Control
    /// elsewhere.
    pub fn command(&self) -> bool { command(self.modifiers) }

    /// Whether `entity` was clicked: pressed, then let go of over it.
    pub fn activated(&self, entity: EntityID) -> bool {
        self.released && self.captured == Some(entity) && self.hovered == Some(entity)
    }
}

/// Reads this frame's window events into `UiInput`.
pub(crate) fn read_ui_input(mut input: ResMut<UiInput>, window_events: Event<WindowEvent>, graphics: Res<Graphics>, window: Res<WindowDimensions>) {
    let input = &mut *input;
    // a capture lasts through the frame its button is let go
    if !input.down { input.captured = None; }
    input.scale = graphics.window().map_or(1.0, |window| window.scale_factor() as f32);
    input.screen = Vec2::new(window.x as f32, window.y as f32);
    let last_cursor = input.cursor;
    input.pressed = false;
    input.released = false;
    input.right_pressed = false;
    input.wheel = 0.0;
    input.keys.clear();
    for event in window_events.read() {
        match &*event {
            winit::event::WindowEvent::CursorMoved { position, .. } => input.cursor = Vec2::new(position.x as f32, position.y as f32),
            winit::event::WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                input.down = *state == ElementState::Pressed;
                input.pressed |= input.down;
                input.released |= !input.down;
                if input.down {
                    let now = Instant::now();
                    let again = input.last_press.is_some_and(|(at, pos)| {
                        now.duration_since(at).as_secs_f32() < MULTI_CLICK_SECONDS && pos.distance(input.cursor) < MULTI_CLICK_PX
                    });
                    input.clicks = if again { input.clicks + 1 } else { 1 };
                    input.last_press = Some((now, input.cursor));
                }
            }
            winit::event::WindowEvent::ModifiersChanged(modifiers) => input.modifiers = modifiers.state(),
            winit::event::WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => input.right_pressed = true,
            winit::event::WindowEvent::MouseWheel { delta, .. } => input.wheel += match delta {
                MouseScrollDelta::LineDelta(_, y) => *y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 / PX_PER_NOTCH
            },
            winit::event::WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                input.keys.push((event.logical_key.clone(), event.text.as_ref().map(|text| text.to_string())));
            }
            _ => {}
        }
    }
    input.delta = input.cursor - last_cursor;
}

/// What a key did to the text being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// The text changed.
    Edited,
    /// Nothing happened.
    Unchanged,
    /// Enter: hand the text back.
    Submit,
    /// Escape: stop editing.
    Cancel,
}

/// Applies `key`, which typed `typed`, to `text`.
pub fn apply_key(text: &mut String, key: &Key, typed: Option<&str>) -> KeyOutcome {
    match key {
        Key::Named(NamedKey::Enter) => KeyOutcome::Submit,
        Key::Named(NamedKey::Escape) => KeyOutcome::Cancel,
        Key::Named(NamedKey::Backspace) => if text.pop().is_some() { KeyOutcome::Edited } else { KeyOutcome::Unchanged },
        _ => {
            let before = text.len();
            text.extend(typed.unwrap_or_default().chars().filter(|c| !c.is_control()));
            if text.len() == before { KeyOutcome::Unchanged } else { KeyOutcome::Edited }
        }
    }
}

/// Whether `modifiers` hold the key that goes with letters for shortcuts: Command on macOS,
/// Control elsewhere.
pub fn command(modifiers: ModifiersState) -> bool {
    if cfg!(target_os = "macos") { modifiers.super_key() } else { modifiers.control_key() }
}

/// The entity node taking typed text, if any.
#[derive(Debug, Default, Resource)]
pub struct UiFocus {
    pub focused: Option<EntityID>,
}

/// Widget colors and fonts, shared by immediate mode and themed entity widgets. Change part of
/// it for one entity tree with a `ThemeOverride`.
#[derive(Debug, Default, Clone, Resource)]
pub struct UiTheme {
    pub theme: Theme,
    pub font: Option<Arc<SDFFont>>,
    /// Font for titles, headings and strong text, the regular one if `None`.
    pub bold: Option<Arc<SDFFont>>,
}

impl UiTheme {
    /// The regular or bold font.
    pub fn font(&self, bold: bool) -> Option<&Arc<SDFFont>> {
        if bold { self.bold.as_ref().or(self.font.as_ref()) } else { self.font.as_ref() }
    }
}

/// Changes the theme for a node and everything below it, put on a bundle with
/// `UiBundle::theme`. Each field left `None` keeps what the node's parent uses, and overrides
/// nearer the node win. Change its fields at runtime to restyle the tree; a node only has one if
/// it was spawned with one, so spawn `ThemeOverride::default()` to fill it in later.
#[derive(Debug, Default, Clone, Component)]
pub struct ThemeOverride {
    pub panel: Option<Vec4>,
    pub border: Option<Vec4>,
    pub widget: Option<Vec4>,
    pub hover: Option<Vec4>,
    pub well: Option<Vec4>,
    pub selected: Option<Vec4>,
    pub accent: Option<Vec4>,
    pub on_accent: Option<Vec4>,
    pub text: Option<Vec4>,
    pub dim: Option<Vec4>,
    pub error: Option<Vec4>,
    pub font: Option<Arc<SDFFont>>,
    pub bold: Option<Arc<SDFFont>>,
}

impl ThemeOverride {
    /// Replaces every color with `theme`'s.
    pub fn colors(theme: Theme) -> Self {
        Self {
            panel: Some(theme.panel),
            border: Some(theme.border),
            widget: Some(theme.widget),
            hover: Some(theme.hover),
            well: Some(theme.well),
            selected: Some(theme.selected),
            accent: Some(theme.accent),
            on_accent: Some(theme.on_accent),
            text: Some(theme.text),
            dim: Some(theme.dim),
            error: Some(theme.error),
            font: None,
            bold: None,
        }
    }

    /// Writes the fields that are set over `theme`.
    pub fn apply(&self, theme: &mut UiTheme) {
        let t = &mut theme.theme;
        for (field, value) in [
            (&mut t.panel, self.panel), (&mut t.border, self.border), (&mut t.widget, self.widget),
            (&mut t.hover, self.hover), (&mut t.well, self.well), (&mut t.selected, self.selected),
            (&mut t.accent, self.accent), (&mut t.on_accent, self.on_accent), (&mut t.text, self.text),
            (&mut t.dim, self.dim), (&mut t.error, self.error),
        ] {
            if let Some(value) = value { *field = value; }
        }
        if let Some(font) = &self.font { theme.font = Some(font.clone()); }
        if let Some(bold) = &self.bold { theme.bold = Some(bold.clone()); }
    }
}

/// The theme `id` is drawn with: `base` with the `ThemeOverride`s on `id` and its ancestors
/// applied, the nearest last. Useful for styling headless widgets to match their tree.
pub fn local_theme(world: &World, id: EntityID, base: &UiTheme) -> UiTheme {
    let overrides: Vec<ThemeOverride> = std::iter::once(id)
        .chain(world.hierarchy().ancestors(id))
        .filter_map(|entity| world.handle(entity).get::<&ThemeOverride>().ok().flatten().map(|found| found.clone()))
        .collect();
    let mut theme = base.clone();
    for found in overrides.iter().rev() { found.apply(&mut theme); }
    theme
}

/// The OS clipboard, or one kept in memory where there's none (the web, tests).
#[derive(Debug, Resource, Clone)]
pub struct UiClipboard(pub(crate) Arc<Mutex<Clipboard>>);

impl Default for UiClipboard {
    fn default() -> Self { Self(Arc::new(Mutex::new(Clipboard::Memory(None)))) }
}

impl UiClipboard {
    pub(crate) fn new(display: Option<RawDisplayHandle>) -> Self { Self(Arc::new(Mutex::new(Clipboard::new(display)))) }

    /// The clipboard's text, if it has any.
    pub fn text(&self) -> Option<String> { self.0.lock().ok()?.text() }

    /// Puts `text` on the clipboard.
    pub fn set_text(&self, text: impl Into<String>) {
        if let Ok(mut clipboard) = self.0.lock() { clipboard.set_text(text.into()); }
    }
}

/// The OS clipboard, through the window's own Wayland connection there like egui-winit.
pub(crate) enum Clipboard {
    #[cfg(target_os = "linux")]
    Wayland(smithay_clipboard::Clipboard),
    #[cfg(not(target_arch = "wasm32"))]
    Arboard(arboard::Clipboard),
    /// Text copied inside the app, where there's no OS clipboard.
    Memory(Option<String>)
}

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("Clipboard").finish_non_exhaustive() }
}

impl Clipboard {
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn new(display: Option<RawDisplayHandle>) -> Self {
        #[cfg(target_os = "linux")]
        if let Some(RawDisplayHandle::Wayland(display)) = display {
            // SAFETY: the display is the window's, which lives as long as the app
            return Self::Wayland(unsafe { smithay_clipboard::Clipboard::new(display.display.as_ptr()) })
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(clipboard) = arboard::Clipboard::new().inspect_err(|err| eprintln!("No clipboard: {err}")).ok() {
            return Self::Arboard(clipboard)
        }
        Self::Memory(None)
    }

    fn text(&mut self) -> Option<String> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Wayland(clipboard) => clipboard.load().ok(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Arboard(clipboard) => clipboard.get_text().ok(),
            Self::Memory(text) => text.clone()
        }
    }

    fn set_text(&mut self, text: String) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Wayland(clipboard) => clipboard.store(text),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Arboard(clipboard) => if let Err(err) = clipboard.set_text(text) { log::error!("Failed to copy: {err}") },
            Self::Memory(stored) => *stored = Some(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_inherit_down_the_tree() {
        use crate::ui::{Node, UiBundle};
        let world = World::default();
        let (red, green, blue) = (Vec4::new(1.0, 0.0, 0.0, 1.0), Vec4::new(0.0, 1.0, 0.0, 1.0), Vec4::new(0.0, 0.0, 1.0, 1.0));

        let root = world.spawn(
            UiBundle::new(Node::new())
                .theme(ThemeOverride { accent: Some(red), panel: Some(red), ..Default::default() })
                .children([
                    UiBundle::new(Node::new())
                        .theme(ThemeOverride { accent: Some(green), text: Some(blue), ..Default::default() })
                        .child(UiBundle::new(Node::new())),
                    UiBundle::new(Node::new()),
                ])
                .into_entity_builder()
        );

        let children = world.hierarchy().children(root).unwrap().to_vec();
        let grandchild = world.hierarchy().children(children[0]).unwrap()[0];
        let base = UiTheme::default();

        let deep = local_theme(&world, grandchild, &base).theme;
        assert_eq!((deep.accent, deep.text, deep.panel, deep.well), (green, blue, red, base.theme.well));
        let sibling = local_theme(&world, children[1], &base).theme;
        assert_eq!((sibling.accent, sibling.text), (red, base.theme.text));
        assert_eq!(local_theme(&world, root, &base).theme.accent, red);
    }

    #[test]
    fn keys_edit_text() {
        let mut text = String::from("ab");
        assert_eq!(apply_key(&mut text, &Key::Character("c".into()), Some("c")), KeyOutcome::Edited);
        assert_eq!(text, "abc");
        assert_eq!(apply_key(&mut text, &Key::Named(NamedKey::Backspace), None), KeyOutcome::Edited);
        assert_eq!(text, "ab");
        assert_eq!(apply_key(&mut text, &Key::Named(NamedKey::Tab), Some("\t")), KeyOutcome::Unchanged);
        assert_eq!(apply_key(&mut text, &Key::Named(NamedKey::Enter), Some("\r")), KeyOutcome::Submit);
        assert_eq!(apply_key(&mut text, &Key::Named(NamedKey::Escape), None), KeyOutcome::Cancel);
        assert_eq!(text, "ab");
        let mut empty = String::new();
        assert_eq!(apply_key(&mut empty, &Key::Named(NamedKey::Backspace), None), KeyOutcome::Unchanged);
    }
}
