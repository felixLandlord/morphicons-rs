//! [iced] widget for [morphicons]: stroke icons that morph into each other
//! with spring physics.
//!
//! ```no_run
//! use iced::widget::button;
//! use morphicons_iced::{morph_icon, morphicons::icons};
//! # #[derive(Clone)] enum Message { Toggle }
//! # struct App { open: bool }
//! # impl App {
//! fn view(&self) -> iced::Element<'_, Message> {
//!     // Uncontrolled: pass the icon; when a later `view` passes another
//!     // one, the widget animates there on its own. No subscription needed.
//!     let icon = if self.open { icons::x() } else { icons::menu() };
//!     button(morph_icon(icon).size(32.0)).on_press(Message::Toggle).into()
//! }
//! # }
//! ```
//!
//! [`MorphIcon::between`] is the controlled mode (you drive `progress`), and
//! [`MorphIcon::morph`] draws a [`Morph`] you own and tick yourself (see
//! [`seconds`]).
//!
//! It depends on `iced_widget` (not the `iced` facade), so it works with
//! whatever renderer and executor your app picks.
//!
//! The widget is an iced `canvas`: it advances its animation on each
//! `RedrawRequested` event and asks for the next frame only while moving.

use std::cell::RefCell;
use std::sync::OnceLock;

use iced_widget::canvas::{
    self, Canvas, Event, Frame, Geometry, LineCap, LineJoin, Path, Program, Stroke,
};
use iced_widget::core::time::Instant;
use iced_widget::core::{Color, Element, Rectangle, Theme, Vector, mouse, window};
use iced_widget::graphics::geometry;
use morphicons::{Controller, GRID, Icon, Morph, PathSink, Point, Source, SpringConfig};

pub use morphicons;

/// Default edge length in logical pixels.
pub const DEFAULT_SIZE: f32 = 24.0;

/// Uncontrolled icon: shorthand for [`MorphIcon::new`].
pub fn morph_icon<'a>(icon: Icon) -> MorphIcon<'a> {
    MorphIcon::new(icon)
}

/// A morphing stroke icon. Convert it into an [`Element`] (it implements
/// `Into<Element>`), or wrap it in a `button` to make it clickable.
pub struct MorphIcon<'a> {
    driver: Driver<'a>,
    size: f32,
    stroke_width: f32,
    absolute_stroke_width: bool,
    color: Option<Color>,
    spring: SpringConfig,
    grid: f64,
}

enum Driver<'a> {
    Source(Source),
    Morph(&'a Morph),
}

impl<'a> MorphIcon<'a> {
    /// Uncontrolled: shows `icon`, and animates whenever a later `view`
    /// passes a different one. State lives in the widget tree.
    pub fn new(icon: Icon) -> Self {
        Self::with_driver(Driver::Source(Source::Icon(icon)))
    }

    /// Controlled: the morph from `from` to `to` frozen at `progress`
    /// (0 = `from`, 1 = `to`). No spring; drive `progress` from your state.
    pub fn between(from: Icon, to: Icon, progress: f64) -> Self {
        Self::with_driver(Driver::Source(Source::Between { from, to, progress }))
    }

    /// Imperative: draws a [`Morph`] you own. You advance it yourself, e.g.
    /// from `iced::window::frames()` while [`Morph::is_animating`]:
    ///
    /// ```ignore
    /// fn subscription(&self) -> Subscription<Message> {
    ///     if self.morph.is_animating() {
    ///         iced::window::frames().map(Message::Frame)
    ///     } else {
    ///         Subscription::none()
    ///     }
    /// }
    /// // in update: Message::Frame(now) => { self.morph.update(morphicons_iced::seconds(now)); }
    /// ```
    pub fn morph(morph: &'a Morph) -> Self {
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
        }
    }

    /// Edge length in logical pixels (default 24).
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

    /// When `true`, [`MorphIcon::stroke_width`] is in logical pixels and does
    /// not scale with [`MorphIcon::size`].
    pub fn absolute_stroke_width(mut self, absolute: bool) -> Self {
        self.absolute_stroke_width = absolute;
        self
    }

    /// Stroke color. Defaults to the theme's text color.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
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
}

/// Seconds since a process-wide epoch: the clock [`Morph::update`] expects,
/// from the `Instant`s iced hands out (`window::frames()`, redraw events).
pub fn seconds(now: Instant) -> f64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = *EPOCH.get_or_init(|| now);
    now.checked_duration_since(epoch)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Widget-tree state: the controller (uncontrolled/controlled modes) and a
