//! Correspondence, alignment and interpolation.
//!
//! Closed-form 2D Procrustes (no SVD: atan2), subpath matching by
//! centroid+length cost (surjective when p ≠ q: leftovers duplicate, "cell
//! division"), circular correspondence for closed loops, minimal-rotation
//! tie-break with λ, and a global hybrid: if the whole icon is congruent under
//! ONE similarity, every subpath shares it (coherent block rotation).

use std::f64::consts::PI;

use crate::{Point, Sampled};

/// Weight of |ΔL| in the subpath pairing cost.
const LEN_WEIGHT: f64 = 0.35;

/// λ of the minimal-rotation tie-break: score = res + λ·|θ|/π. It exists
/// because shapes symmetric under inversion (lines) tie in residual for both
/// traversal orientations yet produce different rotations.
const LAMBDA: f64 = 0.05;

/// Global residual below which the whole icon counts as congruent and the
/// plan shares (θ, σ) across all items (hybrid variant of Procrustes).
const GLOBAL_EPS: f64 = 5e-3;

/// Bounds for exhaustive matching; above them it falls back to greedy with
/// repair. 8! = 40 320 permutations / 1e5 assignments — both sub-ms.
const PERM_MAX: usize = 8;
const SURJ_MAX: f64 = 1e5;

/// Optimal similarity between two point clouds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Similarity {
    /// Rotation in radians, in (−π, π].
    pub theta: f64,
    /// Uniform scale.
    pub sigma: f64,
    /// Normalized RMS residual after alignment: ≈ 0 means "same shape".
    pub residual: f64,
}

pub(crate) fn centroid(p: &[Point]) -> Point {
    let n = p.len() as f64;
    let (sx, sy) = p.iter().fold((0.0, 0.0), |(x, y), q| (x + q.x, y + q.y));
    Point::new(sx / n, sy / n)
}

fn poly_len(p: &[Point]) -> f64 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Procrustes over pairs produced by `pair(i)`, so candidate correspondences
/// can be scored without materializing re-indexed clouds.
fn procrustes_by(
    n: usize,
    ca: Point,
    cb: Point,
    pair: impl Fn(usize) -> (Point, Point),
) -> Similarity {
    let (mut sxx, mut sxy, mut syx, mut syy, mut na, mut nb) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let (a, b) = pair(i);
        let (ax, ay) = (a.x - ca.x, a.y - ca.y);
        let (bx, by) = (b.x - cb.x, b.y - cb.y);
        sxx += ax * bx;
        syy += ay * by;
        sxy += ax * by;
        syx += ay * bx;
        na += ax * ax + ay * ay;
        nb += bx * bx + by * by;
    }
    let theta = (sxy - syx).atan2(sxx + syy);
    let num = theta.cos() * (sxx + syy) + theta.sin() * (sxy - syx);
    let mut sigma = if na > 1e-12 { num / na } else { 1.0 };
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // also catches NaN
    if !(sigma > 1e-6) {
        sigma = 1e-6;
    }
    let res2 = (sigma * sigma * na - 2.0 * sigma * num + nb).max(0.0);
    let residual = if nb > 1e-12 { (res2 / nb).sqrt() } else { 0.0 };
    Similarity {
        theta,
        sigma,
        residual,
    }
}

/// Optimal similarity (θ, σ) minimizing Σ|σ·R(θ)·(aᵢ−c_A) − (bᵢ−c_B)|².
/// θ* = atan2(S_xy − S_yx, S_xx + S_yy); σ* by zero derivative.
pub fn procrustes(a: &[Point], b: &[Point], ca: Point, cb: Point) -> Similarity {
    debug_assert_eq!(a.len(), b.len());
    procrustes_by(a.len().min(b.len()), ca, cb, |i| (a[i], b[i]))
}

struct Alignment {
    sim: Similarity,
    ca: Point,
    cb: Point,
    a: Vec<Point>,
    b: Vec<Point>,
}

