use crate::anyhow;
use bytemuck::{Pod, Zeroable};
use crate::{AssetVault, AtlasTextureVault, BindlessArrayTextureVault, TextureHandle};
use magician_vgpu::{ChunkedBufferContent, TreeBufferContent, VirtualGpu};
use ordered_float::OrderedFloat;

use crate::ui::{SDFElement, SDFRawStyleHandle, SDFShape, UIRenderResources};

const HALF_MASK: u32 = 0xFFFF;
const HALF_NONE: u32 = 0xFFFF;

/// This is a data container meant to be uploaded to the buffers
/// for use in the UISDFShader for rendering.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct SDFRawShaderData {
    pub metadata: SDFRawMetadata,
    pub shapes: [SDFRawShape; 1000],
    pub styles: [SDFRawStyle; 1000],
    pub rectangles: [SDFRawRectangle; 1000],
    pub bezier: [SDFRawBezier; 1000],
    pub glyphs: [SDFRawGlyph; 1000]
}

/// Raw data associated with the shaders implementation of SDFMetadata
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawMetadata {
    pub screen_dimensions: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub time: OrderedFloat<f32>,
    pub mode: u32
}

unsafe impl Pod for SDFRawMetadata {}
unsafe impl Zeroable for SDFRawMetadata {}

/// Raw data associated with the shaders implementation of SDFShape.
///
/// `shape_ty` is the shape type ID (`0` empty, `1` circle, `2` rectangle,
/// `3` bezier, `4` glyph, `5` wave). `looks_ptrs` packs shape and style indices in the
/// high and low 16 bits respectively. `next_ptrs` packs the next-sibling and
/// first-child tree links in the high and low 16 bits respectively.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawShape {
    pub center: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub dimensions: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub shape_ty: u32,
    pub looks_ptrs: u32,
    pub next_ptrs: u32,
    pub _pad0: u32
}

unsafe impl Pod for SDFRawShape {}
unsafe impl Zeroable for SDFRawShape {}

impl Default for SDFRawShape {
    fn default() -> Self {
        Self {
            center: (0.0.into(), 0.0.into()),
            dimensions: (0.0.into(), 0.0.into()),
            shape_ty: 0,
            looks_ptrs: u32::MAX,
            next_ptrs: u32::MAX,
            _pad0: 0
        }
    }
}

impl TreeBufferContent for SDFRawShape {
    type ConvertInput<'a> = (&'a UIRenderResources, &'a BindlessArrayTextureVault, &'a AtlasTextureVault);
    type InputType = SDFElement;

    fn new_gpu_type<'a>(vgpu: &VirtualGpu, rust: &Self::InputType, input: &'a Self::ConvertInput<'a>, next_ptr: u32, first_child_ptr: u32) -> anyhow::Result<Self> {
        let (input, bindless_vault, atlas_vault) = input;

        let shape_ty = match &rust.shape {
            SDFShape::Empty => 0,
            SDFShape::Circle => 1,
            SDFShape::Rectangle(_sdfrectangle) => 2,
            SDFShape::Bezier(_sdfcurve) => 3,
            SDFShape::Glyph(_sdfcurves) => 4,
            SDFShape::Wave => 5
        };

        let shape_ptr = match &rust.shape {
            SDFShape::Rectangle(sdfrectangle) => 
                SDFRawStyleHandle::Rectangle(input.rectangles_buffer().get(
                    vgpu, 
                    Box::new([
                        SDFRawRectangle { 
                            radii: (
                                sdfrectangle.radii.x.into(),
                                sdfrectangle.radii.y.into(),
                                sdfrectangle.radii.z.into(),
                                sdfrectangle.radii.w.into()
                            )
                        }
                    ])
                )?),
            SDFShape::Bezier(sdfcurve) => 
                SDFRawStyleHandle::Curve(input.bezier_buffer.get(
                    vgpu, 
                    Box::new([
                        SDFRawBezier {
                            a_off: (sdfcurve.a_offset.x.into(), sdfcurve.a_offset.y.into()),
                            b_off: (sdfcurve.b_offset.x.into(), sdfcurve.b_offset.y.into()),
                            c_off: (sdfcurve.c_offset.x.into(), sdfcurve.c_offset.y.into()),
                            thickness: sdfcurve.thickness.into(),
                            _pad0: 0
                        }
                    ])
                )?),
            SDFShape::Glyph(sdfcurves) => {
                // create curve chunk and handle
                let chunk: Box<[SDFRawBezier]> = sdfcurves.iter()
                    .map(|sdfcurve| SDFRawBezier {
                        a_off: (sdfcurve.a_offset.x.into(), sdfcurve.a_offset.y.into()),
                        b_off: (sdfcurve.b_offset.x.into(), sdfcurve.b_offset.y.into()),
                        c_off: (sdfcurve.c_offset.x.into(), sdfcurve.c_offset.y.into()),
                        thickness: sdfcurve.thickness.into(),
                        _pad0: 0
                    }).collect();
                let chunk = input.bezier_buffer.get(vgpu, chunk)?;

                // create glyph handle
                let glyph = input.glyphs_buffer.get(
                    vgpu, 
                    Box::new([
                        SDFRawGlyph {
                            start_idx: *chunk.start_idx(),
                            length: *chunk.size(),
                            _pad0: 0,
                            _pad1: 0
                        }
                    ])
                )?;

                SDFRawStyleHandle::Glyph(chunk, glyph)
            },
            _ => SDFRawStyleHandle::Empty
        };

        let texture_ptr = match rust.style.texture.as_ref() {
            Some(TextureHandle::Bindless(handle)) => bindless_vault.get(handle)
                .map(|a| *a.texture_idx() as u32)
                .unwrap_or(u32::MAX),
            Some(TextureHandle::Atlas(handle)) => atlas_vault.get(handle)
                .and_then(|a| {
                    let rect = SDFRawTextureRect {
                        rect: (
                            (a.offset_px().x as f32).into(), (a.offset_px().y as f32).into(),
                            (a.size_px().x as f32).into(), (a.size_px().y as f32).into()
                        )
                    };
                    input.texture_rects_buffer().get(vgpu, Box::new([rect])).ok()
                })
                .map(|chunk| *chunk.start_idx() as u32)
                .unwrap_or(u32::MAX),
            None => u32::MAX
        };

        let style_ptr = input.styles_buffer().get(
            vgpu, 
            Box::new([
                SDFRawStyle {
                    primary_color: (rust.style.primary_color.x.into(), rust.style.primary_color.y.into(), rust.style.primary_color.z.into(), rust.style.primary_color.w.into()),
                    border_color: (rust.style.border_color.x.into(), rust.style.border_color.y.into(), rust.style.border_color.z.into(), rust.style.border_color.w.into()),
                    border_width: rust.style.border_width.into(),
                    texture_ptr,
                    _padding: (0, 0)
                }
            ])
        )?;

        let looks_ptrs = pack_u32(
            shape_ptr.handle_ptr() as u16,
            *style_ptr.start_idx() as u16
        );
        rust.handles.set((style_ptr, shape_ptr));

        Ok(Self {
            center: (rust.center.x.into(), rust.center.y.into()),
            dimensions: (rust.dimensions.x.into(), rust.dimensions.y.into()),
            shape_ty, looks_ptrs,
            next_ptrs: (pack_half(next_ptr) << 16) | pack_half(first_child_ptr),
            _pad0: 0,
        })
    }

    fn set_next_ptr(&mut self, ptr: u32) {
        self.next_ptrs = (self.next_ptrs & HALF_MASK) | (pack_half(ptr) << 16);
    }

    fn set_child_ptr(&mut self, ptr: u32) {
        self.next_ptrs = (self.next_ptrs & (HALF_MASK << 16)) | pack_half(ptr);
    }
}

