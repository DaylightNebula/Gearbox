//! Caret, selection and undo for text being edited, shared by immediate mode fields and the
//! `TextInput` entity widget.

use std::ops::Range;

use magician_vgpu::glam::Vec2;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::ui::{KeyOutcome, TextLayout, TextSpan, UiClipboard, command};

/// Undo steps kept.
const UNDO_STEPS: usize = 200;

/// What a key edits besides the text: modifiers, line handling, the layout for moving between
/// lines, and the clipboard.
#[derive(Clone, Copy)]
pub struct EditContext<'a> {
    pub modifiers: ModifiersState,
    /// Enter types a newline (Command or Control with Enter submits) and Tab types a tab.
    pub multiline: bool,
    /// The text's layout, for Up, Down, Home and End on wrapped lines. Ignored if it's of other
    /// text.
    pub layout: Option<&'a TextLayout>,
    pub clipboard: &'a UiClipboard,
}

/// The caret, selection and undo history of text being edited. The text itself is passed to
/// each call, so it can live anywhere.
#[derive(Clone, Debug, Default)]
pub struct TextEditor {
    /// Byte offset of the caret.
    pub caret: usize,
    /// The other end of the selection, the caret when nothing's selected.
    pub anchor: usize,
    /// Where Up and Down try to keep the caret across lines.
    preferred_x: Option<f32>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The last change was typing, which later typing joins in one undo step.
    typing: bool,
}

#[derive(Clone, Debug)]
struct Snapshot { text: String, caret: usize, anchor: usize }

/// The byte class words are runs of.
fn class(ch: char) -> u8 {
    if ch.is_whitespace() { 0 } else if ch.is_alphanumeric() || ch == '_' { 1 } else { 2 }
}

/// Start of the word before `at`.
fn word_before(text: &str, at: usize) -> usize {
    let mut chars = text[..at].char_indices().rev().skip_while(|(_, ch)| ch.is_whitespace()).peekable();
    let Some(&(_, first)) = chars.peek() else { return 0 };
    let mut start = at;
    for (idx, ch) in chars {
        if class(ch) != class(first) { break }
        start = idx;
    }
    start.min(at)
}

/// End of the word after `at`.
fn word_after(text: &str, at: usize) -> usize {
    let mut chars = text[at..].char_indices().skip_while(|(_, ch)| ch.is_whitespace()).peekable();
    let Some(&(_, first)) = chars.peek() else { return text.len() };
    for (idx, ch) in chars {
        if class(ch) != class(first) { return at + idx }
    }
    text.len()
}

fn prev_char(text: &str, at: usize) -> usize { text[..at].char_indices().next_back().map_or(0, |(idx, _)| idx) }

fn next_char(text: &str, at: usize) -> usize { text[at..].chars().next().map_or(at, |ch| at + ch.len_utf8()) }

/// The hard line (between newlines) around `at`.
fn hard_line(text: &str, at: usize) -> Range<usize> {
    let start = text[..at].rfind('\n').map_or(0, |idx| idx + 1);
    let end = text[at..].find('\n').map_or(text.len(), |idx| at + idx);
    start..end
}

impl TextEditor {
    /// An editor with the caret at the end of `text`.
    pub fn at_end(text: &str) -> Self { Self { caret: text.len(), anchor: text.len(), ..Default::default() } }

    /// The selected bytes, if any.
    pub fn selection(&self) -> Option<Range<usize>> {
        (self.caret != self.anchor).then(|| self.caret.min(self.anchor)..self.caret.max(self.anchor))
    }

    pub fn select_all(&mut self, text: &str) {
        self.anchor = 0;
        self.caret = text.len();
    }

    /// Moves the caret to `at`, keeping the selection's other end if `extend`.
    pub fn set_caret(&mut self, at: usize, extend: bool) {
        self.caret = at;
        if !extend { self.anchor = at; }
        self.preferred_x = None;
        self.typing = false;
    }

