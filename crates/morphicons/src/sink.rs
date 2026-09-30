//! Output contract: anything that accepts path commands. This is the Rust
//! counterpart of upstream's `PathEl`: each binding implements it for its
//! framework's path builder (or uses [`Flattener`] when the framework only
//! draws polylines).

use crate::Point;
use crate::resample::bezier_point;

/// Receives path commands in icon-grid coordinates. Map them to screen space
/// inside the implementation.
pub trait PathSink {
    fn move_to(&mut self, p: Point);
    fn line_to(&mut self, p: Point);
    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point);
    /// Closes the current subpath.
    fn close(&mut self);
}

/// A flattened subpath.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    pub points: Vec<Point>,
    pub closed: bool,
}

/// A [`PathSink`] that flattens cubics into polylines, for renderers that only
/// stroke line strips. Closed polylines don't repeat their first point.
#[derive(Clone, Debug, PartialEq)]
pub struct Flattener {
    tolerance: f64,
    pub polylines: Vec<Polyline>,
}

impl Flattener {
    /// `tolerance` is the maximum distance, in icon-grid units, between a
    /// curve and its flattening. For an icon drawn at `px` pixels on the 24
    /// grid, `0.1 * 24.0 / px` keeps the error around a tenth of a pixel.
    pub fn new(tolerance: f64) -> Self {
        Self {
            tolerance: tolerance.max(1e-6),
            polylines: Vec::new(),
        }
    }

    fn current(&mut self) -> &mut Polyline {
        if self.polylines.is_empty() {
            self.polylines.push(Polyline::default());
        }
        self.polylines.last_mut().expect("just ensured")
    }
}

impl PathSink for Flattener {
    fn move_to(&mut self, p: Point) {
        self.polylines.push(Polyline {
            points: vec![p],
            closed: false,
        });
    }

    fn line_to(&mut self, p: Point) {
        self.current().points.push(p);
    }

    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        let tolerance = self.tolerance;
        let line = self.current();
        let p0 = line.points.last().copied().unwrap_or(c1);
        // Wang's formula: segments needed so the chord error stays ≤ tolerance.
        let dd = |a: Point, b: Point, c: Point| (a - b * 2.0 + c).length_squared().sqrt();
        let m = dd(p0, c1, c2).max(dd(c1, c2, p));
        let segs = ((0.75 * m / tolerance).sqrt().ceil() as usize).clamp(1, 64);
        let seg = [p0, c1, c2, p];
        for i in 1..segs {
            line.points.push(bezier_point(&seg, i as f64 / segs as f64));
        }
        line.points.push(p);
    }

    fn close(&mut self) {
        let line = self.current();
        line.closed = true;
        if line.points.len() > 1 {
            let (first, last) = (line.points[0], line.points[line.points.len() - 1]);
            if first.distance(last) < 1e-9 {
                line.points.pop();
            }
        }
    }
}