/// Raw data associated with the shaders implementation of SDFStyle.
///
/// `texture_ptr` is an index into a texture buffer, or `u32::MAX` when no
/// texture is bound.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawStyle {
    pub primary_color: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>),
    pub border_color: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>),
    pub border_width: OrderedFloat<f32>,
    pub texture_ptr: u32,
    pub _padding: (u32, u32)
}

impl Default for SDFRawStyle {
    fn default() -> Self {
        Self {
            primary_color: (1.0.into(), 1.0.into(), 1.0.into(), 0.0.into()),
            border_color: (1.0.into(), 1.0.into(), 1.0.into(), 0.0.into()),
            border_width: 0.0.into(),
            texture_ptr: u32::MAX,
            _padding: (0, 0),
        }
    }
}

unsafe impl Pod for SDFRawStyle {}
unsafe impl Zeroable for SDFRawStyle {}
impl ChunkedBufferContent for SDFRawStyle {}

/// Raw data associated with the shaders implementation of SDFRectangle
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawRectangle {
    pub radii: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>)
}

unsafe impl Pod for SDFRawRectangle {}
unsafe impl Zeroable for SDFRawRectangle {}
impl ChunkedBufferContent for SDFRawRectangle {}

/// Raw data associated with the shaders implementation of SDFBezier
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawBezier {
    pub a_off: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub b_off: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub c_off: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub thickness: OrderedFloat<f32>,
    pub _pad0: u32
}

unsafe impl Pod for SDFRawBezier {}
unsafe impl Zeroable for SDFRawBezier {}
impl ChunkedBufferContent for SDFRawBezier {}

/// Raw data associated with the shaders implementation of SDFGlyph.
///
/// `start_idx` is the index of the first bezier curve in this glyph, and
/// `length` is the number of curves that follow.
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawGlyph {
    pub start_idx: u32,
    pub length: u32,
    pub _pad0: u32,
    pub _pad1: u32
}

unsafe impl Pod for SDFRawGlyph {}
unsafe impl Zeroable for SDFRawGlyph {}
impl ChunkedBufferContent for SDFRawGlyph {}

/// A texture's placement rect in atlas-page pixel space: `(offset_x, offset_y, size_x,
/// size_y)`. Only used (and only meaningful) when the atlas texture backend is active --
/// see [`crate::AtlasTextureVault`].
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawTextureRect {
    pub rect: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>)
}

unsafe impl Pod for SDFRawTextureRect {}
unsafe impl Zeroable for SDFRawTextureRect {}
impl ChunkedBufferContent for SDFRawTextureRect {}

/// Pack a (possibly u32::MAX) pointer into its lower 16 bits, mapping
/// u32::MAX -> 0xFFFF (the "no pointer" sentinel for a half).
#[inline]
fn pack_half(ptr: u32) -> u32 {
    if ptr == u32::MAX {
        HALF_NONE
    } else {
        debug_assert!(
            ptr <= HALF_MASK,
            "pointer {ptr} does not fit in 16 bits used by SDFRawShape::next_ptrs"
        );
        ptr & HALF_MASK
    }
}

/// Combine two 16-bit values into a single 32-bit packed field.
fn pack_u32(val1: u16, val2: u16) -> u32 {
    ((val1 as u32) << 16) | (val2 as u32)
}
