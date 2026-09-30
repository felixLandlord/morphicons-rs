use crate::{
    Flattener, Icon, PathSink, Plan, Point, Polyline, Sampled, Spring, SpringConfig, polylines_to_d,
};

/// Longest step a single frame may advance the spring (a backgrounded window
/// resuming must not teleport the animation).
const MAX_DT: f64 = 0.1;

/// The morph driver: one animated icon.
///
/// It owns the spring, re-plans mid-flight on interruptions (preserving
/// velocity), supports frozen seeks for gestures, and snaps to the target's
/// exact curves when it settles. It never owns a clock: call
/// [`Morph::update`] (absolute time) or [`Morph::tick`] (delta) once per
/// frame, and keep requesting frames while they return `true`.
#[derive(Clone, Debug)]
pub struct Morph {
    target: Icon,
    /// `None` at rest: the target's exact cubics are what's shown.
    flight: Option<Flight>,
    spring: Spring,
    animating: bool,
    t: f64,
    clock: Option<f64>,
    reduced_motion: bool,
}

#[derive(Clone, Debug)]
struct Flight {
    plan: Plan,
    out: Vec<Vec<Point>>,
}

impl Morph {
    /// A morph at rest on `icon`.
    pub fn new(icon: Icon) -> Self {
        Self {
            target: icon,
            flight: None,
            spring: Spring::default(),
            animating: false,
            t: 1.0,
            clock: None,
            reduced_motion: false,
        }
    }

    /// Animates toward `icon`. Interruptible: mid-flight it re-plans from the
    /// current intermediate shape while preserving velocity, so rapid toggles
    /// never jump. A no-op if already at, or flying to, `icon`.
    pub fn morph_to(&mut self, icon: Icon, spring: SpringConfig) {
        if icon == self.target && (self.flight.is_none() || self.animating) {
            return;
        }
        if self.reduced_motion {
            self.set(icon);
            return;
        }
        self.spring.config = spring;
        self.retarget(icon);
        self.spring.start();
        self.render(self.spring.x);
        if !self.animating {
            self.animating = true;
            self.clock = None; // first frame gets dt = 0: paint, don't jump
        }
    }

    /// Jumps to `icon` without animating (cancels any flight).
    pub fn set(&mut self, icon: Icon) {
        self.animating = false;
        self.target = icon;
        self.settle();
    }

    /// Shows the morph toward `icon` frozen at `t` (no spring): the
    /// controlled-mode primitive for scrubbing, gestures and scroll. A later
    /// [`Morph::morph_to`] takes off from that intermediate shape.
    pub fn seek(&mut self, icon: Icon, t: f64) {
        let reuse = self.flight.is_some() && icon == self.target;
        self.animating = false;
        self.spring.v = 0.0; // controlled mode: no inherited velocity
        if !reuse {
            self.retarget(icon);
        }
        self.render(t);
    }

    /// Current progress: t of the last frame, 1 at rest.
    pub fn progress(&self) -> f64 {
        if self.flight.is_none() { 1.0 } else { self.t }
    }

    /// Equivalent to `seek(target, t)` on the current target.
    pub fn set_progress(&mut self, t: f64) {
        self.seek(self.target.clone(), t);
    }

    /// Advances the animation by `dt` seconds. Returns `true` while it is
    /// still moving (request another frame).
    pub fn tick(&mut self, dt: f64) -> bool {
        if !self.animating {
            return false;
        }
        let settled = self.spring.step(dt.clamp(0.0, MAX_DT));
        self.render(self.spring.x);
        if settled {
            self.animating = false;
            self.settle();
        }
        self.animating
    }

    /// Advances the animation to absolute time `now` (seconds, any epoch;
    /// e.g. egui's `input.time`). Handles dt itself: the first frame of a
    /// flight advances 0. Returns `true` while still moving.
    pub fn update(&mut self, now: f64) -> bool {
        let dt = self.clock.map_or(0.0, |last| now - last);
        self.clock = Some(now);
        self.tick(dt)
    }

    /// True while the spring is running.
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// True when showing the target's exact geometry (no flight, no seek).
    pub fn is_at_rest(&self) -> bool {
        self.flight.is_none()
    }

    /// The icon this morph shows at rest or is flying toward.
    pub fn target(&self) -> &Icon {
        &self.target
    }

    /// When `true`, [`Morph::morph_to`] jumps instead of animating. Feed it
    /// your platform's reduce-motion setting if you want to honor it.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
    }

    pub fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    /// The live plan, while in flight or seeking (introspection).
    pub fn plan(&self) -> Option<&Plan> {
        self.flight.as_ref().map(|f| &f.plan)
    }

    /// Draws the current shape: exact cubics at rest, polylines in flight.
    pub fn draw(&self, sink: &mut impl PathSink) {
        let Some(f) = &self.flight else {
            self.target.draw(sink);
            return;
        };
        for (pts, item) in f.out.iter().zip(f.plan.items()) {
            let Some((&first, rest)) = pts.split_first() else {
                continue;
            };
            sink.move_to(first);
            for &p in rest {
                sink.line_to(p);
            }
            if item.closed() {
                sink.close();
            }
        }
    }

    /// The current shape as SVG path data.
    pub fn path_d(&self) -> String {
        match &self.flight {
            None => self.target.d().to_string(),
            Some(f) => {
                let closed: Vec<bool> = f.plan.items().iter().map(|it| it.closed()).collect();
                polylines_to_d(&f.out, &closed)
            }
        }
    }

    /// The current shape as polylines, flattening curves to `tolerance`
    /// icon-grid units.
    pub fn flatten(&self, tolerance: f64) -> Vec<Polyline> {
        let mut f = Flattener::new(tolerance);
        self.draw(&mut f);
        f.polylines
    }

    fn retarget(&mut self, icon: Icon) {
        let plan = match &self.flight {
            None => Plan::new(self.target.samples(), icon.samples()),
            // Mid-flight: the rendered buffers (already N points per subpath)
            // are a valid plan source.
            Some(f) => {
                let snapshot: Vec<Sampled> = f
                    .out
                    .iter()
                    .zip(f.plan.items())
                    .map(|(pts, it)| Sampled {
                        points: pts.clone(),
                        closed: it.closed(),
                    })
                    .collect();
                Plan::new(&snapshot, icon.samples())
            }
        };
        let out = plan.outputs();
        self.flight = Some(Flight { plan, out });
        self.target = icon;
    }

    fn render(&mut self, t: f64) {
        if let Some(f) = &mut self.flight {
            self.t = t;
            f.plan.interpolate(t, &mut f.out);
        }
    }

    fn settle(&mut self) {
        self.flight = None;
        self.t = 1.0;
        self.spring.x = 1.0;
        self.spring.v = 0.0;
    }
}
