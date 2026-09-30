//! Universal morphing for stroke-based icons: any icon morphs into any other
//! with spring physics. Rotations are never declared by hand; they come out
//! of the math (closed-form 2D Procrustes plus polar interpolation).
//!
//! This crate is the renderer-agnostic core: it has no dependencies and never
//! draws anything itself. It turns icon data into geometry, and a binding
//! crate (`morphicons-egui`, `morphicons-iced`, …) or your own code draws that
//! geometry through a [`PathSink`].
//!
//! ```
//! use morphicons::{icons, Morph, SpringConfig};
//!
//! let mut morph = Morph::new(icons::menu());
//! morph.morph_to(icons::x(), SpringConfig::SNAPPY);
//!
//! // Once per frame, from your UI framework's clock (seconds):
//! let mut now = 0.0;
//! while morph.update(now) {
//!     let d = morph.path_d(); // or morph.draw(&mut your_sink)
//!     assert!(d.starts_with('M'));
//!     now += 1.0 / 60.0;
//! }
//! assert_eq!(morph.target(), &icons::x());
//! ```
//!
//! # Layers
//!
//! - [`Icon`]: parsed icon data (SVG `d` strings, Lucide-style nodes or typed
//!   [`Element`]s), normalized to cubic Béziers and resampled once.
//! - [`Plan`]: the correspondence and alignment between two icons, plus the
//!   polar interpolator. Pure math, cacheable.
//! - [`Morph`]: the driver. Spring, interruptions, seeking, at-rest snapping.
//!   It never owns a clock: the host calls [`Morph::update`] or [`Morph::tick`]
//!   once per frame.
//! - [`Controller`]: the binding-side lifecycle (uncontrolled icon prop vs a
//!   controlled `from`/`to`/`progress` triple), shared by every binding.
//!
//! Ported from the TypeScript library [morphicons] by Guillermo (MIT).
//!
//! [morphicons]: https://github.com/guillermolg00/morphicons

mod controller;
mod error;
mod geom;
mod icon;
pub mod icons;
mod morph;
mod normalize;
mod parse;
mod plan;
mod resample;
mod serialize;
mod sink;
mod spring;

#[cfg(test)]
mod tests;

pub use controller::{Controller, Source};
pub use error::Error;
pub use geom::Point;
pub use icon::{Icon, ViewBox};
pub use morph::Morph;
pub use normalize::{CubicPath, Element, KAPPA};
pub use plan::{Plan, PlanItem, Similarity, procrustes};
pub use resample::{CORNER_THRESHOLD, Sampled, arc_length, detect_corners, resample_path};
pub use serialize::{cubics_to_d, polylines_to_d};
pub use sink::{Flattener, PathSink, Polyline};
pub use spring::{Spring, SpringConfig};

/// Points per sampled subpath. Every [`Icon`] is resampled at this count, so
/// any two icons can be planned against each other.
pub const SAMPLES: usize = 64;

/// The coordinate grid icons are drawn on (Lucide, Tabler, Heroicons outline,
/// Iconoir…). Use [`Icon::fit`] to bring icons from other grids onto it.
pub const GRID: f64 = 24.0;
