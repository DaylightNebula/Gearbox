---
name: ui-immediate-widgets
description: Use when writing quick immediate mode UI with gearbox's ui module (gearbox::ui) - the Widget resource and Ui builder (panels, popups, huds, dialogs, labels, buttons, checkboxes, toggles, log sliders, text and number fields, segmented controls, list rows, headers, scroll_rows, sections, scroll areas and scrolling text boxes, right click context menus with submenus, LEDs, seven segment displays, keycaps, hints), its theme and fonts, and checking whether the pointer or keyboard belongs to the UI. For retained entity widgets see the gearbox ui-entity-widgets skill.
---

# gearbox ui: immediate mode widgets

Describe windows every frame from a render-update system through the `Widget` resource. Each
widget returns whether it was clicked or changed. This is good for debug and tool UIs; for UI
other code composes, styles or keeps, use entity nodes built as `UiBundle`s (see the gearbox
ui-entity-widgets skill).

Needs `UIPlugin` and `WidgetPlugin::new(font)` (see the gearbox ui-layout skill).

```rust
use gearbox::{anarchy::{ResMut, Resource}, glam::Vec2};
use gearbox::ui::*;

#[derive(Debug, Resource)]
struct Settings { enabled: bool, rate: u32, name: String, mode: usize, picked: Option<String> }

/// Add with `on_render_update`, after inserting a `Settings`.
fn settings_ui(mut widget: ResMut<Widget>, mut settings: ResMut<Settings>) {
    // reborrow once, so the closures can borrow fields
    let settings = &mut *settings;
    widget.panel("Settings", Vec2::splat(10.0), 320.0, |ui| {
        ui.checkbox(&mut settings.enabled, "Enabled");
        ui.log_slider("rate", &mut settings.rate, 1..=1000, settings.enabled, " per second");
        ui.row(|ui| {
            ui.text_field("name", &mut settings.name, "Name", 140.0);
            if ui.button_hint("Save", "Writes the settings") { log::info!("saving {}", settings.name); }
        });
        if let Some(mode) = ui.segmented("mode", &["Fast", "Nice"], settings.mode) { settings.mode = mode; }
        let fruit = vec!["Apple".to_string(), "Banana".to_string()];
        ui.sections("food", &[("Fruit", fruit, true)], 8, |ui, name| {
            if ui.selectable(name, settings.picked.as_ref() == Some(name), name) { settings.picked = Some(name.clone()); }
        });
    });
}
```

## Windows

These are methods on `Widget`. Positions are logical pixels, and all windows are kept on
screen and sized to their content.

- **`panel(title, pos, width, |ui| ..)`:** titled, and dragged by its title.
  `panel_with_header` adds widgets right of the title.
- **`popup(id, pos, width, ..)`:** untitled, drawn over panels.
- **`hud(id, anchor, margin, width, ..)` / `hud_row(..)`:** pinned to the screen and drawn under
  panels. `anchor` runs from (0, 0) at the top left to (1, 1) at the bottom right.
- **`dialog(id, anchor, width, ..)`:** a popup at a screen anchor.

To close a popup when the user clicks elsewhere, check `widget.pressed_outside(id)`. A window
opened while building another, like a submenu, draws over it.

```rust
use gearbox::{anarchy::ResMut, glam::Vec2};
use gearbox::ui::*;

fn files(mut widget: ResMut<Widget>) {
    widget.panel("Files", Vec2::splat(10.0), 240.0, |ui| {
        ui.scroll_rows("files", 50, 8, |ui, idx| {
            ui.selectable(&idx.to_string(), false, &format!("file {idx}.txt"));
            ui.context_menu(&format!("menu {idx}"), 160.0, |ui| {
                if ui.menu_item_shortcut("Open", "Enter") { log::info!("open {idx}"); }
                ui.submenu("Move to", 120.0, |ui| {
                    if ui.menu_item("Archive") { log::info!("archive {idx}"); }
                });
            });
        });
        ui.text_box("about", "Right click a file for its menu. Long text like this wraps and scrolls.", 40.0);
    });
}
```