/// Best index-to-index correspondence between a and b: tries both traversal
/// directions and, if there is a closed loop, its N circular offsets, scoring
/// with score = res + λ·|θ|/π. The freedom is applied to ONE cloud — the
/// closed one (b if both are); varying both at once would be redundant.
fn align_pair(a: &[Point], b: &[Point], a_closed: bool, b_closed: bool) -> Alignment {
    let n = a.len();
    let ca = centroid(a);
    let cb = centroid(b);
    let vary_a = a_closed && !b_closed;
    let base = if vary_a { a } else { b };
    let offs = if a_closed || b_closed { n } else { 1 };
    // Candidate `i`-th point: `base` walked forward or reversed, then cut at `off`.
    let pick = |dir: usize, off: usize, i: usize| {
        let j = (i + off) % n;
        base[if dir == 1 { n - 1 - j } else { j }]
    };
    let mut best = (0, 0);
    let mut best_score = f64::INFINITY;
    let mut sim = Similarity {
        theta: 0.0,
        sigma: 1.0,
        residual: 0.0,
    };
    for dir in 0..2 {
        for off in 0..offs {
            let s = if vary_a {
                procrustes_by(n, ca, cb, |i| (pick(dir, off, i), b[i]))
            } else {
                procrustes_by(n, ca, cb, |i| (a[i], pick(dir, off, i)))
            };
            let score = s.residual + LAMBDA * s.theta.abs() / PI;
            if score < best_score {
                best_score = score;
                best = (dir, off);
                sim = s;
            }
        }
    }
    let chosen: Vec<Point> = (0..n).map(|i| pick(best.0, best.1, i)).collect();
    if vary_a {
        Alignment {
            sim,
            ca,
            cb,
            a: chosen,
            b: b.to_vec(),
        }
    } else {
        Alignment {
            sim,
            ca,
            cb,
            a: a.to_vec(),
            b: chosen,
        }
    }
}

/// Cost matrix dist(centroids) + LEN_WEIGHT·|ΔL| between all pairs.
fn cost_matrix(a: &[&[Point]], b: &[&[Point]]) -> Vec<Vec<f64>> {
    let cbs: Vec<(Point, f64)> = b.iter().map(|p| (centroid(p), poly_len(p))).collect();
    a.iter()
        .map(|p| {
            let (ca, la) = (centroid(p), poly_len(p));
            cbs.iter()
                .map(|&(cb, lb)| ca.distance(cb) + LEN_WEIGHT * (la - lb).abs())
                .collect()
        })
        .collect()
}

/// p = q: minimum-cost permutation. Exhaustive with pruning up to PERM_MAX;
/// greedy (pairs sorted by cost) above it — with that many subpaths the exact
/// optimum stops mattering visually.
fn best_permutation(c: &[Vec<f64>]) -> Vec<usize> {
    let n = c.len();
    if n > PERM_MAX {
        let mut pairs: Vec<(f64, usize, usize)> = (0..n)
            .flat_map(|i| (0..n).map(move |j| (c[i][j], i, j)))
            .collect();
        pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut out = vec![usize::MAX; n];
        let mut used = vec![false; n];
        for (_, i, j) in pairs {
            if out[i] == usize::MAX && !used[j] {
                out[i] = j;
                used[j] = true;
            }
        }
        return out;
    }
    fn perm(
        c: &[Vec<f64>],
        arr: &mut Vec<usize>,
        k: usize,
        acc: f64,
        best: &mut Vec<usize>,
        bc: &mut f64,
    ) {
        if acc >= *bc {
            return;
        }
        if k == arr.len() {
            *bc = acc;
            best.clone_from(arr);
            return;
        }
        for i in k..arr.len() {
            arr.swap(k, i);
            let cost = c[k][arr[k]];
            perm(c, arr, k + 1, acc + cost, best, bc);
            arr.swap(k, i);
        }
    }
    let mut arr: Vec<usize> = (0..n).collect();
    let mut best = arr.clone();
    let mut bc = f64::INFINITY;
    perm(c, &mut arr, 0, 0.0, &mut best, &mut bc);
    best
}

