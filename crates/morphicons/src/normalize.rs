//! Normalization: any SVG primitive → cubic Bézier segments.
//! Line → collinear controls at ⅓ and ⅔. Quadratic → exact degree elevation.
//! Arc → center parametrization (SVG spec F.6), slices ≤ 90°,
//! α = 4/3·tan(Δθ/4). circle/ellipse/rect/polyline/polygon → lines and
//! quarter ellipses.

use std::f64::consts::{FRAC_PI_2, PI, SQRT_2, TAU};

use crate::parse::{RawSeg, RawSubpath, parse_path};
use crate::{Error, Point};

/// Control-point offset for a quarter circle: (4/3)·tan(π/8) ≈ 0.5523.
pub const KAPPA: f64 = 4.0 / 3.0 * (SQRT_2 - 1.0);

/// A subpath as a chain of cubics packed as points
/// `[p0, c1, c2, p1, c1', c2', p2, …]` (length `3m + 1` for `m` segments).
/// Consecutive segments share an endpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct CubicPath {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl CubicPath {
    /// Number of cubic segments.
    pub fn segment_count(&self) -> usize {
        self.points.len().saturating_sub(1) / 3
    }

    /// The four control points of segment `k`.
    pub fn segment(&self, k: usize) -> &[Point] {
        &self.points[3 * k..3 * k + 4]
    }
}

/// A typed icon primitive: the seven stroke elements an icon may use.
/// Coordinates are literal (no transforms, no groups).
#[derive(Clone, Debug, PartialEq)]
pub enum Element {
    /// SVG path data (`d` attribute).
    Path(String),
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
    },
    Ellipse {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
    },
    /// `rx`/`ry` follow SVG rules: a missing one copies the other; both are
    /// clamped to half the side.
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        rx: Option<f64>,
        ry: Option<f64>,
    },
    Polyline(Vec<Point>),
    Polygon(Vec<Point>),
}

impl Element {
    /// Builds an element from a Lucide-style `[tag, attrs]` node. Missing or
    /// non-numeric attributes fall back to 0, like the browser does.
    pub fn from_node(tag: &str, attrs: &[(&str, &str)]) -> Result<Element, Error> {
        let get = |key: &str| attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.trim());
        let num = |key: &str| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let n = |key: &str| num(key).unwrap_or(0.0);
        Ok(match tag {
            "path" => Element::Path(get("d").unwrap_or("").to_string()),
            "line" => Element::Line {
                x1: n("x1"),
                y1: n("y1"),
                x2: n("x2"),
                y2: n("y2"),
            },
            "circle" => Element::Circle {
                cx: n("cx"),
                cy: n("cy"),
                r: n("r"),
            },
            "ellipse" => Element::Ellipse {
                cx: n("cx"),
                cy: n("cy"),
                rx: n("rx"),
                ry: n("ry"),
            },
            "rect" => Element::Rect {
                x: n("x"),
                y: n("y"),
                width: n("width"),
                height: n("height"),
                rx: num("rx"),
                ry: num("ry"),
            },
            "polyline" => Element::Polyline(parse_points(get("points").unwrap_or(""))?),
            "polygon" => Element::Polygon(parse_points(get("points").unwrap_or(""))?),
            other => return Err(Error::UnsupportedTag(other.to_string())),
        })
    }

    /// Appends this element's subpaths, as cubics, to `out`.
    pub(crate) fn lower(&self, out: &mut Vec<CubicPath>) -> Result<(), Error> {
        match self {
            Element::Path(d) => out.extend(d_to_cubics(d)?),
            &Element::Line { x1, y1, x2, y2 } => {
                let mut b = Builder::new(Point::new(x1, y1));
                b.line(Point::new(x2, y2));
                out.extend(b.finish(false));
            }
            &Element::Circle { cx, cy, r } => out.extend(ellipse_path(cx, cy, r, r)),
            &Element::Ellipse { cx, cy, rx, ry } => out.extend(ellipse_path(cx, cy, rx, ry)),
            &Element::Rect {
                x,
                y,
                width,
                height,
                rx,
                ry,
            } => {
                out.extend(rect_path(x, y, width, height, rx, ry));
            }
            Element::Polyline(pts) => out.extend(poly_path(pts, false)),
            Element::Polygon(pts) => out.extend(poly_path(pts, true)),
        }
        Ok(())
    }
}