    /// Keeps the caret and selection inside `text` and on character boundaries, after the text
    /// changed underneath.
    pub fn clamp(&mut self, text: &str) {
        let fix = |mut at: usize| {
            at = at.min(text.len());
            while !text.is_char_boundary(at) { at -= 1; }
            at
        };
        self.caret = fix(self.caret);
        self.anchor = fix(self.anchor);
    }

    /// Places the caret for a press at `at`: one click moves it there (extending the selection
    /// if `extend`), two select the word there, three its line.
    pub fn click(&mut self, text: &str, at: usize, extend: bool, clicks: u32) {
        self.clamp(text);
        let at = at.min(text.len());
        match clicks {
            0 | 1 => self.set_caret(at, extend),
            2 => {
                let start = if at > 0 && text[..at].chars().next_back().is_some_and(|ch| class(ch) != 0) { word_before(text, at) } else { at };
                let end = text[at..].chars().next().filter(|ch| class(*ch) != 0).map_or(at, |_| word_after(text, at));
                self.anchor = start;
                self.caret = end;
            }
            _ => {
                let line = hard_line(text, at);
                self.anchor = line.start;
                self.caret = (line.end + 1).min(text.len());
            }
        }
        self.typing = false;
    }

    /// Moves the caret to `at` while a press is dragged, selecting from where it went down.
    pub fn drag(&mut self, text: &str, at: usize) {
        self.caret = at.min(text.len());
        self.preferred_x = None;
    }

    fn snapshot(&mut self, text: &str) {
        self.undo.push(Snapshot { text: text.to_string(), caret: self.caret, anchor: self.anchor });
        if self.undo.len() > UNDO_STEPS { self.undo.remove(0); }
        self.redo.clear();
    }

    /// Replaces the selection with `typed`, or inserts it at the caret.
    pub fn insert(&mut self, text: &mut String, typed: &str) {
        let range = self.selection().unwrap_or(self.caret..self.caret);
        if typed.is_empty() && range.is_empty() { return }
        // typing a run of characters is one undo step
        let joins = self.typing && range.is_empty() && !typed.is_empty() && !typed.contains(char::is_whitespace);
        if !joins { self.snapshot(text); }
        text.replace_range(range.clone(), typed);
        self.caret = range.start + typed.len();
        self.anchor = self.caret;
        self.preferred_x = None;
        self.typing = !typed.is_empty();
    }

    /// Deletes the selection, or from the caret to `to`.
    fn delete_to(&mut self, text: &mut String, to: usize) -> KeyOutcome {
        if self.selection().is_none() {
            if to == self.caret { return KeyOutcome::Unchanged }
            self.anchor = to;
        }
        self.insert(text, "");
        self.typing = false;
        KeyOutcome::Edited
    }

    /// Steps back through undo or redo history.
    fn restore(&mut self, text: &mut String, redo: bool) -> KeyOutcome {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(snapshot) = from.pop() else { return KeyOutcome::Unchanged };
        to.push(Snapshot { text: std::mem::replace(text, snapshot.text), caret: self.caret, anchor: self.anchor });
        self.caret = snapshot.caret;
        self.anchor = snapshot.anchor;
        self.typing = false;
        self.preferred_x = None;
        KeyOutcome::Edited
    }

    /// Text typed or pasted with what a field can't hold taken out.
    fn typable(typed: &str, multiline: bool) -> String {
        typed.chars()
            .map(|ch| if ch == '\r' { '\n' } else { ch })
            .filter(|ch| if *ch == '\n' || *ch == '\t' { multiline } else { !ch.is_control() })
            .collect()
    }

    /// The caret moved to the line `delta` lines away, at the same x.
    fn vertical(&mut self, text: &str, layout: Option<&TextLayout>, delta: isize) -> usize {
        let Some(layout) = layout.filter(|layout| layout.len == text.len()) else {
            // a single line: up goes to the start, down to the end
            return if delta < 0 { 0 } else { text.len() }
        };
        let line = layout.line_of(self.caret) as isize + delta;
        let x = *self.preferred_x.get_or_insert(layout.caret_at(self.caret).x);
        if line < 0 { return 0 }
        if line as usize >= layout.lines.len() { return text.len() }
        layout.index_on_line(line as usize, x)
    }