/// geometry cache reused while the icon is at rest.
pub struct State<Renderer: geometry::Renderer> {
    ctrl: Option<Controller>,
    cache: canvas::Cache<Renderer>,
    /// What the cache holds: the at-rest icon and the style it was drawn with.
    cached: RefCell<Option<(Icon, f32, Color)>>,
}

impl<Renderer: geometry::Renderer> Default for State<Renderer> {
    fn default() -> Self {
        Self {
            ctrl: None,
            cache: canvas::Cache::new(),
            cached: RefCell::new(None),
        }
    }
}

impl<Message, Renderer> Program<Message, Theme, Renderer> for MorphIcon<'_>
where
    Renderer: geometry::Renderer + 'static,
{
    type State = State<Renderer>;

    fn update(
        &self,
        state: &mut State<Renderer>,
        event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let (Driver::Source(source), Event::Window(window::Event::RedrawRequested(now))) =
            (&self.driver, event)
        else {
            return None;
        };
        let ctrl = state
            .ctrl
            .get_or_insert_with(|| Controller::new(source.clone()));
        ctrl.sync(source, self.spring);
        ctrl.update(seconds(*now))
            .then(canvas::Action::request_redraw)
    }

    fn draw(
        &self,
        state: &State<Renderer>,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        // Before the first redraw event the state is empty: draw the source.
        let fallback;
        let morph = match (&self.driver, &state.ctrl) {
            (Driver::Morph(morph), _) => *morph,
            (Driver::Source(_), Some(ctrl)) => ctrl.morph(),
            (Driver::Source(source), None) => {
                fallback = Controller::new(source.clone());
                fallback.morph()
            }
        };

        let scale = bounds.width.min(bounds.height) / self.grid as f32;
        let offset = Vector::new(
            (bounds.width - self.grid as f32 * scale) / 2.0,
            (bounds.height - self.grid as f32 * scale) / 2.0,
        );
        let path = Path::new(|builder| {
            morph.draw(&mut Sink {
                builder,
                scale,
                offset,
            });
        });
        let width = if self.absolute_stroke_width {
            self.stroke_width
        } else {
            self.stroke_width * scale
        };
        let color = self.color.unwrap_or_else(|| theme.palette().text);
        let stroke = |frame: &mut Frame<Renderer>| {
            frame.stroke(
                &path,
                Stroke::default()
                    .with_width(width)
                    .with_color(color)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
            );
        };

        // In flight every frame differs: draw fresh. At rest, reuse the
        // cached geometry until the icon or its style changes (the cache
        // itself invalidates on resize).
        if !morph.is_at_rest() {
            *state.cached.borrow_mut() = None;
            let mut frame = Frame::new(renderer, bounds.size());
            stroke(&mut frame);
            return vec![frame.into_geometry()];
        }
        let key = (morph.target().clone(), width, color);
        if state.cached.borrow().as_ref() != Some(&key) {
            state.cache.clear();
            *state.cached.borrow_mut() = Some(key);
        }
        vec![state.cache.draw(renderer, bounds.size(), stroke)]
    }
}

/// Maps icon-grid commands into an iced path builder.
struct Sink<'b> {
    builder: &'b mut canvas::path::Builder,
    scale: f32,
    offset: Vector,
}

impl Sink<'_> {
    fn map(&self, p: Point) -> iced_widget::core::Point {
        iced_widget::core::Point::new(
            p.x as f32 * self.scale + self.offset.x,
            p.y as f32 * self.scale + self.offset.y,
        )
    }
}

impl PathSink for Sink<'_> {
    fn move_to(&mut self, p: Point) {
        let p = self.map(p);
        self.builder.move_to(p);
    }

    fn line_to(&mut self, p: Point) {
        let p = self.map(p);
        self.builder.line_to(p);
    }

    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        let (c1, c2, p) = (self.map(c1), self.map(c2), self.map(p));
        self.builder.bezier_curve_to(c1, c2, p);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

impl<'a, Message, Renderer> From<MorphIcon<'a>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: geometry::Renderer + 'static,
{
    fn from(icon: MorphIcon<'a>) -> Self {
        let size = icon.size;
        Canvas::new(icon).width(size).height(size).into()
    }
}