fn parse_points(s: &str) -> Result<Vec<Point>, Error> {
    let nums = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect::<Option<Vec<f64>>>()
        .ok_or_else(|| Error::InvalidPoints(s.to_string()))?;
    Ok(nums
        .chunks_exact(2)
        .map(|c| Point::new(c[0], c[1]))
        .collect())
}

/// Cubic accumulator for one subpath.
struct Builder {
    pts: Vec<Point>,
    cur: Point,
}

impl Builder {
    fn new(start: Point) -> Self {
        Self {
            pts: vec![start],
            cur: start,
        }
    }

    fn cubic(&mut self, c1: Point, c2: Point, p: Point) {
        self.pts.extend([c1, c2, p]);
        self.cur = p;
    }

    fn line(&mut self, p: Point) {
        let c = self.cur;
        if (p.x - c.x).abs() < 1e-12 && (p.y - c.y).abs() < 1e-12 {
            return; // degenerate
        }
        self.cubic(
            Point::new(c.x + (p.x - c.x) / 3.0, c.y + (p.y - c.y) / 3.0),
            Point::new(
                c.x + (2.0 * (p.x - c.x)) / 3.0,
                c.y + (2.0 * (p.y - c.y)) / 3.0,
            ),
            p,
        );
    }

    fn quad(&mut self, q: Point, p: Point) {
        let c = self.cur;
        self.cubic(
            Point::new(
                c.x + (2.0 / 3.0) * (q.x - c.x),
                c.y + (2.0 / 3.0) * (q.y - c.y),
            ),
            Point::new(
                p.x + (2.0 / 3.0) * (q.x - p.x),
                p.y + (2.0 / 3.0) * (q.y - p.y),
            ),
            p,
        );
    }

    /// Elliptical arc → cubics. Endpoint → center per SVG spec, appendix F.6.
    fn arc(&mut self, rx: f64, ry: f64, rotation: f64, large: bool, sweep: bool, to: Point) {
        let Point { x: x1, y: y1 } = self.cur;
        let Point { x, y } = to;
        if (x - x1).abs() < 1e-12 && (y - y1).abs() < 1e-12 {
            return; // F.6.2
        }
        let mut rx = rx.abs();
        let mut ry = ry.abs();
        if rx < 1e-12 || ry < 1e-12 {
            self.line(to); // F.6.6: zero radius → line
            return;
        }
        let phi = rotation * PI / 180.0;
        let (sin_p, cos_p) = phi.sin_cos();
        let hx = (x1 - x) / 2.0;
        let hy = (y1 - y) / 2.0;
        let x1p = cos_p * hx + sin_p * hy;
        let y1p = -sin_p * hx + cos_p * hy;
        // F.6.6: scale up insufficient radii
        let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
        if lam > 1.0 {
            let s = lam.sqrt();
            rx *= s;
            ry *= s;
        }
        // F.6.5: center
        let (rx2, ry2, xp2, yp2) = (rx * rx, ry * ry, x1p * x1p, y1p * y1p);
        let rad = ((rx2 * ry2 - rx2 * yp2 - ry2 * xp2) / (rx2 * yp2 + ry2 * xp2)).max(0.0);
        let co = if large == sweep { -1.0 } else { 1.0 } * rad.sqrt();
        let cxp = (co * rx * y1p) / ry;
        let cyp = (-co * ry * x1p) / rx;
        let ccx = cos_p * cxp - sin_p * cyp + (x1 + x) / 2.0;
        let ccy = sin_p * cxp + cos_p * cyp + (y1 + y) / 2.0;
        let th1 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
        let mut dth = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx) - th1;
        if !sweep && dth > 0.0 {
            dth -= TAU;
        } else if sweep && dth < 0.0 {
            dth += TAU;
        }
        // Slice into arcs ≤ 90°, each slice to a cubic with α = 4/3·tan(δ/4)
        let slices = ((dth.abs() / FRAC_PI_2 - 1e-9).ceil() as usize).max(1);
        let delta = dth / slices as f64;
        let alpha = (4.0 / 3.0) * (delta / 4.0).tan();
        let at = |t: f64| {
            Point::new(
                ccx + rx * t.cos() * cos_p - ry * t.sin() * sin_p,
                ccy + rx * t.cos() * sin_p + ry * t.sin() * cos_p,
            )
        };
        let deriv = |t: f64| {
            Point::new(
                -rx * t.sin() * cos_p - ry * t.cos() * sin_p,
                -rx * t.sin() * sin_p + ry * t.cos() * cos_p,
            )
        };
        let mut t0 = th1;
        let mut p0 = self.cur;
        for s in 1..=slices {
            let t1 = th1 + delta * s as f64;
            let p1 = if s == slices { to } else { at(t1) }; // exact final endpoint
            let d0 = deriv(t0);
            let d1 = deriv(t1);
            self.cubic(
                Point::new(p0.x + alpha * d0.x, p0.y + alpha * d0.y),
                Point::new(p1.x - alpha * d1.x, p1.y - alpha * d1.y),
                p1,
            );
            t0 = t1;
            p0 = p1;
        }
    }

    fn finish(mut self, closed: bool) -> Option<CubicPath> {
        if closed {
            self.line(self.pts[0]); // explicit closing segment if needed
        }
        (self.pts.len() >= 4).then_some(CubicPath {
            points: self.pts,
            closed,
        })
    }
}