    /// The line around the caret, as wrapped if the layout is of `text`.
    fn line(&self, text: &str, layout: Option<&TextLayout>) -> Range<usize> {
        match layout.filter(|layout| layout.len == text.len()) {
            Some(layout) => layout.lines.get(layout.line_of(self.caret)).map_or(0..text.len(), |line| line.range.clone()),
            None => hard_line(text, self.caret),
        }
    }

    /// Applies `key`, which typed `typed`, to `text`.
    pub fn apply_key(&mut self, text: &mut String, key: &Key, typed: Option<&str>, context: EditContext) -> KeyOutcome {
        self.clamp(text);
        let modifiers = context.modifiers;
        let shift = modifiers.shift_key();
        let mac = cfg!(target_os = "macos");
        let word = if mac { modifiers.alt_key() } else { modifiers.control_key() };
        // Command with arrows works on whole lines on macOS
        let line_jump = mac && modifiers.super_key();
        let command = command(modifiers);
        let move_to = |editor: &mut Self, at: usize, keep_x: bool| {
            let x = editor.preferred_x;
            editor.set_caret(at, shift);
            if keep_x { editor.preferred_x = x; }
            KeyOutcome::Unchanged
        };
        match key {
            Key::Named(NamedKey::Enter) if context.multiline && !command => {
                self.insert(text, "\n");
                KeyOutcome::Edited
            }
            Key::Named(NamedKey::Enter) => KeyOutcome::Submit,
            Key::Named(NamedKey::Escape) => KeyOutcome::Cancel,
            Key::Named(NamedKey::Tab) if context.multiline => {
                self.insert(text, "\t");
                KeyOutcome::Edited
            }
            Key::Named(NamedKey::Backspace) => {
                let to = if line_jump { self.line(text, context.layout).start } else if word { word_before(text, self.caret) } else { prev_char(text, self.caret) };
                self.delete_to(text, to)
            }
            Key::Named(NamedKey::Delete) => {
                let to = if line_jump { self.line(text, context.layout).end } else if word { word_after(text, self.caret) } else { next_char(text, self.caret) };
                self.delete_to(text, to)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                let at = match self.selection() {
                    Some(selection) if !shift => selection.start,
                    _ if line_jump => self.line(text, context.layout).start,
                    _ if word => word_before(text, self.caret),
                    _ => prev_char(text, self.caret),
                };
                move_to(self, at, false)
            }
            Key::Named(NamedKey::ArrowRight) => {
                let at = match self.selection() {
                    Some(selection) if !shift => selection.end,
                    _ if line_jump => self.line(text, context.layout).end,
                    _ if word => word_after(text, self.caret),
                    _ => next_char(text, self.caret),
                };
                move_to(self, at, false)
            }
            Key::Named(NamedKey::ArrowUp) if line_jump => move_to(self, 0, false),
            Key::Named(NamedKey::ArrowDown) if line_jump => move_to(self, text.len(), false),
            Key::Named(NamedKey::ArrowUp) => {
                let at = self.vertical(text, context.layout, -1);
                move_to(self, at, true)
            }
            Key::Named(NamedKey::ArrowDown) => {
                let at = self.vertical(text, context.layout, 1);
                move_to(self, at, true)
            }
            Key::Named(NamedKey::Home) if command => move_to(self, 0, false),
            Key::Named(NamedKey::End) if command => move_to(self, text.len(), false),
            Key::Named(NamedKey::Home) => {
                let at = self.line(text, context.layout).start;
                move_to(self, at, false)
            }
            Key::Named(NamedKey::End) => {
                let at = self.line(text, context.layout).end;
                move_to(self, at, false)
            }
            Key::Character(ch) if command => match ch.to_lowercase().as_str() {
                "a" => {
                    self.select_all(text);
                    KeyOutcome::Unchanged
                }
                "c" => {
                    if let Some(selection) = self.selection() { context.clipboard.set_text(&text[selection]); }
                    KeyOutcome::Unchanged
                }
                "x" => {
                    let Some(selection) = self.selection() else { return KeyOutcome::Unchanged };
                    context.clipboard.set_text(&text[selection]);
                    self.delete_to(text, self.caret)
                }
                "v" => {
                    let pasted = Self::typable(&context.clipboard.text().unwrap_or_default(), context.multiline);
                    if pasted.is_empty() { return KeyOutcome::Unchanged }
                    self.insert(text, &pasted);
                    self.typing = false;
                    KeyOutcome::Edited
                }
                "z" => self.restore(text, shift),
                "y" if !mac => self.restore(text, true),
                _ => KeyOutcome::Unchanged,
            },
            _ => {
                let typed = Self::typable(typed.unwrap_or_default(), false);
                if typed.is_empty() || modifiers.control_key() && !modifiers.alt_key() { return KeyOutcome::Unchanged }
                self.insert(text, &typed);
                KeyOutcome::Edited
            }
        }
    }
}

