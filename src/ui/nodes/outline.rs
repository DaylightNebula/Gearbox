//! Filled vector shapes for nodes to draw, like font glyphs but built in code.

use std::sync::Arc;

use magician_vgpu::glam::Vec2;

use crate::ui::SDFCurve;

/// A filled shape made of closed contours of quadratic curves, drawn by a node with
/// `Background::Shape` stretched over its content box. Points run from (-0.5, -0.5) at the
/// top left to (0.5, 0.5) at the bottom right. Overlapping contours fill by the nonzero rule,
/// so a contour wound the other way cuts a hole.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline(Arc<[SDFCurve]>);

impl Outline {
    pub fn builder() -> OutlineBuilder { OutlineBuilder::default() }

    /// The curves, in the unit box.
    pub fn curves(&self) -> &[SDFCurve] { &self.0 }

    /// The curves stretched over a box `size` big whose center is `offset` from the element's.
    pub(crate) fn placed(&self, size: Vec2, offset: Vec2) -> Vec<SDFCurve> {
        self.0.iter().map(|curve| SDFCurve {
            a_offset: curve.a_offset * size + offset,
            b_offset: curve.b_offset * size + offset,
            c_offset: curve.c_offset * size + offset,
            thickness: curve.thickness,
        }).collect()
    }
}

/// Draws an `Outline` like a pen: `move_to` starts a contour, `line_to` and `quad_to` extend it
/// and `close` joins it back to its start.
#[derive(Debug, Default, Clone)]
pub struct OutlineBuilder {
    curves: Vec<SDFCurve>,
    start: Vec2,
    cursor: Vec2,
}

impl OutlineBuilder {
    /// Closes the open contour and starts a new one at `point`.
    pub fn move_to(mut self, point: Vec2) -> Self {
        self = self.close();
        self.start = point;
        self.cursor = point;
        self
    }

    /// A straight edge to `point`, skipped if it has no length.
    pub fn line_to(self, point: Vec2) -> Self {
        if point == self.cursor { return self }
        let control = (self.cursor + point) / 2.0;
        self.quad_to(control, point)
    }

    /// A quadratic curve pulled toward `control`, ending at `point`.
    pub fn quad_to(mut self, control: Vec2, point: Vec2) -> Self {
        self.curves.push(SDFCurve { a_offset: self.cursor, b_offset: control, c_offset: point, thickness: 1.0 });
        self.cursor = point;
        self
    }

    /// Joins the open contour back to its start, if it isn't there already.
    pub fn close(self) -> Self {
        if self.cursor == self.start { return self }
        let start = self.start;
        self.line_to(start)
    }

    /// A closed polygon through `points`, each corner rounded by a curve starting and ending
    /// `radius` along its edges, at most halfway.
    pub fn rounded_polygon(mut self, points: &[Vec2], radius: f32) -> Self {
        let n = points.len();
        if n < 3 { return self }
        if radius <= 0.0 {
            self = self.move_to(points[0]);
            for &point in &points[1..] { self = self.line_to(point); }
            return self.close();
        }
        let toward = |from: Vec2, to: Vec2| from + (to - from).clamp_length_max(radius.min(from.distance(to) / 2.0));
        let corner = |i: usize| {
            let point = points[i];
            (toward(point, points[(i + n - 1) % n]), point, toward(point, points[(i + 1) % n]))
        };
        self = self.move_to(corner(0).2);
        for i in (1..n).chain([0]) {
            let (enter, point, leave) = corner(i);
            self = self.line_to(enter).quad_to(point, leave);
        }
        self.close()
    }

    pub fn polygon(self, points: &[Vec2]) -> Self { self.rounded_polygon(points, 0.0) }

