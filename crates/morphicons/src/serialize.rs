//! SVG path data output. In flight each subpath is a polyline `M x y L x y …`
//! with 2 decimals (invisible at 24px). At rest the canonical `d` holds the
//! real cubics, quantized to 4 decimals so the bytes are stable across
//! platforms (trig in arc conversion differs in the last ulp).

use std::fmt::Write;

use crate::{CubicPath, Point};

fn push_num(d: &mut String, v: f64, scale: f64) {
    let r = (v * scale).round() / scale;
    // `+ 0.0` turns -0 into 0.
    let _ = write!(d, "{}", r + 0.0);
}

fn push_point(d: &mut String, cmd: char, p: Point, scale: f64) {
    d.push(cmd);
    push_num(d, p.x, scale);
    d.push(' ');
    push_num(d, p.y, scale);
}

/// Sampled subpaths → polyline `d`. `closed[k]` appends `Z` to subpath `k`.
pub fn polylines_to_d(subs: &[Vec<Point>], closed: &[bool]) -> String {
    let mut d = String::with_capacity(subs.iter().map(|s| s.len() * 12).sum());
    for (k, sub) in subs.iter().enumerate() {
        let Some((&first, rest)) = sub.split_first() else {
            continue;
        };
        push_point(&mut d, 'M', first, 100.0);
        for &p in rest {
            push_point(&mut d, 'L', p, 100.0);
        }
        if closed.get(k).copied().unwrap_or(false) {
            d.push('Z');
        }
    }
    d
}

/// Cubic subpaths → canonical `d`, quantized to 4 decimals.
pub fn cubics_to_d(paths: &[CubicPath]) -> String {
    let mut d = String::new();
    for path in paths {
        let Some((&first, rest)) = path.points.split_first() else {
            continue;
        };
        push_point(&mut d, 'M', first, 1e4);
        for seg in rest.chunks_exact(3) {
            push_point(&mut d, 'C', seg[0], 1e4);
            for &p in &seg[1..] {
                d.push(' ');
                push_num(&mut d, p.x, 1e4);
                d.push(' ');
                push_num(&mut d, p.y, 1e4);
            }
        }
        if path.closed {
            d.push('Z');
        }
    }
    d
}
