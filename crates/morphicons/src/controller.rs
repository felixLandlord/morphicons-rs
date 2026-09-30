//! The binding lifecycle as framework-neutral code, shared by every binding:
//! a widget is either *uncontrolled* (show this icon; animate when it
//! changes) or *controlled* (show `from → to` frozen at `progress`, for
//! gestures and scroll). Imperative calls work in both modes.

use crate::{Icon, Morph, SpringConfig};

/// What a widget was asked to show this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Uncontrolled: show this icon; changing it animates with the spring.
    Icon(Icon),
    /// Controlled: the morph from `from` to `to`, frozen at `progress`
    /// (no spring). 0 shows `from`, 1 shows `to`.
    Between { from: Icon, to: Icon, progress: f64 },
}

impl From<Icon> for Source {
    fn from(icon: Icon) -> Self {
        Source::Icon(icon)
    }
}

/// A [`Morph`] plus change detection over a [`Source`]. Feed it the widget's
/// source every frame with [`Controller::sync`]; it only reacts to changes,
/// so imperative calls in between aren't overridden until the source changes.
///
/// Contract (mirrors upstream's bindings):
/// - Controlled wins: while the source is `Between`, the pair owns the path.
/// - Leaving controlled mode hands the path back to the icon, animated.
/// - Any exit from a pair (imperative call or icon takeover) invalidates it,
///   so returning to the same pair re-bases on `from`.
#[derive(Clone, Debug)]
pub struct Controller {
    morph: Morph,
    prev: Source,
    /// The morph holds a seek plan based on the current pair.
    based: bool,
}

impl Controller {
    pub fn new(source: Source) -> Self {
        let (morph, based) = match &source {
            Source::Icon(icon) => (Morph::new(icon.clone()), false),
            Source::Between { from, to, progress } => {
                let mut m = Morph::new(from.clone());
                let based = apply_pair(&mut m, from, to, *progress, false);
                (m, based)
            }
        };
        Self {
            morph,
            prev: source,
            based,
        }
    }

    /// Reconciles with this frame's source. Cheap when nothing changed.
    pub fn sync(&mut self, source: &Source, spring: SpringConfig) {
        if *source == self.prev {
            return;
        }
        match source {
            Source::Between { from, to, progress } => {
                let same_pair = matches!(&self.prev, Source::Between { from: f, to: t, .. } if f == from && t == to);
                let based = same_pair && self.based;
                self.based = apply_pair(&mut self.morph, from, to, *progress, based);
            }
            Source::Icon(icon) => {
                self.based = false;
                self.morph.morph_to(icon.clone(), spring);
            }
        }
        self.prev = source.clone();
    }

    /// Animates to `icon` now, regardless of the source (until it changes).
    pub fn morph_to(&mut self, icon: Icon, spring: SpringConfig) {
        self.based = false;
        self.morph.morph_to(icon, spring);
    }

    /// Jumps to `icon` now, regardless of the source (until it changes).
    pub fn set(&mut self, icon: Icon) {
        self.based = false;
        self.morph.set(icon);
    }

    /// See [`Morph::update`].
    pub fn update(&mut self, now: f64) -> bool {
        self.morph.update(now)
    }

    pub fn morph(&self) -> &Morph {
        &self.morph
    }

    pub fn morph_mut(&mut self) -> &mut Morph {
        &mut self.morph
    }
}

/// Freezes the pair at `progress`. Returns whether the morph now holds a seek
/// plan based on `from` (so the next progress change can reuse it).
fn apply_pair(m: &mut Morph, from: &Icon, to: &Icon, progress: f64, based: bool) -> bool {
    if progress <= 0.0 {
        m.set(from.clone());
        false
    } else if progress >= 1.0 {
        m.set(to.clone());
        false
    } else {
        if !based {
            m.set(from.clone()); // re-base the plan on the pair's origin
        }
        m.seek(to.clone(), progress);
        true
    }
}
