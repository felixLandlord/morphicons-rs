use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::normalize::d_to_cubics;
use crate::{
    CORNER_THRESHOLD, CubicPath, Element, Error, GRID, PathSink, Point, SAMPLES, Sampled,
    cubics_to_d, resample_path,
};

/// Parsed icon data: cubic subpaths, their arc-length samples and the
/// canonical `d`, all computed once. Cloning is cheap (reference-counted), so
/// build icons once (at startup, or in a `LazyLock`) and pass clones around.
///
/// Two icons compare equal when they have the same geometry.
#[derive(Clone)]
pub struct Icon(Arc<IconData>);

struct IconData {
    cubics: Vec<CubicPath>,
    samples: Vec<Sampled>,
    d: String,
}

impl Icon {
    /// Parses SVG path data (the `d` attribute).
    pub fn from_d(d: &str) -> Result<Icon, Error> {
        Icon::from_cubics(d_to_cubics(d)?)
    }

    /// Builds an icon from typed primitives.
    pub fn from_elements(elements: &[Element]) -> Result<Icon, Error> {
        let mut cubics = Vec::new();
        for el in elements {
            el.lower(&mut cubics)?;
        }
        Icon::from_cubics(cubics)
    }

    /// Builds an icon from Lucide-style `[tag, attrs]` nodes, the shape of
    /// Lucide's `IconNode` data (also Feather, Tabler and custom sets):
    ///
    /// ```
    /// # use morphicons::Icon;
    /// let menu = Icon::from_nodes(&[
    ///     ("line", &[("x1", "4"), ("y1", "6"), ("x2", "20"), ("y2", "6")]),
    ///     ("line", &[("x1", "4"), ("y1", "12"), ("x2", "20"), ("y2", "12")]),
    ///     ("path", &[("d", "M4 18h16")]),
    /// ])?;
    /// # Ok::<(), morphicons::Error>(())
    /// ```
    pub fn from_nodes(nodes: &[(&str, &[(&str, &str)])]) -> Result<Icon, Error> {
        let elements = nodes
            .iter()
            .map(|(tag, attrs)| Element::from_node(tag, attrs))
            .collect::<Result<Vec<_>, _>>()?;
        Icon::from_elements(&elements)
    }

    /// Builds an icon from already-normalized cubic subpaths.
    pub fn from_cubics(cubics: Vec<CubicPath>) -> Result<Icon, Error> {
        if cubics.is_empty() {
            return Err(Error::Empty);
        }
        let samples = cubics
            .iter()
            .map(|c| {
                Ok(Sampled {
                    points: resample_path(c, SAMPLES, CORNER_THRESHOLD)?,
                    closed: c.closed,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let d = cubics_to_d(&cubics);
        Ok(Icon(Arc::new(IconData { cubics, samples, d })))
    }

    /// Re-grids an icon drawn on `view_box` onto the shared 24 grid, centered
    /// and preserving aspect ratio (SVG `xMidYMid meet`). Both ends of a morph
    /// must live on the same grid; do this once per icon, not per frame.
    pub fn fit(&self, view_box: impl Into<ViewBox>) -> Result<Icon, Error> {
        self.fit_to_grid(view_box, GRID)
    }

    /// [`Icon::fit`] onto a grid of any size.
    pub fn fit_to_grid(&self, view_box: impl Into<ViewBox>, grid: f64) -> Result<Icon, Error> {
        let vb = view_box.into().validated()?;
        let s = (grid / vb.width).min(grid / vb.height);
        let tx = (grid - vb.width * s) / 2.0 - vb.min_x * s;
        let ty = (grid - vb.height * s) / 2.0 - vb.min_y * s;
        let cubics = self
            .cubics()
            .iter()
            .map(|c| CubicPath {
                points: c
                    .points
                    .iter()
                    .map(|p| Point::new(p.x * s + tx, p.y * s + ty))
                    .collect(),
                closed: c.closed,
            })
            .collect();
        Icon::from_cubics(cubics)
    }

    /// The icon as cubic subpaths (exact geometry).
    pub fn cubics(&self) -> &[CubicPath] {
        &self.0.cubics
    }

    /// The icon resampled at [`SAMPLES`] points per subpath.
    pub fn samples(&self) -> &[Sampled] {
        &self.0.samples
    }

    /// Canonical `d`: the exact cubics, quantized to 4 decimals.
    pub fn d(&self) -> &str {
        &self.0.d
    }

    /// Draws the exact geometry.
    pub fn draw(&self, sink: &mut impl PathSink) {
        for path in self.cubics() {
            let Some((&first, rest)) = path.points.split_first() else {
                continue;
            };
            sink.move_to(first);
            for seg in rest.chunks_exact(3) {
                sink.cubic_to(seg[0], seg[1], seg[2]);
            }
            if path.closed {
                sink.close();
            }
        }
    }
}

impl PartialEq for Icon {
    fn eq(&self, other: &Icon) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.d == other.0.d
    }
}

impl Eq for Icon {}

impl fmt::Debug for Icon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Icon").field(&self.0.d).finish()
    }
}

impl FromStr for Icon {
    type Err = Error;
    fn from_str(d: &str) -> Result<Icon, Error> {
        Icon::from_d(d)
    }
}

/// A source view box for [`Icon::fit`]: `24.0`, `"0 0 20 20"` or
/// `[min_x, min_y, width, height]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewBox {
    pub min_x: f64,
    pub min_y: f64,
    pub width: f64,
    pub height: f64,
}

impl ViewBox {
    pub const fn new(min_x: f64, min_y: f64, width: f64, height: f64) -> Self {
        Self {
            min_x,
            min_y,
            width,
            height,
        }
    }

    fn validated(self) -> Result<Self, Error> {
        let ok = self.width > 0.0
            && self.height > 0.0
            && self.width.is_finite()
            && self.height.is_finite()
            && self.min_x.is_finite()
            && self.min_y.is_finite();
        if ok {
            Ok(self)
        } else {
            Err(Error::InvalidViewBox(format!("{self:?}")))
        }
    }
}

impl From<f64> for ViewBox {
    fn from(size: f64) -> Self {
        ViewBox::new(0.0, 0.0, size, size)
    }
}

impl From<[f64; 4]> for ViewBox {
    fn from([x, y, w, h]: [f64; 4]) -> Self {
        ViewBox::new(x, y, w, h)
    }
}

impl FromStr for ViewBox {
    type Err = Error;
    fn from_str(s: &str) -> Result<ViewBox, Error> {
        let nums: Option<Vec<f64>> = s
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .map(|t| t.parse().ok())
            .collect();
        match nums.as_deref() {
            Some(&[x, y, w, h]) => ViewBox::new(x, y, w, h).validated(),
            _ => Err(Error::InvalidViewBox(s.to_string())),
        }
    }
}
