//! Text laid out in lines, shared by measuring, drawing, caret placement and hit testing.

use std::ops::Range;

use magician_vgpu::glam::Vec2;

use crate::ui::{Align, SDFFont};

/// Text broken into lines. Positions are pixels from the top left of the first line, and
/// indices are byte offsets into the text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<TextLine>,
    pub line_height: f32,
    /// Distance from the top of a line down to its baseline.
    pub baseline: f32,
    pub font_size: f32,
    /// Length of the text in bytes.
    pub len: usize,
}

/// One line of a `TextLayout`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextLine {
    /// Bytes of the text on this line, without the newline ending it.
    pub range: Range<usize>,
    /// Where the line starts, moved by `TextLayout::align`.
    pub x: f32,
    pub width: f32,
    pub glyphs: Vec<TextGlyph>,
}

/// A character placed in a `TextLine`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextGlyph {
    /// Byte offset of the character.
    pub index: usize,
    pub ch: char,
    /// From the start of the line.
    pub x: f32,
    pub advance: f32,
}

impl TextLine {
    /// Where `index` falls along the line, from the start of the layout.
    pub fn x_of(&self, index: usize) -> f32 {
        let along = self.glyphs.iter().find(|glyph| glyph.index >= index).map_or(self.width, |glyph| glyph.x);
        self.x + along
    }

    /// The index nearest `x` (from the start of the layout) on this line.
    pub fn index_at(&self, x: f32) -> usize {
        let x = x - self.x;
        self.glyphs.iter().find(|glyph| x < glyph.x + glyph.advance * 0.5).map_or(self.range.end, |glyph| glyph.index)
    }
}

impl TextLayout {
    pub(crate) fn new(font: &SDFFont, text: &str, font_size: f32, wrap: Option<f32>) -> Self {
        let mut lines = vec![];
        let mut start = 0;
        for hard in text.split('\n') {
            let mut line = TextLine { range: start..start, ..Default::default() };
            // the glyph after the last space, where a wrap can break the line
            let mut break_at = None;
            for (offset, ch) in hard.char_indices() {
                let index = start + offset;
                let Some(advance) = font.advance(ch, font_size) else { continue };
                if let Some(width) = wrap && line.width + advance > width && !line.glyphs.is_empty() && !ch.is_whitespace() {
                    let split = break_at.filter(|at| *at > 0).unwrap_or(line.glyphs.len());
                    let moved = line.glyphs.split_off(split);
                    let next_start = moved.first().map_or(index, |glyph| glyph.index);
                    line.range.end = next_start;
                    line.width = line.glyphs.last().map_or(0.0, |glyph| glyph.x + glyph.advance);
                    let shift = moved.first().map_or(0.0, |glyph| glyph.x);
                    let glyphs = moved.into_iter().map(|glyph| TextGlyph { x: glyph.x - shift, ..glyph }).collect::<Vec<_>>();
                    let width = glyphs.last().map_or(0.0, |glyph| glyph.x + glyph.advance);
                    lines.push(std::mem::replace(&mut line, TextLine { range: next_start..next_start, x: 0.0, width, glyphs }));
                    break_at = None;
                }
                line.glyphs.push(TextGlyph { index, ch, x: line.width, advance });
                line.width += advance;
                if ch.is_whitespace() { break_at = Some(line.glyphs.len()); }
            }
            line.range.end = start + hard.len();
            lines.push(line);
            start += hard.len() + 1;
        }
        Self { lines, line_height: font.line_height(font_size), baseline: font.baseline(font_size), font_size, len: text.len() }
    }

    /// Width of the widest line and height of every line.
    pub fn size(&self) -> Vec2 {
        let width = self.lines.iter().map(|line| line.width).fold(0.0, f32::max);
        Vec2::new(width, self.lines.len() as f32 * self.line_height)
    }

    /// Places each line in a box `width` wide.
    pub fn align(&mut self, align: Align, width: f32) {
        for line in &mut self.lines {
            line.x = match align {
                Align::Start => 0.0,
                Align::End => width - line.width,
                Align::Center => (width - line.width) * 0.5,
            };
        }
    }

    /// The line at height `y`, clamped to the first and last.
    pub fn line_at(&self, y: f32) -> usize {
        if self.line_height <= 0.0 { return 0 }
        ((y / self.line_height).floor().max(0.0) as usize).min(self.lines.len().saturating_sub(1))
    }

    /// The line holding `index`.
    pub fn line_of(&self, index: usize) -> usize {
        self.lines.partition_point(|line| line.range.start <= index).saturating_sub(1)
    }

    /// The index nearest `point`, where a click there puts the caret.
    pub fn index_at(&self, point: Vec2) -> usize {
        self.lines.get(self.line_at(point.y)).map_or(0, |line| line.index_at(point.x))
    }

