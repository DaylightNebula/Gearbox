use std::sync::Mutex;

use ahash::AHashMap;
use crate::anyhow;
use magician_vgpu::glam::Vec2;

use crate::ui::SDFShape;

mod layout;
mod outline;

pub use layout::*;

/// Spaces a tab is as wide as.
const TAB_SPACES: f32 = 4.0;

/// A font that creates characters rendered as SDFElement's.
pub struct SDFFont {
    face: ttf_parser::Face<'static>,
    /// Advances of ASCII characters in font units, `None` for ones the font lacks. Looking
    /// glyphs up is most of laying text out.
    ascii: [Option<u16>; 128],
    /// Glyph outlines by character and font size bits, `None` for glyphs without one.
    glyphs: Mutex<AHashMap<(char, u32), Option<SDFShape>>>,
}

impl std::fmt::Debug for SDFFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("SDFFont").finish_non_exhaustive() }
}

/// A string of `SDFCharEntry`s meant to represent a string in SDFs.
pub struct SDFGlyphString {
    pub chars: Vec<SDFCharEntry>,
    pub dimensions: Vec2
}

/// A single SDF character and its metrics.
#[derive(Clone, Debug)]
pub struct SDFCharEntry {
    pub shape: Option<SDFShape>,
    pub dimensions: Vec2,
    pub advance: f32
}

impl SDFFont {
    /// Create a new font from the bytes of a TTF file.
    pub fn new(bytes: &'static [u8]) -> anyhow::Result<Self> {
        let face = ttf_parser::Face::parse(bytes, 0)?;
        let ascii = std::array::from_fn(|idx| {
            let id = face.glyph_index(char::from(idx as u8))?;
            Some(face.glyph_hor_advance(id).unwrap_or(0))
        });
        Ok(Self { face, ascii, glyphs: Mutex::default() })
    }

    fn scale(&self, font_size: f32) -> f32 { font_size / self.face.units_per_em() as f32 }

    /// Height of a line of text.
    pub fn line_height(&self, font_size: f32) -> f32 { self.face.height() as f32 * self.scale(font_size) }

    /// Distance from the top of a line down to its baseline.
    pub fn baseline(&self, font_size: f32) -> f32 {
        (self.face.height() as f32 + self.face.descender() as f32) * self.scale(font_size)
    }

    /// How far `ch` moves the pen, tabs being `TAB_SPACES` spaces and characters the font
    /// doesn't have `None`.
    pub fn advance(&self, ch: char, font_size: f32) -> Option<f32> {
        if ch == '\t' { return self.advance(' ', font_size).map(|space| space * TAB_SPACES) }
        let units = match self.ascii.get(ch as usize) {
            Some(advance) => (*advance)?,
            None => self.face.glyph_hor_advance(self.face.glyph_index(ch)?).unwrap_or(0),
        };
        Some(units as f32 * self.scale(font_size))
    }

    /// The outline of `ch` around the center of its box, `None` for glyphs without one like
    /// spaces. Outlines are cached per size.
    pub fn glyph(&self, ch: char, font_size: f32) -> Option<SDFShape> {
        let key = (ch, font_size.to_bits());
        if let Ok(cache) = self.glyphs.lock() && let Some(shape) = cache.get(&key) { return shape.clone() }
        let shape = self.outline(ch, font_size);
        if let Ok(mut cache) = self.glyphs.lock() { cache.insert(key, shape.clone()); }
        shape
    }

    fn outline(&self, ch: char, font_size: f32) -> Option<SDFShape> {
        let id = self.face.glyph_index(ch)?;
        let mult = self.scale(font_size);
        let height = self.line_height(font_size);
        let hor_advance = self.face.glyph_hor_advance(id).unwrap_or(0) as f32 * mult;
        let mut outline = outline::GlyphBuilder::default();
        let bbox = self.face.outline_glyph(id, &mut outline)?;
        outline.set_bbox(bbox);
        outline.set_units_per_em(self.face.units_per_em() as f32);
        Some(outline.build(font_size, Vec2::new(hor_advance / -2.0, height / 2.0 + (self.face.descender() as f32 * mult))))
    }

    /// Measure the width and height of a single line of text
    pub fn measure_text(&self, string: &str, font_size: f32) -> Vec2 {
        let width = string.chars().filter_map(|ch| self.advance(ch, font_size)).sum();
        Vec2::new(width, self.line_height(font_size))
    }

    /// Render a string to a `SDFGlyphString` for drawing a string
    /// via SDFs.
    pub fn render_glyph_line(
        &self,
        string: &str,
        font_size: f32
    ) -> anyhow::Result<SDFGlyphString> {
        let height = self.line_height(font_size);
        let mut size_tracker = Vec2::new(0.0, height);
        let chars = string.chars()
            .filter_map(|ch| {
                let advance = self.advance(ch, font_size)?;
                // spaces have no outline but still take up room in the line
                size_tracker.x += advance;
                let shape = self.glyph(ch, font_size);
                let dimensions = if shape.is_some() { Vec2::new(advance, height) } else { Vec2::ZERO };
                Some(SDFCharEntry { shape, dimensions, advance })
            })
            .collect::<Vec<_>>();
        Ok(SDFGlyphString { chars, dimensions: size_tracker })
    }

    /// Lays `text` out in lines, breaking at newlines and, given `wrap`, wrapping words onto
    /// new lines to fit that width.
    pub fn layout(&self, text: &str, font_size: f32, wrap: Option<f32>) -> TextLayout {
        TextLayout::new(self, text, font_size, wrap)
    }
}