    /// A rectangle from `min` to `max` with corners rounded by `radius`.
    pub fn rounded_rect(self, min: Vec2, max: Vec2, radius: f32) -> Self {
        self.rounded_polygon(&[min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)], radius)
    }

    /// A circle, as eight curves.
    pub fn circle(mut self, center: Vec2, radius: f32) -> Self {
        let at = |turn: f32| center + Vec2::from_angle(turn * std::f32::consts::TAU) * radius;
        // the control point of a curve spanning an eighth of a turn, on its middle's ray
        let reach = radius / (std::f32::consts::PI / 8.0).cos();
        let control = |turn: f32| center + Vec2::from_angle(turn * std::f32::consts::TAU) * reach;
        let start = at(0.0);
        self = self.move_to(start);
        for i in 0..8 {
            let turn = i as f32 / 8.0;
            // the last curve ends exactly where the first began, closing the circle
            let end = if i == 7 { start } else { at(turn + 1.0 / 8.0) };
            self = self.quad_to(control(turn + 1.0 / 16.0), end);
        }
        self.close()
    }

    pub fn build(self) -> Outline { Outline(self.close().curves.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every contour ends where it starts and each curve starts where the last ended.
    fn assert_closed(outline: &Outline) {
        let curves = outline.curves();
        assert!(!curves.is_empty());
        let mut start = curves[0].a_offset;
        for pair in curves.windows(2) {
            if pair[1].a_offset != pair[0].c_offset {
                assert_eq!(pair[0].c_offset, start, "contour left open");
                start = pair[1].a_offset;
            }
        }
        assert_eq!(curves[curves.len() - 1].c_offset, start, "last contour left open");
    }

    #[test]
    fn close_joins_back_to_the_start() {
        let outline = Outline::builder().move_to(Vec2::ZERO).line_to(Vec2::X).line_to(Vec2::Y).build();
        assert_eq!(outline.curves().len(), 3);
        assert_closed(&outline);
    }

    #[test]
    fn rounded_shapes_close_and_round_every_corner() {
        let triangle = Outline::builder().rounded_polygon(&[Vec2::new(-0.3, -0.4), Vec2::new(0.4, 0.0), Vec2::new(-0.3, 0.4)], 0.05).build();
        // an edge and a corner curve per point
        assert_eq!(triangle.curves().len(), 6);
        assert_closed(&triangle);

        let bars = Outline::builder()
            .rounded_rect(Vec2::new(-0.3, -0.4), Vec2::new(-0.1, 0.4), 0.05)
            .rounded_rect(Vec2::new(0.1, -0.4), Vec2::new(0.3, 0.4), 0.05)
            .build();
        assert_eq!(bars.curves().len(), 16);
        assert_closed(&bars);
    }

    #[test]
    fn sharp_polygon_is_just_its_edges() {
        let outline = Outline::builder().polygon(&[Vec2::new(-0.5, -0.5), Vec2::new(0.5, 0.0), Vec2::new(-0.5, 0.5)]).build();
        assert_eq!(outline.curves().len(), 3);
        assert_closed(&outline);
    }

    #[test]
    fn corner_radius_stops_halfway_along_short_edges() {
        let outline = Outline::builder().rounded_rect(Vec2::splat(-0.1), Vec2::splat(0.1), 1.0).build();
        for curve in outline.curves() {
            assert!(curve.a_offset.abs().max_element() <= 0.1 + f32::EPSILON);
            assert_ne!(curve.a_offset, curve.c_offset, "zero length curve");
        }
        // the corners meet mid-edge, leaving no straight edges
        assert_eq!(outline.curves().len(), 4);
        assert_closed(&outline);
    }

    #[test]
    fn circle_stays_near_its_radius() {
        let outline = Outline::builder().circle(Vec2::ZERO, 0.5).build();
        assert_eq!(outline.curves().len(), 8);
        assert_closed(&outline);
        for curve in outline.curves() {
            // the curve's middle, where it strays furthest from its ends
            let middle = 0.25 * curve.a_offset + 0.5 * curve.b_offset + 0.25 * curve.c_offset;
            assert!((middle.length() - 0.5).abs() < 0.01);
        }
    }
}
