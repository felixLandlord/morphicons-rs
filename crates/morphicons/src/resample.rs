//! Arc-length resampling with anchored corners.
//!
//! The length of a cubic has no closed form: |B′(t)| is integrated with
//! 8-point Gauss-Legendre. Corners (tangent discontinuity above an angular
//! threshold) are anchored as exact sample points; the remaining points are
//! distributed by arc length between corners — integer apportionment by
//! largest remainder, minimum 1 interval per run, summing exactly to N−1
//! (N if closed).

use std::collections::BTreeSet;
use std::f64::consts::PI;

use crate::{CubicPath, Error, Point};

/// Default angular threshold for a segment joint to count as a corner (22.5°).
pub const CORNER_THRESHOLD: f64 = PI / 8.0;

/// A subpath sampled at N points by arc length, plus its topology. It is the
/// currency between resampling and planning; an in-flight shape (an
/// interruption) is also a list of `Sampled`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sampled {
    pub points: Vec<Point>,
    pub closed: bool,
}

// Gauss-Legendre, 8 points on [−1, 1]; symmetric nodes, only half is stored.
const GX: [f64; 4] = [
    0.18343464249564978,
    0.525532409916329,
    0.7966664774136267,
    0.9602898564975363,
];
const GW: [f64; 4] = [
    0.362683783378362,
    0.31370664587788727,
    0.22238103445337448,
    0.10122853629037626,
];

/// |B′(t)| of a segment. B′(t) = 3(1−t)²(P₁−P₀) + 6(1−t)t(P₂−P₁) + 3t²(P₃−P₂).
fn speed(p: &[Point], t: f64) -> f64 {
    let u = 1.0 - t;
    let (c0, c1, c2) = (3.0 * u * u, 6.0 * u * t, 3.0 * t * t);
    let dx = c0 * (p[1].x - p[0].x) + c1 * (p[2].x - p[1].x) + c2 * (p[3].x - p[2].x);
    let dy = c0 * (p[1].y - p[0].y) + c1 * (p[2].y - p[1].y) + c2 * (p[3].y - p[2].y);
    dx.hypot(dy)
}

/// ∫₀^t1 |B′| of a segment via Gauss-Legendre.
fn seg_len(p: &[Point], t1: f64) -> f64 {
    let half = t1 / 2.0;
    let mut s = 0.0;
    for j in 0..4 {
        s += GW[j] * (speed(p, half + half * GX[j]) + speed(p, half - half * GX[j]));
    }
    s * half
}

/// Bernstein evaluation of a segment at t.
pub(crate) fn bezier_point(p: &[Point], t: f64) -> Point {
    let u = 1.0 - t;
    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Point::new(
        b0 * p[0].x + b1 * p[1].x + b2 * p[2].x + b3 * p[3].x,
        b0 * p[0].y + b1 * p[1].y + b2 * p[2].y + b3 * p[3].y,
    )
}

/// Tangent at an endpoint of a segment. `at_end`: outgoing at P₃ (P₃−P₂);
/// otherwise incoming at P₀ (P₁−P₀). Falls back to the next control point
/// when degenerate.
fn tangent(p: &[Point], at_end: bool) -> Option<Point> {
    let (base, order, sign) = if at_end {
        (3, [2, 1, 0], -1.0)
    } else {
        (0, [1, 2, 3], 1.0)
    };
    order.into_iter().find_map(|j| {
        let d = (p[j] - p[base]) * sign;
        (d.length_squared() > 1e-18).then_some(d)
    })
}

/// Segment boundaries (index of the segment starting at the corner) whose
/// tangent discontinuity exceeds `threshold`. For closed paths this includes
/// the closing joint (boundary = first active segment).
pub fn detect_corners(path: &CubicPath, threshold: f64) -> Vec<usize> {
    let m = path.segment_count();
    let active: Vec<usize> = (0..m)
        .filter(|&k| seg_len(path.segment(k), 1.0) > 1e-9)
        .collect();
    let mut corners = BTreeSet::new();
    let mut test = |a: usize, b: usize| {
        let (Some(u), Some(v)) = (
            tangent(path.segment(a), true),
            tangent(path.segment(b), false),
        ) else {
            return;
        };
        let ang = (u.x * v.y - u.y * v.x).atan2(u.x * v.x + u.y * v.y).abs();
        if ang > threshold {
            corners.insert(b);
        }
    };
    for w in active.windows(2) {
        test(w[0], w[1]);
    }
    if path.closed && active.len() > 1 {
        test(active[active.len() - 1], active[0]);
    }
    corners.into_iter().collect()
}

/// Total arc length of a subpath (per-segment Gauss-Legendre).
pub fn arc_length(path: &CubicPath) -> f64 {
    (0..path.segment_count())
        .map(|k| seg_len(path.segment(k), 1.0))
        .sum()
}

