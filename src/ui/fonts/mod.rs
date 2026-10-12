use crate::anyhow;
use magician_vgpu::glam::Vec2;

use crate::ui::SDFShape;

mod outline;

/// A font that creates characters rendered as SDFElement's.
#[derive(Debug)]
pub struct SDFFont {
    face: ttf_parser::Face<'static>
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
        Ok(Self {
            face: ttf_parser::Face::parse(bytes, 0)?
        })
    }

    /// Measure the width and height of a single line of text
    pub fn measure_text(&self, string: &str, font_size: f32) -> Vec2 {
        let font_size_mult = font_size / self.face.units_per_em() as f32;
        let height = self.face.height() as f32 * font_size_mult;
        let width = string.chars()
            .filter_map(|ch| {
                let Some(id) = self.face.glyph_index(ch)
                    else { return None };
                let advance = self.face.glyph_hor_advance(id)
                    .unwrap_or(0) as f32 * font_size_mult;
                Some(advance)
            })
            .sum();
        Vec2::new(width, height)
    }

    /// Render a string to a `SDFGlyphString` for drawing a string
    /// via SDFs.
    pub fn render_glyph_line(
        &self, 
        string: &str, 
        font_size: f32
    ) -> anyhow::Result<SDFGlyphString> {
        // calculate metadata constant across characters
        let font_size_mult = font_size / self.face.units_per_em() as f32;
        let height = self.face.height() as f32 * font_size_mult;
        let mut size_tracker = Vec2::new(0.0, height);

        // build entry for character
        let chars = string.chars()
            .filter_map(|ch| {
                let Some(id) = self.face.glyph_index(ch) 
                    else { return None; };

                // calculate character metadata and build outline
                let mut outline = outline::GlyphBuilder::default();
                let hor_advance = self.face
                    .glyph_hor_advance(id)
                    .unwrap_or(0) as f32 * font_size_mult;
                let bbox = self.face
                    .outline_glyph(id, &mut outline);

                // spaces have no outline but still take up room in the line
                size_tracker.x += hor_advance;

                // build character entries
                let char_entry =
                    if let Some(bbox) = bbox {
                        outline.set_bbox(bbox);
                        outline.set_units_per_em(self.face.units_per_em() as f32);

                        SDFCharEntry { 
                            shape: Some(outline.build(font_size, Vec2::new(hor_advance / -2.0, height / 2.0 + (self.face.descender() as f32 * font_size_mult)))), 
                            dimensions: Vec2::new(hor_advance, height), 
                            advance: hor_advance
                        }
                    } else {
                        SDFCharEntry { 
                            shape: None, 
                            dimensions: Vec2::ZERO, 
                            advance: hor_advance
                        }
                    };

                // cache and return char_entry
                return Some(char_entry);
            })
            .collect::<Vec<_>>();

        Ok(SDFGlyphString { chars, dimensions: size_tracker })
    }
}