/// Seconds the caret stays on, then off, while blinking.
pub const CARET_BLINK: f32 = 0.5;

/// Whether a caret `seconds` into blinking shows.
pub fn caret_shows(seconds: f32) -> bool { seconds % (CARET_BLINK * 2.0) < CARET_BLINK }

/// Width of the caret drawn in text `font_size` big. SDF edges fade over a pixel, so 2 is a
/// crisp line one pixel wide when centered on a pixel.
pub fn caret_width(font_size: f32) -> f32 { (font_size / 12.0).max(2.0) }

/// `scroll` kept between nothing and the end of `layout`'s text in a view `view` big.
pub fn clamp_scroll(layout: &TextLayout, view: Vec2, scroll: Vec2) -> Vec2 {
    let size = layout.size() + Vec2::new(caret_width(layout.font_size), 0.0);
    scroll.clamp(Vec2::ZERO, (size - view).max(Vec2::ZERO))
}

/// `scroll` moved as little as it takes to show the caret at `caret` in a view `view` big.
pub fn follow_caret(layout: &TextLayout, caret: usize, view: Vec2, scroll: Vec2) -> Vec2 {
    let at = layout.caret_at(caret.min(layout.len));
    let end = at + Vec2::new(caret_width(layout.font_size), layout.line_height);
    let mut scroll = scroll;
    if end.x - scroll.x > view.x { scroll.x = end.x - view.x; }
    if at.x < scroll.x { scroll.x = at.x; }
    if end.y - scroll.y > view.y { scroll.y = end.y - view.y; }
    if at.y < scroll.y { scroll.y = at.y; }
    clamp_scroll(layout, view, scroll)
}

/// What a text field shows: its text, the draft being typed, or its hint while there's
/// neither. The second value is whether that's a placeholder (the hint, or the text an empty
/// draft would replace), drawn dim with the caret before it.
pub fn field_shows<'a>(text: &'a str, draft: Option<&'a str>, hint: &'a str, focused: bool) -> (&'a str, bool) {
    match (focused, draft) {
        (true, Some("")) => (text, true),
        (true, Some(draft)) => (draft, false),
        (true, None) => (text, false),
        (false, _) if text.is_empty() => (hint, true),
        (false, _) => (text, false),
    }
}

/// The bytes of `old` that changed to make `new`, and how long their replacement is.
pub fn text_change(old: &str, new: &str) -> Option<(Range<usize>, usize)> {
    if old == new { return None }
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) { prefix -= 1; }
    let room = old.len().min(new.len()) - prefix;
    let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take(room).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) { suffix -= 1; }
    Some((prefix..old.len() - suffix, new.len() - prefix - suffix))
}