/// Arc-length inversion: t such that ∫₀^t |B′| = s. Safeguarded Newton with a
/// bisection bracket; |B′| is the exact derivative of the objective.
fn invert(p: &[Point], s: f64, ls: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    if s >= ls {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    let mut t = s / ls;
    for _ in 0..12 {
        let f = seg_len(p, t) - s;
        if f.abs() < 1e-10 * ls + 1e-14 {
            break;
        }
        if f > 0.0 {
            hi = t;
        } else {
            lo = t;
        }
        let sp = speed(p, t);
        let mut nt = if sp > 1e-12 {
            t - f / sp
        } else {
            (lo + hi) / 2.0
        };
        if !(nt > lo && nt < hi) {
            nt = (lo + hi) / 2.0;
        }
        t = nt;
    }
    t
}

/// Samples a cubic subpath at `n` points equidistant by arc length, anchoring
/// corners and endpoints as exact samples. Closed paths distribute `n`
/// intervals around the loop (without duplicating the first point); the
/// circular start-point freedom is resolved by the plan's circular
/// correspondence.
pub fn resample_path(
    path: &CubicPath,
    n: usize,
    corner_threshold: f64,
) -> Result<Vec<Point>, Error> {
    let p = &path.points;
    let m = path.segment_count();
    let first = p.first().copied().unwrap_or_default();
    if m < 1 {
        return Ok(vec![first; n]);
    }
    let lens: Vec<f64> = (0..m).map(|k| seg_len(path.segment(k), 1.0)).collect();
    if lens.iter().sum::<f64>() < 1e-12 {
        return Ok(vec![first; n]);
    }

    // Anchors: segment boundaries. For open paths, endpoints + corners. For
    // closed paths, ONLY corners: sampling must be intrinsic to the shape and
    // not to the arbitrary M point — two congruent loops with different start
    // points produce the same sample set (modulo index rotation, which the
    // plan's circular correspondence resolves). With no corners (a circle)
    // the path start is the only possible reference.
    let cs = detect_corners(path, corner_threshold);
    let anchors: Vec<usize> = if path.closed {
        if cs.is_empty() { vec![0] } else { cs }
    } else {
        let mut set: BTreeSet<usize> = cs.into_iter().collect();
        set.insert(0);
        set.insert(m);
        set.into_iter().collect()
    };
    // Runs between anchors; for closed paths the last wraps to anchors[0] + m.
    let runs: Vec<(usize, usize)> = if path.closed {
        (0..anchors.len())
            .map(|j| {
                (
                    anchors[j],
                    anchors.get(j + 1).copied().unwrap_or(anchors[0] + m),
                )
            })
            .collect()
    } else {
        anchors.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let rl: Vec<f64> = runs
        .iter()
        .map(|&(a, b)| (a..b).map(|k| lens[k % m]).sum())
        .collect();
    let intervals = if path.closed { n } else { n - 1 };
    if runs.len() > intervals {
        return Err(Error::TooManyCorners {
            runs: runs.len(),
            samples: n,
        });
    }

    // Largest-remainder apportionment: proportional to length, min 1, exact sum.
    let total = match rl.iter().sum::<f64>() {
        0.0 => 1.0,
        t => t,
    };
    let ideal: Vec<f64> = rl.iter().map(|l| intervals as f64 * l / total).collect();
    let mut counts: Vec<usize> = ideal.iter().map(|q| (q.floor() as usize).max(1)).collect();
    let assigned: usize = counts.iter().sum();
    if assigned < intervals {
        // Quantized fraction: the quadrature's fp noise (~1e-15) must not
        // decide the tie-break — runs congruent under rotation must apportion
        // the same in both icons or Procrustes loses the exact congruence.
        let mut order: Vec<(i64, usize)> = ideal
            .iter()
            .enumerate()
            .map(|(idx, q)| (((q - q.floor()) * 1e9).round() as i64, idx))
            .collect();
        order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for j in 0..intervals - assigned {
            counts[order[j % order.len()].1] += 1;
        }
    } else {
        let mut excess = assigned - intervals;
        while excess > 0 {
            let mut bi = 0;
            for idx in 1..counts.len() {
                if counts[idx] > counts[bi] {
                    bi = idx;
                }
            }
            if counts[bi] <= 1 {
                break;
            }
            counts[bi] -= 1;
            excess -= 1;
        }
    }

    // Sampling: exact anchor at the start of each run + interiors by inversion.
    let mut out = Vec::with_capacity(n);
    for (r, &(k0, k1)) in runs.iter().enumerate() {
        let cnt = counts[r];
        let lr = rl[r];
        out.push(p[3 * (k0 % m)]);
        let mut seg = k0;
        let mut acc = 0.0;
        for j in 1..cnt {
            let target = lr * j as f64 / cnt as f64;
            while seg < k1 - 1 && acc + lens[seg % m] < target {
                acc += lens[seg % m];
                seg += 1;
            }
            let k = seg % m;
            let ls = lens[k];
            let t = if ls > 1e-12 {
                invert(path.segment(k), target - acc, ls)
            } else {
                0.0
            };
            out.push(bezier_point(path.segment(k), t));
        }
    }
    if !path.closed {
        out.push(p[3 * m]);
    }
    debug_assert_eq!(out.len(), n);
    out.resize(n, p[3 * m]);
    Ok(out)
}
