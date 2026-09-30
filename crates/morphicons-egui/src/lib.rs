//! [egui] widget for [morphicons]: stroke icons that morph into each other
//! with spring physics.
//!
//! Three modes, like the upstream bindings:
//!
//! ```no_run
//! # use morphicons_egui::{MorphIcon, morphicons::{icons, Morph, SpringConfig}};
//! # fn demo(ui: &mut egui::Ui, open: &mut bool, drag: f64, morph: &mut Morph) {
//! // 1. Uncontrolled: pass the icon; changing it animates. State lives in
//! //    egui memory, keyed by the widget's id.
//! let icon = if *open { icons::x() } else { icons::menu() };
//! if ui.add(MorphIcon::new(icon).size(32.0).sense(egui::Sense::click())).clicked() {
//!     *open = !*open;
//! }
//!
//! // 2. Controlled: from → to frozen at `progress` (gestures, sliders). No spring.
//! ui.add(MorphIcon::between(icons::play(), icons::pause(), drag));
//!
//! // 3. Imperative: you own the `Morph` and call `morph_to` / `set` / `seek`.
//! ui.add(MorphIcon::morph(morph));
//! if ui.button("check").clicked() {
//!     morph.morph_to(icons::check(), SpringConfig::BOUNCY);
//! }
//! # }
//! ```
//!
//! The widget advances the animation from `ui.input(|i| i.time)` and requests
//! repaints only while something is moving.

use std::sync::{Arc, Mutex};

use egui::{
    Color32, Id, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget, pos2,
};
use morphicons::{Controller, Flattener, GRID, Icon, Morph, Point, Polyline, Source, SpringConfig};

pub use morphicons;

/// Default edge length in points.
pub const DEFAULT_SIZE: f32 = 24.0;

/// A morphing stroke icon. See the [crate docs](crate) for the three modes.
#[must_use = "add it with `ui.add(...)`"]
pub struct MorphIcon<'a> {
    driver: Driver<'a>,
    size: f32,
    stroke_width: f32,
    absolute_stroke_width: bool,
    color: Option<Color32>,
    spring: SpringConfig,
    grid: f64,
    id_salt: Option<Id>,
    sense: Sense,
}

enum Driver<'a> {
    Source(Source),
    Morph(&'a mut Morph),
}

impl<'a> MorphIcon<'a> {
    /// Uncontrolled: shows `icon`, and animates whenever a later frame passes
    /// a different one.
    pub fn new(icon: Icon) -> Self {
        Self::with_driver(Driver::Source(Source::Icon(icon)))
    }

    /// Controlled: the morph from `from` to `to` frozen at `progress`
    /// (0 = `from`, 1 = `to`). No spring; drive `progress` yourself.
    pub fn between(from: Icon, to: Icon, progress: f64) -> Self {
        Self::with_driver(Driver::Source(Source::Between { from, to, progress }))
    }

    /// Imperative: draws (and advances) a [`Morph`] you own.
    pub fn morph(morph: &'a mut Morph) -> Self {
        Self::with_driver(Driver::Morph(morph))
    }

    fn with_driver(driver: Driver<'a>) -> Self {
        Self {
            driver,
            size: DEFAULT_SIZE,
            stroke_width: 2.0,
            absolute_stroke_width: false,
            color: None,
            spring: SpringConfig::SNAPPY,
            grid: GRID,
            id_salt: None,
            sense: Sense::hover(),
        }
    }

    /// Edge length in points (default 24).
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Stroke width in icon-grid units (default 2, like Lucide), scaled with
    /// the icon. See [`MorphIcon::absolute_stroke_width`].
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    /// When `true`, [`MorphIcon::stroke_width`] is in screen points and does
    /// not scale with [`MorphIcon::size`].
    pub fn absolute_stroke_width(mut self, absolute: bool) -> Self {
        self.absolute_stroke_width = absolute;
        self
    }

    /// Stroke color. Defaults to the widget foreground color for its
    /// interaction state (so clickable icons react to hover like text does).
    pub fn color(mut self, color: impl Into<Color32>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Spring used when an uncontrolled icon changes (default snappy).
    pub fn spring(mut self, spring: SpringConfig) -> Self {
        self.spring = spring;
        self
    }

    /// Size of the source grid (default 24). Icons on other grids are best
    /// re-gridded once with [`Icon::fit`] instead.
    pub fn grid(mut self, grid: f64) -> Self {
        self.grid = grid;
        self
    }

    /// Stable identity for the uncontrolled/controlled state. Without it the
    /// widget uses its auto id, which is stable as long as the surrounding
    /// layout doesn't change which widgets come before it.
    pub fn id_salt(mut self, salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id_salt = Some(Id::new(salt));
        self
    }

    /// How the icon reacts to input (default hover only). Use
    /// [`Sense::click`] to make it a button.
    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense;
        self
    }
}

type SharedController = Arc<Mutex<Controller>>;

