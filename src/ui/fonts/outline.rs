use magician_vgpu::glam::*;
use ttf_parser::OutlineBuilder;

use crate::ui::{SDFCurve, SDFShape};

#[derive(Default)]
pub(crate) struct GlyphBuilder {
    bbox: Option<ttf_parser::Rect>,
    units_per_em: Option<f32>,
    strokes: Vec<SDFShape>,
    cursor: Vec2
}

impl GlyphBuilder {
    pub(crate) fn set_bbox(&mut self, bbox: ttf_parser::Rect) {
        self.bbox = Some(bbox);
    }

    pub(crate) fn set_units_per_em(&mut self, em: f32) {
        self.units_per_em = Some(em);
    }

    pub(crate) fn build(&self, font_size: f32, offset: Vec2) -> SDFShape {
        let mult = font_size / self.units_per_em.expect("Units per em not set");
        let curves = self.strokes.clone()
            .into_iter()
            .filter_map(|shape| match shape {
                SDFShape::Bezier(curve) => {
                    Some(SDFCurve { 
                        a_offset: curve.a_offset * mult + offset, 
                        b_offset: curve.b_offset * mult + offset, 
                        c_offset: curve.c_offset * mult + offset, 
                        thickness: curve.thickness
                    })
                },
                _ => None
            })
            .collect::<Vec<_>>();
        SDFShape::Glyph(curves)
    }
}

impl OutlineBuilder for GlyphBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cursor = Vec2::new(x, -y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let new_point = Vec2::new(x, -y);
        let avg_point = (self.cursor + new_point) / 2.0;
        self.strokes.push(SDFShape::Bezier(
            SDFCurve { 
                a_offset: self.cursor, 
                b_offset: avg_point, 
                c_offset: new_point, 
                thickness: 1.0
            }
        ));
        self.cursor = new_point;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let control_point = Vec2::new(x1, -y1);
        let end_point = Vec2::new(x, -y);

        self.strokes.push(SDFShape::Bezier(
            SDFCurve {
                a_offset: self.cursor, 
                b_offset: control_point, 
                c_offset: end_point, 
                thickness: 1.0
            }
        ));
        self.cursor = end_point;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        // todo store cubic instead of quadratic bezier

        let control_point = (3.0 * (Vec2::new(x1, y1) + Vec2::new(x2, y2)) - self.cursor - Vec2::new(x, y)) / 4.0;
        let end_point = Vec2::new(x, y);

        self.strokes.push(SDFShape::Bezier(
            SDFCurve {
                a_offset: self.cursor, 
                b_offset: control_point, 
                c_offset: end_point, 
                thickness: 1.0
            }
        ));
        self.cursor = end_point;
    }

    fn close(&mut self) {}
}