/// p ≠ q: surjective assignment from the large side (rows) to the small one
/// (columns), minimum cost. Enumeration with pruning when S^B is small;
/// greedy with coverage repair otherwise. Surjectivity guarantees no subpath appears or
/// vanishes out of nowhere.
fn best_surjection(c: &[Vec<f64>]) -> Vec<usize> {
    let big = c.len();
    let small = c[0].len();
    if (small as f64).powi(big as i32) > SURJ_MAX {
        let mut f: Vec<usize> = c
            .iter()
            .map(|row| (1..row.len()).fold(0, |m, j| if row[j] < row[m] { j } else { m }))
            .collect();
        let mut mult = vec![0usize; small];
        for &s in &f {
            mult[s] += 1;
        }
        for s in 0..small {
            if mult[s] > 0 {
                continue;
            }
            let mut donor = None;
            let mut bc = f64::INFINITY;
            for i in 0..big {
                if mult[f[i]] < 2 {
                    continue; // only donors with multiplicity
                }
                let extra = c[i][s] - c[i][f[i]];
                if extra < bc {
                    bc = extra;
                    donor = Some(i);
                }
            }
            if let Some(i) = donor {
                mult[f[i]] -= 1;
                f[i] = s;
                mult[s] += 1;
            }
        }
        return f;
    }
    struct Search<'a> {
        c: &'a [Vec<f64>],
        f: Vec<usize>,
        mult: Vec<usize>,
        best: Option<Vec<usize>>,
        bc: f64,
    }
    fn rec(s: &mut Search, i: usize, acc: f64, covered: usize) {
        let (big, small) = (s.c.len(), s.c[0].len());
        if acc >= s.bc || small - covered > big - i {
            return;
        }
        if i == big {
            s.bc = acc;
            s.best = Some(s.f.clone());
            return;
        }
        for j in 0..small {
            s.f[i] = j;
            s.mult[j] += 1;
            let newly = usize::from(s.mult[j] == 1);
            rec(s, i + 1, acc + s.c[i][j], covered + newly);
            s.mult[j] -= 1;
        }
    }
    let mut s = Search {
        c,
        f: vec![0; big],
        mult: vec![0; small],
        best: None,
        bc: f64::INFINITY,
    };
    rec(&mut s, 0, 0.0, 0);
    s.best
        .unwrap_or_else(|| (0..big).map(|i| i % small).collect())
}

/// Block transport under the global hybrid: mid-flight the centroid rides the
/// shared similarity around the global centroid instead of lerping.
/// `off` = c_A − g_A; `drift` closes c(1) = c_B exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Block {
    off: Point,
    drift: Point,
}

/// One matched pair of subpaths in a [`Plan`].
#[derive(Clone, Debug, PartialEq)]
pub struct PlanItem {
    /// Points of A with the chosen correspondence (a closed loop may come
    /// circularly re-indexed: same points, different cut).
    pub(crate) a: Vec<Point>,
    /// A centered on its centroid.
    pub(crate) a_c: Vec<Point>,
    /// B brought into A's frame: R(−θ)·(b − c_B)/σ.
    pub(crate) b_t: Vec<Point>,
    /// B oriented, raw (for linear mode and for exact t = 1).
    pub(crate) b_o: Vec<Point>,
    pub(crate) ca: Point,
    pub(crate) cb: Point,
    pub(crate) theta: f64,
    pub(crate) ln_sigma: f64,
    pub(crate) residual: f64,
    pub(crate) closed: bool,
    pub(crate) block: Option<Block>,
}

impl PlanItem {
    /// Rotation this subpath performs over the flight, in radians.
    pub fn theta(&self) -> f64 {
        self.theta
    }
    /// Scale this subpath performs over the flight.
    pub fn sigma(&self) -> f64 {
        self.ln_sigma.exp()
    }
    /// Shape residual after alignment (≈ 0: pure similarity).
    pub fn residual(&self) -> f64 {
        self.residual
    }
    /// Both endpoints are closed loops: the subpath flies closed.
    pub fn closed(&self) -> bool {
        self.closed
    }
    /// The source samples, in flight order.
    pub fn source(&self) -> &[Point] {
        &self.a
    }
    /// The target samples, in flight order.
    pub fn target(&self) -> &[Point] {
        &self.b_o
    }
    pub fn source_centroid(&self) -> Point {
        self.ca
    }
    pub fn target_centroid(&self) -> Point {
        self.cb
    }
    /// True when the whole icon moves as one rigid block (global hybrid).
    pub fn is_block(&self) -> bool {
        self.block.is_some()
    }
}