/// Moves `spans` to follow the text after `replaced` became `inserted` bytes long. Text typed
/// inside a span joins it, text typed at either end doesn't, and deleted text leaves it.
pub fn remap_spans(spans: &mut [TextSpan], replaced: Range<usize>, inserted: usize) {
    let shift = |at: usize| {
        if at <= replaced.start { at } else if at >= replaced.end { at + inserted - (replaced.end - replaced.start) } else { replaced.start + inserted }
    };
    for span in spans {
        let start = if span.range.start == replaced.start && replaced.is_empty() { span.range.start + inserted } else { shift(span.range.start) };
        let end = shift(span.range.end).max(start);
        span.range = start..end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: NamedKey) -> Key { Key::Named(name) }

    struct Field { text: String, editor: TextEditor, clipboard: UiClipboard, modifiers: ModifiersState, multiline: bool }

    impl Field {
        fn new(text: &str) -> Self {
            Self { text: text.into(), editor: TextEditor::at_end(text), clipboard: UiClipboard::default(), modifiers: ModifiersState::empty(), multiline: false }
        }

        fn press(&mut self, key: Key, typed: Option<&str>) -> KeyOutcome {
            let context = EditContext { modifiers: self.modifiers, multiline: self.multiline, layout: None, clipboard: &self.clipboard };
            self.editor.apply_key(&mut self.text, &key, typed, context)
        }

        fn typed(&mut self, typed: &str) { for ch in typed.chars() { self.press(Key::Character(ch.to_string().into()), Some(&ch.to_string())); } }

        fn with(&mut self, modifiers: ModifiersState, run: impl FnOnce(&mut Self)) {
            self.modifiers = modifiers;
            run(self);
            self.modifiers = ModifiersState::empty();
        }
    }

    fn word_key() -> ModifiersState { if cfg!(target_os = "macos") { ModifiersState::ALT } else { ModifiersState::CONTROL } }

    fn command_key() -> ModifiersState { if cfg!(target_os = "macos") { ModifiersState::SUPER } else { ModifiersState::CONTROL } }

    #[test]
    fn types_and_moves_the_caret() {
        let mut field = Field::new("ac");
        field.press(key(NamedKey::ArrowLeft), None);
        field.typed("b");
        assert_eq!((field.text.as_str(), field.editor.caret), ("abc", 2));
        assert_eq!(field.press(key(NamedKey::Backspace), None), KeyOutcome::Edited);
        assert_eq!(field.press(key(NamedKey::Delete), None), KeyOutcome::Edited);
        assert_eq!(field.text, "a");
        assert_eq!(field.press(key(NamedKey::Enter), Some("\r")), KeyOutcome::Submit);
        assert_eq!(field.press(key(NamedKey::Escape), None), KeyOutcome::Cancel);
        // multibyte characters move and delete whole
        let mut field = Field::new("aé");
        field.press(key(NamedKey::ArrowLeft), None);
        assert_eq!(field.editor.caret, 1);
        field.press(key(NamedKey::Delete), None);
        assert_eq!(field.text, "a");
    }

    #[test]
    fn selects_and_replaces() {
        let mut field = Field::new("hello world");
        field.with(ModifiersState::SHIFT, |field| { for _ in 0..5 { field.press(key(NamedKey::ArrowLeft), None); } });
        assert_eq!(field.editor.selection(), Some(6..11));
        field.typed("there");
        assert_eq!(field.text, "hello there");
        field.press(key(NamedKey::Home), None);
        assert_eq!(field.editor.caret, 0);
        field.with(word_key() | ModifiersState::SHIFT, |field| { field.press(key(NamedKey::ArrowRight), None); });
        assert_eq!(field.editor.selection(), Some(0..5));
        field.press(key(NamedKey::Backspace), None);
        assert_eq!(field.text, " there");
    }

    #[test]
    fn jumps_and_deletes_words() {
        let mut field = Field::new("let x = foo_bar(1);");
        field.with(word_key(), |field| { field.press(key(NamedKey::ArrowLeft), None); });
        assert_eq!(field.editor.caret, 17);
        field.with(word_key(), |field| { field.press(key(NamedKey::ArrowLeft), None); });
        assert_eq!(field.editor.caret, 16);
        field.with(word_key(), |field| { field.press(key(NamedKey::ArrowLeft), None); });
        assert_eq!(field.editor.caret, 15);
        field.with(word_key(), |field| { field.press(key(NamedKey::Backspace), None); });
        assert_eq!(field.text, "let x = (1);");
    }

    #[test]
    fn copies_pastes_and_undoes() {
        let mut field = Field::new("abc");
        field.with(command_key(), |field| {
            field.press(Key::Character("a".into()), Some("a"));
            field.press(Key::Character("c".into()), Some("c"));
        });
        assert_eq!(field.clipboard.text().as_deref(), Some("abc"));
        field.press(key(NamedKey::ArrowRight), None);
        field.with(command_key(), |field| { field.press(Key::Character("v".into()), Some("v")); });
        assert_eq!(field.text, "abcabc");
        field.typed("de");
        assert_eq!(field.text, "abcabcde");
        field.with(command_key(), |field| { field.press(Key::Character("z".into()), Some("z")); });
        assert_eq!(field.text, "abcabc");
        field.with(command_key(), |field| { field.press(Key::Character("z".into()), Some("z")); });
        assert_eq!(field.text, "abc");
        field.with(command_key() | ModifiersState::SHIFT, |field| { field.press(Key::Character("Z".into()), Some("Z")); });
        assert_eq!(field.text, "abcabc");
        // a single line field takes pasted newlines out
        field.clipboard.set_text("x\ny");
        field.with(command_key(), |field| { field.press(Key::Character("v".into()), Some("v")); });
        assert_eq!(field.text, "abcabcxy");
    }

    #[test]
    fn multiline_fields_take_newlines_and_move_by_line() {
        let mut field = Field::new("ab");
        field.multiline = true;
        assert_eq!(field.press(key(NamedKey::Enter), Some("\r")), KeyOutcome::Edited);
        field.typed("cd");
        assert_eq!(field.text, "ab\ncd");
        field.press(key(NamedKey::Home), None);
        assert_eq!(field.editor.caret, 3);
        field.with(command_key(), |field| { assert_eq!(field.press(key(NamedKey::Enter), Some("\r")), KeyOutcome::Submit); });
        let font = crate::ui::SDFFont::new(include_bytes!("../../examples/LiberationSans-Regular.ttf")).unwrap();
        let layout = font.layout(&field.text, 20.0, None);
        field.editor.set_caret(4, false);
        let context = EditContext { modifiers: ModifiersState::empty(), multiline: true, layout: Some(&layout), clipboard: &field.clipboard };
        field.editor.apply_key(&mut field.text, &key(NamedKey::ArrowUp), None, context);
        assert_eq!(field.editor.caret, 1);
        field.editor.apply_key(&mut field.text, &key(NamedKey::ArrowDown), None, context);
        assert_eq!(field.editor.caret, 4);
    }

    #[test]
    fn clicks_select_words_and_lines() {
        let mut editor = TextEditor::default();
        let text = "one two\nthree";
        editor.click(text, 5, false, 2);
        assert_eq!(editor.selection(), Some(4..7));
        editor.click(text, 10, false, 3);
        assert_eq!(editor.selection(), Some(8..13));
        editor.click(text, 2, false, 1);
        editor.drag(text, 6);
        assert_eq!(editor.selection(), Some(2..6));
    }

    #[test]
    fn spans_follow_edits() {
        assert_eq!(text_change("abc", "abXc"), Some((2..2, 1)));
        assert_eq!(text_change("abcd", "ad"), Some((1..3, 0)));
        assert_eq!(text_change("aaa", "aa"), Some((2..3, 0)));
        assert_eq!(text_change("same", "same"), None);

        let mut spans = vec![TextSpan::new(0..3), TextSpan::new(4..8), TextSpan::new(10..12)];
        // typing inside the second span grows it, and pushes the third along
        remap_spans(&mut spans, 5..5, 2);
        assert_eq!(spans.iter().map(|span| span.range.clone()).collect::<Vec<_>>(), vec![0..3, 4..10, 12..14]);
        // typing at a span's start pushes it rather than joining it
        remap_spans(&mut spans, 4..4, 1);
        assert_eq!(spans[1].range, 5..11);
        // deleting over a span's end cuts it short
        remap_spans(&mut spans, 2..6, 0);
        assert_eq!(spans.iter().map(|span| span.range.clone()).collect::<Vec<_>>(), vec![0..2, 2..7, 9..11]);
        // deleting all of it empties it
        remap_spans(&mut spans, 0..11, 0);
        assert!(spans.iter().all(|span| span.range.is_empty()));
    }
}