impl Widget for MorphIcon<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), self.sense);
        let now = ui.input(|i| i.time);
        let scale = self.size / self.grid as f32;
        // Flatten to ~0.1 screen points.
        let tolerance = 0.1 / scale.max(1e-3) as f64;

        let (animating, polylines) = match self.driver {
            Driver::Morph(morph) => {
                let animating = morph.update(now);
                (animating, morph.flatten(tolerance))
            }
            Driver::Source(source) => {
                let id = match self.id_salt {
                    Some(salt) => ui.make_persistent_id(salt),
                    None => response.id,
                };
                let shared = ui.ctx().data_mut(|d| {
                    d.get_temp_mut_or_insert_with::<SharedController>(id, || {
                        Arc::new(Mutex::new(Controller::new(source.clone())))
                    })
                    .clone()
                });
                let mut ctrl = shared
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                ctrl.sync(&source, self.spring);
                let animating = ctrl.update(now);
                (animating, ctrl.morph().flatten(tolerance))
            }
        };
        if animating {
            ui.ctx().request_repaint();
        }

        if ui.is_rect_visible(rect) {
            let color = self
                .color
                .unwrap_or_else(|| ui.style().interact(&response).fg_stroke.color);
            let width = if self.absolute_stroke_width {
                self.stroke_width
            } else {
                self.stroke_width * scale
            };
            paint_polylines(ui.painter(), rect, self.grid, &polylines, width, color);
        }
        response
    }
}

/// Paints `icon`'s exact geometry centered in `rect`, with `stroke_width` in
/// icon-grid units (2 is Lucide's default). For static icons (galleries,
/// toolbars) that don't need a widget or any state.
pub fn paint_icon(painter: &Painter, rect: Rect, icon: &Icon, stroke_width: f32, color: Color32) {
    let scale = rect.width().min(rect.height()) / GRID as f32;
    let mut sink = Flattener::new(0.1 / scale.max(1e-3) as f64);
    icon.draw(&mut sink);
    paint_polylines(
        painter,
        rect,
        GRID,
        &sink.polylines,
        stroke_width * scale,
        color,
    );
}

/// Paints a [`Morph`]'s current shape centered in `rect` (no ticking: call
/// [`Morph::update`] yourself). `stroke_width` is in icon-grid units.
pub fn paint_morph(
    painter: &Painter,
    rect: Rect,
    morph: &Morph,
    stroke_width: f32,
    color: Color32,
) {
    let scale = rect.width().min(rect.height()) / GRID as f32;
    let polylines = morph.flatten(0.1 / scale.max(1e-3) as f64);
    paint_polylines(painter, rect, GRID, &polylines, stroke_width * scale, color);
}

/// Maps grid-space polylines onto `rect` (centered, uniform scale) and
/// strokes them with `width` screen points.
fn paint_polylines(
    painter: &Painter,
    rect: Rect,
    grid: f64,
    polylines: &[Polyline],
    width: f32,
    color: Color32,
) {
    let scale = rect.width().min(rect.height()) / grid as f32;
    let origin = rect.center() - Vec2::splat(grid as f32 * scale / 2.0);
    let to_screen = |p: Point| pos2(origin.x + p.x as f32 * scale, origin.y + p.y as f32 * scale);
    let mut shapes = Vec::new();
    for line in polylines {
        stroke_polyline(&mut shapes, line, to_screen, width, color);
    }
    painter.extend(shapes);
}

/// Strokes a polyline in the stroke-icon style: round caps and round joins
/// (egui strokes are mitered, so both are added as dots).
fn stroke_polyline(
    shapes: &mut Vec<Shape>,
    line: &Polyline,
    to_screen: impl Fn(Point) -> Pos2,
    width: f32,
    color: Color32,
) {
    let pts: Vec<Pos2> = line.points.iter().map(|&p| to_screen(p)).collect();
    if pts.len() < 2 {
        if let Some(&p) = pts.first() {
            shapes.push(Shape::circle_filled(p, width / 2.0, color));
        }
        return;
    }
    let r = width / 2.0;
    let n = pts.len();
    // A vertex needs a round join when the path turns there noticeably.
    let turns = |prev: Pos2, at: Pos2, next: Pos2| {
        let (a, b) = (at - prev, next - at);
        let cross = a.x * b.y - a.y * b.x;
        let dot = a.x * b.x + a.y * b.y;
        cross.atan2(dot).abs() > 0.35
    };
    for i in 1..n - 1 {
        if turns(pts[i - 1], pts[i], pts[i + 1]) {
            shapes.push(Shape::circle_filled(pts[i], r, color));
        }
    }
    if line.closed {
        for (prev, at, next) in [
            (pts[n - 2], pts[n - 1], pts[0]),
            (pts[n - 1], pts[0], pts[1]),
        ] {
            if turns(prev, at, next) {
                shapes.push(Shape::circle_filled(at, r, color));
            }
        }
        shapes.push(Shape::closed_line(pts, Stroke::new(width, color)));
    } else {
        shapes.push(Shape::circle_filled(pts[0], r, color));
        shapes.push(Shape::circle_filled(pts[n - 1], r, color));
        shapes.push(Shape::line(pts, Stroke::new(width, color)));
    }
}