/// The correspondence and alignment between two lists of sampled subpaths,
/// and the interpolator between them. Cheap to build (sub-millisecond),
/// cacheable, and accepts any source — including an in-flight shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    items: Vec<PlanItem>,
    n: usize,
}

impl Plan {
    /// Plans the morph from `src` to `dst`. All subpaths must share one
    /// sample count (every [`Icon`](crate::Icon) uses [`SAMPLES`](crate::SAMPLES)).
    /// An empty side yields an empty plan.
    pub fn new(src: &[Sampled], dst: &[Sampled]) -> Plan {
        let (p, q) = (src.len(), dst.len());
        let n = src.first().or(dst.first()).map_or(0, |s| s.points.len());
        if p == 0 || q == 0 {
            return Plan {
                items: Vec::new(),
                n,
            };
        }
        let a: Vec<&[Point]> = src.iter().map(|s| s.points.as_slice()).collect();
        let b: Vec<&[Point]> = dst.iter().map(|s| s.points.as_slice()).collect();
        let pairs: Vec<(usize, usize)> = if p == q {
            let perm = best_permutation(&cost_matrix(&a, &b));
            (0..p).map(|i| (i, perm[i])).collect()
        } else if p < q {
            let f = best_surjection(&cost_matrix(&b, &a));
            (0..q).map(|j| (f[j], j)).collect()
        } else {
            let f = best_surjection(&cost_matrix(&a, &b));
            (0..p).map(|i| (i, f[i])).collect()
        };
        let mut items: Vec<PlanItem> = pairs
            .into_iter()
            .map(|(si, di)| {
                let al = align_pair(a[si], b[di], src[si].closed, dst[di].closed);
                let (sin, cos) = (-al.sim.theta).sin_cos();
                let a_c = al.a.iter().map(|&p| p - al.ca).collect();
                let b_t =
                    al.b.iter()
                        .map(|&p| {
                            let d = p - al.cb;
                            Point::new(
                                (d.x * cos - d.y * sin) / al.sim.sigma,
                                (d.x * sin + d.y * cos) / al.sim.sigma,
                            )
                        })
                        .collect();
                PlanItem {
                    a: al.a,
                    a_c,
                    b_t,
                    b_o: al.b,
                    ca: al.ca,
                    cb: al.cb,
                    theta: al.sim.theta,
                    ln_sigma: al.sim.sigma.ln(),
                    residual: al.sim.residual,
                    closed: src[si].closed && dst[di].closed,
                    block: None,
                }
            })
            .collect();
        if items.len() > 1 {
            apply_global(&mut items, n);
        }
        Plan { items, n }
    }

    /// The matched subpath pairs (introspection: θ, σ, residual…).
    pub fn items(&self) -> &[PlanItem] {
        &self.items
    }

    /// Samples per subpath.
    pub fn sample_count(&self) -> usize {
        self.n
    }

    /// Output buffers sized for this plan, for [`Plan::interpolate`].
    pub fn outputs(&self) -> Vec<Vec<Point>> {
        self.items
            .iter()
            .map(|_| vec![Point::ZERO; self.n])
            .collect()
    }

