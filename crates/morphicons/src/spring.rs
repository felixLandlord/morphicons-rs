//! Damped harmonic oscillator over the progress x: 0 → 1,
//! ẍ = k·(1−x) − c·ẋ, integrated with semi-implicit Euler at 1/240 s
//! substeps (stable up to ω·h ≈ 2; with k = 420, ω ≈ 20.5 — ample margin).
//! Interruptible: [`Spring::start`] resets x to 0 while preserving velocity
//! (clamped to ±14).

/// Spring physics for a morph: stiffness `k` and damping `c`
/// (damping ratio ζ = c / (2√k)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringConfig {
    pub stiffness: f64,
    pub damping: f64,
}

impl SpringConfig {
    /// ζ = 1.00: critically damped, no overshoot.
    pub const SMOOTH: SpringConfig = SpringConfig {
        stiffness: 170.0,
        damping: 26.0,
    };
    /// ζ = 0.73: fast, subtle overshoot. The default.
    pub const SNAPPY: SpringConfig = SpringConfig {
        stiffness: 420.0,
        damping: 30.0,
    };
    /// ζ = 0.40: playful.
    pub const BOUNCY: SpringConfig = SpringConfig {
        stiffness: 300.0,
        damping: 14.0,
    };

    pub const fn new(stiffness: f64, damping: f64) -> Self {
        Self { stiffness, damping }
    }
}

impl Default for SpringConfig {
    fn default() -> Self {
        Self::SNAPPY
    }
}

/// The integrator state. [`Morph`](crate::Morph) owns one; it's public for
/// hosts that want to drive a [`Plan`](crate::Plan) themselves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Progress (overshoots past 1 with underdamped configs).
    pub x: f64,
    /// Velocity, in progress units per second.
    pub v: f64,
    pub config: SpringConfig,
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            x: 1.0,
            v: 0.0,
            config: SpringConfig::default(),
        }
    }
}

impl Spring {
    /// Maximum inherited velocity on a restart.
    pub const MAX_VELOCITY: f64 = 14.0;

    /// Starts (or restarts mid-flight) preserving velocity.
    pub fn start(&mut self) {
        self.x = 0.0;
        self.v = self.v.clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY);
    }

    /// Advances `dt` seconds. Returns true on settle
    /// (|1−x| < 0.001 and |v| < 0.02).
    pub fn step(&mut self, dt: f64) -> bool {
        const H: f64 = 1.0 / 240.0;
        let steps = ((dt / H).ceil() as usize).clamp(1, 16);
        let s = dt / steps as f64;
        let SpringConfig {
            stiffness: k,
            damping: c,
        } = self.config;
        for _ in 0..steps {
            let a = k * (1.0 - self.x) - c * self.v;
            self.v += a * s;
            self.x += self.v * s;
        }
        self.is_settled()
    }

    pub fn is_settled(&self) -> bool {
        (1.0 - self.x).abs() < 0.001 && self.v.abs() < 0.02
    }
}
