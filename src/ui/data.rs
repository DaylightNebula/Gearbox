use getset::{Getters, MutGetters, Setters};
use crate::TextureHandle;
use magician_vgpu::{ChunkHandle, TreeBufferElement, glam::*};
use mutual::CowData;

use crate::ui::{shader::{SDFRawBezier, SDFRawGlyph, SDFRawRectangle, SDFRawShape, SDFRawStyle}};

/// Screen-wide parameters passed to the UI SDF shader each frame.
///
/// `screen_dimensions` is the render target size in pixels, `time` is elapsed
/// application time in seconds for animation, and `mode` selects how shapes are
/// colored.
#[derive(Debug, Getters, Setters, MutGetters, Clone, PartialEq)]
#[getset(get = "pub", set = "pub", get_mut = "pub")]
pub struct SDFMetadata {
    pub screen_dimensions: Vec2,
    pub time: f32,
    pub mode: SDFMode
}

/// Modes that `SDFMetadata` can use.
///
/// `Normal` renders shapes using their assigned `SDFStyle` colors, while
/// `HashColor` is a debug mode that assigns each shape a deterministic hash
/// color.
#[repr(u32)]
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum SDFMode {
    #[default]
    Normal = 0,
    HashColor = 1
}

/// A node in the CPU-side UI element tree.
///
/// Elements are laid out in screen space with a `center` and `dimensions`, carry
/// a `SDFStyle` and `SDFShape`, and may own a linked list of `children`.
/// When uploaded via `crate::ui::TreeBuffer`, each node is converted to a
/// `crate::ui::shader::SDFRawShape` and shape-specific detail is stored in
/// `crate::ui::ChunkedBuffer`s. The `handles` field records the GPU chunks written
/// during that conversion and is not set by callers.
#[derive(Default, Getters, Setters, MutGetters, Clone)]
#[getset(get = "pub", set = "pub", get_mut = "pub")]
pub struct SDFElement {
    pub center: Vec2,
    pub dimensions: Vec2,
    pub style: SDFStyle,
    pub shape: SDFShape,
    pub children: Vec<SDFElement>,
    pub handles: CowData<(ChunkHandle<SDFRawStyle>, SDFRawStyleHandle)>
}

impl TreeBufferElement for SDFElement {
    type OutputType = SDFRawShape;

    fn children(&self) -> impl Iterator<Item = &Self> {
        self.children.iter()
    }
}

/// Identifies the GPU chunk that holds shape-specific data for an `SDFElement`.
///
/// Each variant corresponds to an `SDFShape` variant and stores the
/// `ChunkHandle` or raw index the shader uses to look up geometry: `Empty`
/// has no shape-specific data (empty shape or circle), `Rectangle` points to
/// a rounded-rectangle radii chunk, `Curve` points to a single cubic-bezier
/// curve chunk, `Glyph` points to the bezier curves and glyph header
/// describing a vector glyph, and `Raw` is a buffer index used when no
/// chunked lookup is needed.
#[derive(Clone, Default)]
pub enum SDFRawStyleHandle {
    #[default]
    Empty,
    Rectangle(ChunkHandle<SDFRawRectangle>),
    Curve(ChunkHandle<SDFRawBezier>),
    Glyph(ChunkHandle<SDFRawBezier>, ChunkHandle<SDFRawGlyph>),
    Raw(u32)
}

impl SDFRawStyleHandle {
    /// Returns the 32-bit index the shader uses to fetch this element's shape data.
    ///
    /// `SDFRawStyleHandle::Empty` maps to `u32::MAX`, which the shader treats
    /// as "no pointer".
    pub fn handle_ptr(&self) -> u32 {
        match &self {
            SDFRawStyleHandle::Empty => u32::MAX,
            SDFRawStyleHandle::Rectangle(chunk_handle) => *chunk_handle.start_idx(),
            SDFRawStyleHandle::Curve(chunk_handle) => *chunk_handle.start_idx(),
            SDFRawStyleHandle::Glyph(_, ptr) => *ptr.start_idx(),
            SDFRawStyleHandle::Raw(ptr) => *ptr
        }
    }
}

/// Fill and border colors applied when rasterizing an `SDFElement`.
#[derive(Debug, Getters, Setters, MutGetters, Clone, PartialEq)]
#[getset(get = "pub", set = "pub", get_mut = "pub")]
pub struct SDFStyle {
    pub primary_color: Vec4,
    pub border_color: Vec4,
    pub border_width: f32,
    pub texture: Option<TextureHandle>
}

impl Default for SDFStyle {
    fn default() -> Self {
        Self {
            primary_color: Vec4::ONE,
            border_color: Vec4::ZERO,
            border_width: 5.0,
            texture: None
        }
    }
}

/// Geometry drawn inside an `SDFElement`'s bounding box.
///
/// `Empty` draws no visible geometry (only children are rendered), `Circle`
/// is a filled circle inscribed in the element's dimensions, `Rectangle` is
/// a rounded rectangle with per-corner radii, `Bezier` is a single bezier
/// stroke defined by three control-point offsets, `Glyph` is a vector
/// glyph composed of one or more bezier strokes, and `Wave` is a squiggly line
/// filling the box, as tall as it is.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum SDFShape {
    #[default]
    Empty,
    Circle,
    Rectangle(SDFRectangle),
    Bezier(SDFCurve),
    Glyph(Vec<SDFCurve>),
    Wave
}

/// Per-corner radii of a rounded rectangle.
///
/// Components are ordered `(top-left, top-right, bottom-right, bottom-left)`.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct SDFRectangle {
    pub radii: Vec4
}

/// A bezier stroke relative to an element's center.
///
/// Control points are expressed as offsets from the element center, and
/// `thickness` sets the stroke width in pixels.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct SDFCurve {
    pub a_offset: Vec2,
    pub b_offset: Vec2,
    pub c_offset: Vec2,
    pub thickness: f32
}