    /// Polar interpolation. The similarity is interpolated in its natural
    /// space (linear angle, log-linear scale, lerped centroid) and applied to
    /// the residual blend in the aligned frame:
    ///
    /// `P(t) = c(t) + σᵗ·R(t·θ)·[(1−t)·aᶜ + t·b̃]`
    ///
    /// Under the global hybrid the centroid rides the shared similarity
    /// instead (block transport), so congruent icons stay rigid mid-flight.
    /// Exact at t = 0 and t = 1; for t > 1 (spring overshoot) it extrapolates
    /// naturally. `out` must come from [`Plan::outputs`].
    pub fn interpolate(&self, t: f64, out: &mut [Vec<Point>]) {
        for (it, o) in self.items.iter().zip(out.iter_mut()) {
            let s = (it.ln_sigma * t).exp();
            let (sin, cos) = (it.theta * t).sin_cos();
            let (cos, sin) = (cos * s, sin * s);
            let c = match it.block {
                Some(Block { off, drift }) => Point::new(
                    it.ca.x + drift.x * t + (off.x * cos - off.y * sin - off.x),
                    it.ca.y + drift.y * t + (off.x * sin + off.y * cos - off.y),
                ),
                None => Point::new(
                    it.ca.x + (it.cb.x - it.ca.x) * t,
                    it.ca.y + (it.cb.y - it.ca.y) * t,
                ),
            };
            for ((dst, a), b) in o.iter_mut().zip(&it.a_c).zip(&it.b_t) {
                let px = a.x + (b.x - a.x) * t;
                let py = a.y + (b.y - a.y) * t;
                *dst = Point::new(c.x + px * cos - py * sin, c.y + px * sin + py * cos);
            }
        }
    }

    /// Raw coordinate lerp over the same correspondence, no decomposition.
    /// Kept for comparison: rotations collapse along the chord.
    pub fn interpolate_linear(&self, t: f64, out: &mut [Vec<Point>]) {
        for (it, o) in self.items.iter().zip(out.iter_mut()) {
            for ((dst, a), b) in o.iter_mut().zip(&it.a).zip(&it.b_o) {
                *dst = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            }
        }
    }
}

/// Global hybrid: Procrustes over the concatenated clouds with the already
/// chosen correspondence. If the global residual ≈ 0 the whole icon is
/// congruent and every item shares (θ, σ): coherent block rotation (keeps a
/// symmetric subpath from picking the opposite spin).
fn apply_global(items: &mut [PlanItem], n: usize) {
    let ga: Vec<Point> = items.iter().flat_map(|it| it.a.iter().copied()).collect();
    let gb: Vec<Point> = items.iter().flat_map(|it| it.b_o.iter().copied()).collect();
    let gca = centroid(&ga);
    let g = procrustes(&ga, &gb, gca, centroid(&gb));
    if g.residual >= GLOBAL_EPS {
        return;
    }
    let (sin, cos) = (-g.theta).sin_cos();
    let (rs, rc) = g.theta.sin_cos();
    for it in items.iter_mut() {
        let (mut e2, mut nb) = (0.0, 0.0);
        for i in 0..n {
            let d = it.b_o[i] - it.cb;
            it.b_t[i] = Point::new(
                (d.x * cos - d.y * sin) / g.sigma,
                (d.x * sin + d.y * cos) / g.sigma,
            );
            let ac = it.a_c[i];
            let ex = g.sigma * (rc * ac.x - rs * ac.y) - d.x;
            let ey = g.sigma * (rs * ac.x + rc * ac.y) - d.y;
            e2 += ex * ex + ey * ey;
            nb += d.length_squared();
        }
        it.theta = g.theta;
        it.ln_sigma = g.sigma.ln();
        it.residual = if nb > 1e-12 { (e2 / nb).sqrt() } else { 0.0 };
        // Block transport: every part spins with the shared θ, but lerping the
        // centroids would send off-center parts along the chord — inside the
        // arc — and the block would deform mid-flight (an arrow's head sags
        // toward its shaft). The centroid rides the shared similarity around
        // the global centroid instead; drift absorbs the (tiny) global
        // residual so t = 1 stays exact. Same ops as the interpolator on
        // purpose: the rotation delta cancels bit-exactly at both endpoints.
        let s1 = it.ln_sigma.exp();
        let (n1, c1) = it.theta.sin_cos();
        let (c1, n1) = (c1 * s1, n1 * s1);
        let off = it.ca - gca;
        let rx = off.x * c1 - off.y * n1 - off.x;
        let ry = off.x * n1 + off.y * c1 - off.y;
        it.block = Some(Block {
            off,
            drift: Point::new(it.cb.x - it.ca.x - rx, it.cb.y - it.ca.y - ry),
        });
    }
}