## `Ui` widgets

| Kind | Methods |
| --- | --- |
| Text | `label`, `colored_label`, `label_right(text, width)`, `legend(text, width)`, `strong`, `heading`, `caption(text, color)` |
| Layout | `row(|ui| ..)`, `space(px)`, `separator()`, `header(key, text, default_open)` / `header_count(..)` with `is_open(key, default)` |
| Scrolling | `scroll_rows(key, rows, visible, |ui, idx| ..)` (rows one row tall), `sections(key, &[(title, entries, open)], visible, |ui, entry| ..)`, `scroll_area(key, max_height, |ui| ..)`, `text_box(key, text, max_height)` (wrapped); each scrolls by the wheel and a draggable scrollbar |
| Controls (`-> bool` clicked or changed) | `button`, `button_tone(text, Tone)`, `key_button(key, text, tone)`, `button_hint(text, hint)`, `selectable(key, selected, text)`, `list_row(key, selected, text, swatch, trailing)`, `checkbox(&mut bool, text)`, `toggle(&mut bool, text)`, `log_slider(key, &mut value, range, enabled, suffix)` (`u32` or `f64`), `log_track(..)`, `text_field(key, &mut String, hint, width)` |
| Others | `segmented(key, options, selected) -> Option<usize>`; `number_field(key, text, width) -> Option<String>` hands back typed text on Enter or a click elsewhere, so parse it yourself |
| Readouts | `seven_segment(text, digits, lit)`, `led(key, lit, color, text)`, `keycap(text)` |
| Menus | `context_menu(key, width, |ui| ..)` opens a menu at the pointer when the widget added last is right clicked; in it, `menu_item(text) -> bool` (chosen), `menu_item_shortcut(text, shortcut) -> bool`, `submenu(text, width, |ui| ..)` and `separator()`. Escape, choosing or pressing elsewhere closes it |
| Extras | `hint(text)` is a tooltip on the widget added last; `right_clicked()` checks it was right clicked |

## Ids and state

- Widgets are told apart by an id: the window's id, a slash, then the `key`. Buttons,
  checkboxes and toggles use their text as the key. Two widgets with the same key in one
  window share state; use `key_button` or distinct keys.
- Open headers, scroll positions, dragged window positions, open menus and text-field focus
  are kept by id inside `Widget`.
- Clicks land a frame late, because widgets are hit tested against last frame's layout.

## Theme and input

- `widget.theme_mut()`, `set_font` and `set_bold_font` change the shared `UiTheme`, so entity
  widgets follow too. `ui.theme()` reads colors, for example `ui.colored_label("!", ui.theme().error)`.
- `widget.wants_pointer()` and `over_ui()` tell game input whether the pointer belongs to the
  UI. `widget.typing()` means a text field has the keyboard, so skip shortcuts.
- `widget.clipboard_text()` reads the OS clipboard.
- **Text fields** edit like entity `TextInput`s: clicks place the caret, double clicks select
  words, arrows and Home/End move, and Command (macOS) or Control does select all, copy, cut,
  paste and undo. Long text scrolls to keep the caret in view. They're single line; for
  multiline fields, colored spans or hovers, use an entity `TextInput` (gearbox
  ui-entity-widgets skill).

## Gotchas

- **Element limits.** Every glyph is an SDF element and the buffers hold a few thousand. Keep
  long lists in `scroll_rows`/`sections`, which build only their visible rows. `scroll_area`
  builds everything, though what's scrolled away isn't drawn.
- **`scroll_area` sizing.** Whether its content overflows is known after building it, so the
  frame it first overflows draws without the scrollbar's room.
- **Borrowing.** Take `ResMut<Widget>` and your own resources as separate parameters, and
  reborrow (`let s = &mut *settings;`) before the closures.
- **Layering.** Immediate windows draw over entity UI, and hide the entity nodes under them
  from the pointer.