    /// The index of the character drawn under `point`, if any.
    pub fn glyph_at(&self, point: Vec2) -> Option<usize> {
        if point.y < 0.0 { return None }
        let line = self.lines.get((point.y / self.line_height.max(f32::EPSILON)) as usize)?;
        let x = point.x - line.x;
        line.glyphs.iter().find(|glyph| x >= glyph.x && x < glyph.x + glyph.advance).map(|glyph| glyph.index)
    }

    /// Top left of a caret before `index`.
    pub fn caret_at(&self, index: usize) -> Vec2 {
        let line = self.line_of(index);
        let x = self.lines.get(line).map_or(0.0, |found| found.x_of(index));
        Vec2::new(x, line as f32 * self.line_height)
    }

    /// The index on line `line` nearest `x`.
    pub fn index_on_line(&self, line: usize, x: f32) -> usize {
        self.lines.get(line).map_or(self.len, |found| found.index_at(x))
    }

    /// The boxes (top left, size) covering `range`, one per line it touches.
    pub fn range_rects(&self, range: Range<usize>) -> Vec<(Vec2, Vec2)> {
        if range.is_empty() { return vec![] }
        let mut rects = vec![];
        let first = self.line_of(range.start);
        for (row, line) in self.lines.iter().enumerate().skip(first) {
            if line.range.start > range.end { break }
            if line.range.end < range.start { continue }
            let (start, end) = (range.start.max(line.range.start), range.end.min(line.range.end));
            let (x0, mut x1) = (line.x_of(start), line.x_of(end));
            // a selected newline shows as a sliver past the line's end
            if range.end > line.range.end && end == line.range.end { x1 += self.font_size * 0.25; }
            if x1 > x0 { rects.push((Vec2::new(x0, row as f32 * self.line_height), Vec2::new(x1 - x0, self.line_height))); }
        }
        rects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> SDFFont { SDFFont::new(include_bytes!("../../../examples/LiberationSans-Regular.ttf")).unwrap() }

    #[test]
    fn breaks_lines_at_newlines() {
        let font = font();
        let layout = font.layout("ab\ncd\n", 20.0, None);
        assert_eq!(layout.lines.iter().map(|line| line.range.clone()).collect::<Vec<_>>(), vec![0..2, 3..5, 6..6]);
        assert_eq!(layout.size().y, layout.line_height * 3.0);
        assert_eq!(layout.line_of(4), 1);
        assert_eq!(layout.line_of(6), 2);
        assert_eq!(layout.size().x, font.measure_text("ab", 20.0).x.max(font.measure_text("cd", 20.0).x));
    }

    #[test]
    fn hit_tests_and_places_carets() {
        let font = font();
        let layout = font.layout("hello\nworld", 20.0, None);
        let h = layout.line_height;
        assert_eq!(layout.index_at(Vec2::new(-5.0, 1.0)), 0);
        assert_eq!(layout.index_at(Vec2::new(1000.0, 1.0)), 5);
        assert_eq!(layout.index_at(Vec2::new(1000.0, h * 1.5)), 11);
        assert_eq!(layout.index_at(Vec2::new(0.0, h * 9.0)), 6);
        let x = layout.caret_at(2).x;
        assert_eq!(layout.index_at(Vec2::new(x + 0.1, 1.0)), 2);
        assert_eq!(layout.caret_at(8), Vec2::new(layout.lines[1].x_of(8), h));
        assert_eq!(layout.glyph_at(Vec2::new(x + 0.1, 1.0)), Some(2));
        assert_eq!(layout.glyph_at(Vec2::new(1000.0, 1.0)), None);
    }

    #[test]
    fn wraps_words() {
        let font = font();
        let word = font.measure_text("word ", 20.0).x;
        let layout = font.layout("word word word", 20.0, Some(word * 2.0 + 1.0));
        assert_eq!(layout.lines.iter().map(|line| line.range.clone()).collect::<Vec<_>>(), vec![0..10, 10..14]);
        assert_eq!(layout.lines[1].glyphs[0].x, 0.0);
        // a word longer than the line breaks where it runs out
        let layout = font.layout("abcdefgh", 20.0, Some(font.measure_text("abc", 20.0).x + 0.5));
        assert_eq!(layout.lines.len(), 3);
        assert_eq!(layout.lines[0].range, 0..3);
    }

    #[test]
    fn selections_cover_each_line() {
        let font = font();
        let layout = font.layout("ab\ncd", 20.0, None);
        let rects = layout.range_rects(1..4);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].0.x, layout.lines[0].x_of(1));
        assert_eq!(rects[1].0, Vec2::new(0.0, layout.line_height));
        assert!(layout.range_rects(2..2).is_empty());
    }
}