fn lower_subpath(raw: &RawSubpath) -> Option<CubicPath> {
    let mut b = Builder::new(raw.start);
    for seg in &raw.segs {
        match *seg {
            RawSeg::Line(p) => b.line(p),
            RawSeg::Cubic(c1, c2, p) => b.cubic(c1, c2, p),
            RawSeg::Quad(q, p) => b.quad(q, p),
            RawSeg::Arc {
                rx,
                ry,
                rotation,
                large,
                sweep,
                to,
            } => b.arc(rx, ry, rotation, large, sweep, to),
        }
    }
    b.finish(raw.closed)
}

fn poly_path(pts: &[Point], closed: bool) -> Option<CubicPath> {
    let (&first, rest) = pts.split_first()?;
    if rest.is_empty() {
        return None;
    }
    let mut b = Builder::new(first);
    for &p in rest {
        b.line(p);
    }
    b.finish(closed)
}

fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> Option<CubicPath> {
    if rx < 1e-12 || ry < 1e-12 {
        return None;
    }
    let kx = KAPPA * rx;
    let ky = KAPPA * ry;
    let (e, w, s, n) = (cx + rx, cx - rx, cy + ry, cy - ry);
    let p = Point::new;
    let mut b = Builder::new(p(e, cy));
    b.cubic(p(e, cy + ky), p(cx + kx, s), p(cx, s));
    b.cubic(p(cx - kx, s), p(w, cy + ky), p(w, cy));
    b.cubic(p(w, cy - ky), p(cx - kx, n), p(cx, n));
    b.cubic(p(cx + kx, n), p(e, cy - ky), p(e, cy));
    b.finish(true)
}

fn rect_path(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rx: Option<f64>,
    ry: Option<f64>,
) -> Option<CubicPath> {
    if w < 1e-12 || h < 1e-12 {
        return None;
    }
    // SVG rules: rx/ry copy each other when only one is given; clamp to half the side.
    let (rx, ry) = match (rx, ry) {
        (None, None) => (0.0, 0.0),
        (Some(r), None) | (None, Some(r)) => (r, r),
        (Some(rx), Some(ry)) => (rx, ry),
    };
    let rx = rx.max(0.0).min(w / 2.0);
    let ry = ry.max(0.0).min(h / 2.0);
    let p = Point::new;
    if rx < 1e-12 || ry < 1e-12 {
        return poly_path(&[p(x, y), p(x + w, y), p(x + w, y + h), p(x, y + h)], true);
    }
    // Coordinates of the straight↔arc joints of each rounded corner.
    let (xa, xb, xr) = (x + rx, x + w - rx, x + w);
    let (ya, yb, yd) = (y + ry, y + h - ry, y + h);
    let kx = KAPPA * rx;
    let ky = KAPPA * ry;
    let mut b = Builder::new(p(xa, y));
    b.line(p(xb, y));
    b.cubic(p(xb + kx, y), p(xr, ya - ky), p(xr, ya));
    b.line(p(xr, yb));
    b.cubic(p(xr, yb + ky), p(xb + kx, yd), p(xb, yd));
    b.line(p(xa, yd));
    b.cubic(p(xa - kx, yd), p(x, yb + ky), p(x, yb));
    b.line(p(x, ya));
    b.cubic(p(x, ya - ky), p(xa - kx, y), p(xa, y));
    b.finish(true)
}

/// Parses a `d` string into cubic subpaths.
pub(crate) fn d_to_cubics(d: &str) -> Result<Vec<CubicPath>, Error> {
    Ok(parse_path(d)?.iter().filter_map(lower_subpath).collect())
}
