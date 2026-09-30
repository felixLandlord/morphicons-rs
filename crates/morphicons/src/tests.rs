//! Executable specification, ported from upstream's test suite: parsing,
//! normalization, resampling invariants, emergent rotations, endpoint
//! exactness, closed loops, block transport, matching, spring and driver.

use std::f64::consts::{FRAC_1_SQRT_2, PI};

use crate::parse::{RawSeg, parse_path};
use crate::*;

const ICONS: &[(&str, &str)] = &[
    ("menu", "M4 6h16M4 12h16M4 18h16"),
    ("x", "M18 6 6 18M6 6l12 12"),
    ("plus", "M5 12h14M12 5v14"),
    ("arrow-right", "M5 12h14M12 5l7 7-7 7"),
    ("arrow-down", "M12 5v14M19 12l-7 7-7-7"),
    ("check", "M20 6 9 17l-5-5"),
];

fn icon(name: &str) -> Icon {
    let d = ICONS
        .iter()
        .find(|(n, _)| *n == name)
        .expect("known icon")
        .1;
    Icon::from_d(d).unwrap()
}

fn plan(a: &str, b: &str) -> Plan {
    Plan::new(icon(a).samples(), icon(b).samples())
}

fn deg(r: f64) -> f64 {
    r * 180.0 / PI
}

fn max_diff(a: &[Point], b: &[Point]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(p, q)| (p.x - q.x).abs().max((p.y - q.y).abs()))
        .fold(0.0, f64::max)
}

fn assert_finite(pts: &[Point]) {
    assert!(pts.iter().all(|p| p.is_finite()), "non-finite point");
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

// ---------------------------------------------------------------- parsing

#[test]
fn parse_implicit_lineto_after_m() {
    let subs = parse_path("M1 2 3 4 5 6").unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].start, p(1.0, 2.0));
    assert_eq!(
        subs[0].segs,
        vec![RawSeg::Line(p(3.0, 4.0)), RawSeg::Line(p(5.0, 6.0))]
    );
}

#[test]
fn parse_relative_m_accumulates() {
    let subs = parse_path("m1 1 2 2 3 3").unwrap();
    assert_eq!(subs[0].start, p(1.0, 1.0));
    assert_eq!(
        subs[0].segs,
        vec![RawSeg::Line(p(3.0, 3.0)), RawSeg::Line(p(6.0, 6.0))]
    );
}

#[test]
fn parse_h_v_absolute_and_relative() {
    let subs = parse_path("M1 1H5V7h-2v-3").unwrap();
    assert_eq!(
        subs[0].segs,
        vec![
            RawSeg::Line(p(5.0, 1.0)),
            RawSeg::Line(p(5.0, 7.0)),
            RawSeg::Line(p(3.0, 7.0)),
            RawSeg::Line(p(3.0, 4.0)),
        ]
    );
}

#[test]
fn parse_packed_negatives_and_repetition() {
    let subs = parse_path("M12 5l7 7-7 7").unwrap();
    assert_eq!(
        subs[0].segs,
        vec![RawSeg::Line(p(19.0, 12.0)), RawSeg::Line(p(12.0, 19.0))]
    );
}

#[test]
fn parse_s_and_t_reflect() {
    let subs = parse_path("M0 0C1 1 2 1 3 0S5 -1 6 0").unwrap();
    assert_eq!(
        subs[0].segs[1],
        RawSeg::Cubic(p(4.0, -1.0), p(5.0, -1.0), p(6.0, 0.0))
    );
    let subs = parse_path("M0 0S2 1 3 0").unwrap();
    assert_eq!(
        subs[0].segs[0],
        RawSeg::Cubic(p(0.0, 0.0), p(2.0, 1.0), p(3.0, 0.0))
    );
    let subs = parse_path("M0 0Q1 1 2 0T4 0").unwrap();
    assert_eq!(subs[0].segs[1], RawSeg::Quad(p(3.0, -1.0), p(4.0, 0.0)));
}

#[test]
fn parse_packed_arc_flags() {
    assert_eq!(
        parse_path("M0 0a5 5 0 1 1 10 0").unwrap(),
        parse_path("M0 0a5 5 0 1110 0").unwrap()
    );
}

#[test]
fn parse_scientific_and_compact_decimals() {
    let subs = parse_path("M1e1 .5L.5.5 -1.5e-1-2").unwrap();
    assert_eq!(subs[0].start, p(10.0, 0.5));
    assert_eq!(
        subs[0].segs,
        vec![RawSeg::Line(p(0.5, 0.5)), RawSeg::Line(p(-0.15, -2.0))]
    );
}

#[test]
fn parse_z_reopens_at_start() {
    let subs = parse_path("M1 1L5 1L5 5ZL9 9").unwrap();
    assert_eq!(subs.len(), 2);
    assert!(subs[0].closed);
    assert_eq!(subs[1].start, p(1.0, 1.0));
    assert!(!subs[1].closed);
}

#[test]
fn parse_drops_empty_subpaths_and_reports_errors() {
    assert_eq!(parse_path("M1 1M2 2L3 3").unwrap().len(), 1);
    assert!(matches!(parse_path("L1 1"), Err(Error::Parse { .. })));
    assert!(matches!(parse_path("M1"), Err(Error::Parse { .. })));
    assert!(matches!(parse_path("M0 0Z 1 1"), Err(Error::Parse { .. })));
    assert!(matches!(
        parse_path("M0 0A1 1 0 2 0 1 1"),
        Err(Error::Parse { .. })
    ));
}

// ---------------------------------------------------------- normalization

#[test]
fn line_becomes_cubic_with_collinear_controls() {
    let icon = Icon::from_d("M0 0L3 6").unwrap();
    assert_eq!(
        icon.cubics()[0].points,
        vec![p(0.0, 0.0), p(1.0, 2.0), p(2.0, 4.0), p(3.0, 6.0)]
    );
}

#[test]
fn circle_is_four_closed_cubics_with_kappa() {
    let icon = Icon::from_elements(&[Element::Circle {
        cx: 12.0,
        cy: 12.0,
        r: 10.0,
    }])
    .unwrap();
    let c = &icon.cubics()[0];
    assert!(c.closed);
    assert_eq!(c.segment_count(), 4);
    assert!((c.points[1].y - 12.0 - KAPPA * 10.0).abs() < 1e-12);
    assert!((arc_length(c) - 2.0 * PI * 10.0).abs() < 0.01);
}

#[test]
fn quarter_arc_is_one_segment_of_length_pi_r_over_2() {
    let icon = Icon::from_d("M0 0A5 5 0 0 1 5 5").unwrap();
    let c = &icon.cubics()[0];
    assert_eq!(c.segment_count(), 1);
    assert_eq!(c.points[3], p(5.0, 5.0), "exact final endpoint");
    assert!((arc_length(c) - 2.0 * PI * 5.0 / 4.0).abs() < 5e-3);
    assert!(detect_corners(c, CORNER_THRESHOLD).is_empty());
}

#[test]
fn arc_with_small_radii_scales_to_semicircle_and_zero_radius_is_a_line() {
    let icon = Icon::from_d("M0 0A1 1 0 0 1 10 0").unwrap();
    assert!((arc_length(&icon.cubics()[0]) - PI * 5.0).abs() < 1e-2);
    let icon = Icon::from_d("M0 0A0 5 0 0 1 10 0").unwrap();
    assert!((arc_length(&icon.cubics()[0]) - 10.0).abs() < 1e-9);
}

#[test]
fn rects_polygons_and_polylines() {
    let rect = Icon::from_nodes(&[(
        "rect",
        &[("x", "2"), ("y", "2"), ("width", "20"), ("height", "10")],
    )])
    .unwrap();
    let c = &rect.cubics()[0];
    assert!(c.closed);
    assert_eq!(detect_corners(c, CORNER_THRESHOLD).len(), 4);
    assert!((arc_length(c) - 60.0).abs() < 1e-9);

    let rounded = Icon::from_nodes(&[(
        "rect",
        &[
            ("x", "2"),
            ("y", "2"),
            ("width", "20"),
            ("height", "20"),
            ("rx", "4"),
        ],
    )])
    .unwrap();
    assert!(detect_corners(&rounded.cubics()[0], CORNER_THRESHOLD).is_empty());

    let tri = Icon::from_nodes(&[("polygon", &[("points", "0,0 10,0 0,10")])]).unwrap();
    assert!(tri.cubics()[0].closed);
    assert!((arc_length(&tri.cubics()[0]) - (20.0 + 200f64.sqrt())).abs() < 1e-9);

    let open = Icon::from_nodes(&[("polyline", &[("points", "0 0 10 0 10 10")])]).unwrap();
    assert!(!open.cubics()[0].closed);
    assert!((arc_length(&open.cubics()[0]) - 20.0).abs() < 1e-9);
}

#[test]
fn unsupported_tags_and_bad_points_error() {
    assert_eq!(
        Icon::from_nodes(&[("g", &[])]).unwrap_err(),
        Error::UnsupportedTag("g".into())
    );
    assert!(matches!(
        Icon::from_nodes(&[("polyline", &[("points", "1 x")])]),
        Err(Error::InvalidPoints(_))
    ));
    assert_eq!(Icon::from_d("").unwrap_err(), Error::Empty);
}

#[test]
fn canonical_d_round_trips_within_emission_precision() {
    let icon = Icon::from_d("M12 2a10 10 0 1 0 0.001 0M3 3l7 7").unwrap();
    let again = Icon::from_d(icon.d()).unwrap();
    assert_eq!(icon.cubics().len(), again.cubics().len());
    for (a, b) in icon.cubics().iter().zip(again.cubics()) {
        assert!(max_diff(&a.points, &b.points) < 5e-5);
    }
    assert_eq!(icon, again);
}

// ------------------------------------------------------------- resampling

#[test]
fn check_is_resampled_with_exact_endpoints_and_anchored_corner() {
    let check = icon("check");
    let s = &check.samples()[0].points;
    assert_eq!(s.len(), SAMPLES);
    assert_eq!(s[0], p(20.0, 6.0));
    assert_eq!(s[SAMPLES - 1], p(4.0, 12.0));
    assert!(s.contains(&p(9.0, 17.0)), "corner is a sample, bit-exact");
}

#[test]
fn arc_length_equidistance_within_a_run() {
    let icon = Icon::from_d("M0 0C10 20 20 -20 30 0").unwrap();
    let s = &icon.samples()[0].points;
    let d: Vec<f64> = s.windows(2).map(|w| w[0].distance(w[1])).collect();
    let mean = d.iter().sum::<f64>() / d.len() as f64;
    assert!(d.iter().all(|x| (x - mean).abs() / mean < 0.02));
}

#[test]
fn closed_square_has_four_exact_vertices() {
    let icon = Icon::from_d("M2 2H22V22H2Z").unwrap();
    let s = &icon.samples()[0];
    assert!(s.closed);
    for v in [p(2.0, 2.0), p(22.0, 2.0), p(22.0, 22.0), p(2.0, 22.0)] {
        assert!(s.points.contains(&v));
    }
}

#[test]
fn too_many_corners_for_n_is_an_error() {
    let zig: String = std::iter::once("M0 0".to_string())
        .chain((1..80).map(|i| format!("L{} {}", i, (i % 2) * 5)))
        .collect();
    assert!(matches!(
        Icon::from_d(&zig),
        Err(Error::TooManyCorners { .. })
    ));
}

#[test]
fn degenerate_subpath_fills_without_nan() {
    let c = CubicPath {
        points: vec![p(3.0, 3.0); 4],
        closed: false,
    };
    let s = resample_path(&c, 16, CORNER_THRESHOLD).unwrap();
    assert_eq!(s, vec![p(3.0, 3.0); 16]);
}

// ------------------------------------------------------ emergent rotations

#[test]
fn arrow_right_to_arrow_down_rotates_90_rigidly() {
    let plan = plan("arrow-right", "arrow-down");
    assert_eq!(plan.items().len(), 2);
    for it in plan.items() {
        assert!((deg(it.theta()).abs() - 90.0).abs() < 0.5);
        assert!((it.sigma() - 1.0).abs() < 1e-6);
        assert!(it.residual() < 1e-6);
    }
    // global hybrid: both parts share the same θ
    assert!((plan.items()[0].theta() - plan.items()[1].theta()).abs() < 1e-6);
}

#[test]
fn plus_to_x_rotates_45_and_scales() {
    for it in plan("plus", "x").items() {
        assert!((deg(it.theta()).abs() - 45.0).abs() < 0.5);
        assert!((it.sigma() - 1.2122).abs() < 0.005);
        assert!(it.residual() < 1e-6);
    }
}

#[test]
fn menu_to_x_folds_both_ways_never_135() {
    let plan = plan("menu", "x");
    assert_eq!(plan.items().len(), 3);
    let sig = |t: &[Point]| {
        let mut ends = [t[0], t[t.len() - 1]].map(|q| format!("{},{}", q.x.round(), q.y.round()));
        ends.sort();
        ends.join("|")
    };
    let sigs: std::collections::BTreeSet<String> =
        plan.items().iter().map(|it| sig(it.target())).collect();
    assert_eq!(sigs.len(), 2, "both diagonals covered");
    let mut signs = std::collections::BTreeSet::new();
    for it in plan.items() {
        assert!(it.residual() < 1e-6);
        assert!((deg(it.theta()).abs() - 45.0).abs() < 0.5);
        assert!(!it.is_block());
        signs.insert(deg(it.theta()).round().signum() as i32);
    }
    assert_eq!(signs.len(), 2, "folds toward both diagonals");
}

#[test]
fn x_to_check_duplicates_instead_of_vanishing() {
    let plan = plan("x", "check");
    assert_eq!(plan.items().len(), 2);
    let (b0, b1) = (plan.items()[0].target(), plan.items()[1].target());
    let rev: Vec<Point> = b1.iter().rev().copied().collect();
    assert_eq!(max_diff(b0, b1).min(max_diff(b0, &rev)), 0.0);
}

#[test]
fn interpolation_is_exact_at_both_endpoints() {
    for (a, b) in [
        ("menu", "x"),
        ("x", "check"),
        ("arrow-right", "arrow-down"),
        ("check", "plus"),
    ] {
        let plan = plan(a, b);
        let mut out = plan.outputs();
        plan.interpolate(0.0, &mut out);
        for (it, o) in plan.items().iter().zip(&out) {
            assert!(max_diff(o, it.source()) < 1e-9, "{a}→{b} at 0");
        }
        plan.interpolate(1.0, &mut out);
        for (it, o) in plan.items().iter().zip(&out) {
            assert!(max_diff(o, it.target()) < 1e-9, "{a}→{b} at 1");
        }
        plan.interpolate_linear(0.0, &mut out);
        for (it, o) in plan.items().iter().zip(&out) {
            assert!(max_diff(o, it.source()) < 1e-9);
        }
    }
}

#[test]
fn mid_flight_d_has_one_m_per_subpath() {
    let plan = plan("menu", "x");
    let mut out = plan.outputs();
    plan.interpolate(0.5, &mut out);
    let d = polylines_to_d(&out, &[]);
    assert!(d.starts_with('M'));
    assert_eq!(d.matches('M').count(), 3);
    assert!(!d.contains("NaN"));
}

// ----------------------------------------------------------- closed loops

fn square() -> Icon {
    Icon::from_nodes(&[(
        "rect",
        &[("x", "2"), ("y", "2"), ("width", "20"), ("height", "20")],
    )])
    .unwrap()
}

#[test]
fn same_square_different_start_point_is_identity() {
    let plan = Plan::new(
        square().samples(),
        Icon::from_d("M12 2H22V22H2V2Z").unwrap().samples(),
    );
    let it = &plan.items()[0];
    assert!(it.closed());
    assert!(it.residual() < 1e-6);
    assert!(deg(it.theta()).abs() < 0.5);
    assert!((it.sigma() - 1.0).abs() < 1e-6);
}

#[test]
fn square_to_diamond_rotates_45_and_scales() {
    let diamond = Icon::from_nodes(&[("polygon", &[("points", "12 2 22 12 12 22 2 12")])]).unwrap();
    let it = &Plan::new(square().samples(), diamond.samples()).items()[0].clone();
    assert!(it.residual() < 1e-6);
    assert!((deg(it.theta()).abs() - 45.0).abs() < 0.5);
    assert!((it.sigma() - FRAC_1_SQRT_2).abs() < 1e-3);
}

#[test]
fn circles_translate_and_scale_without_rotating() {
    let small = Icon::from_elements(&[Element::Circle {
        cx: 6.0,
        cy: 6.0,
        r: 4.0,
    }])
    .unwrap();
    let large = Icon::from_elements(&[Element::Circle {
        cx: 16.0,
        cy: 16.0,
        r: 8.0,
    }])
    .unwrap();
    let it = &Plan::new(small.samples(), large.samples()).items()[0].clone();
    assert!(deg(it.theta()).abs() < 0.5);
    assert!((it.sigma() - 2.0).abs() < 1e-6);
}

#[test]
fn closed_to_open_flies_open() {
    let plan = Plan::new(square().samples(), icon("check").samples());
    assert!(!plan.items()[0].closed());
    let plan = Plan::new(square().samples(), icons::circle().samples());
    assert!(plan.items()[0].closed());
}

#[test]
fn block_transport_keeps_congruent_icons_rigid() {
    let plan = plan("arrow-right", "arrow-down");
    assert!(plan.items().iter().all(|it| it.is_block()));
    let mut out = plan.outputs();
    let n = plan.sample_count();
    let pick = [0, n / 2, n - 1];
    let dists = |out: &[Vec<Point>]| -> Vec<f64> {
        pick.iter()
            .flat_map(|&i| pick.iter().map(move |&j| out[0][i].distance(out[1][j])))
            .collect()
    };
    plan.interpolate(0.0, &mut out);
    let d0 = dists(&out);
    for t in [0.25, 0.5, 0.75] {
        plan.interpolate(t, &mut out);
        for (d, e) in dists(&out).iter().zip(&d0) {
            assert!((d - e).abs() < 1e-6, "rigid mid-flight at t={t}");
        }
    }
}

// ---------------------------------------------------------------- matching

fn dots(k: usize, dx: f64, dy: f64) -> Icon {
    let els: Vec<Element> = (0..k)
        .map(|i| Element::Circle {
            cx: dx + 4.0 + (i % 4) as f64 * 6.0,
            cy: dy + 4.0 + (i / 4) as f64 * 6.0,
            r: 1.5,
        })
        .collect();
    Icon::from_elements(&els).unwrap()
}

#[test]
fn twelve_dots_to_menu_covers_all_targets() {
    let plan = Plan::new(dots(12, 0.0, 0.0).samples(), icon("menu").samples());
    assert_eq!(plan.items().len(), 12);
    let targets: std::collections::BTreeSet<i64> = plan
        .items()
        .iter()
        .map(|it| (it.target_centroid().y * 100.0).round() as i64)
        .collect();
    assert_eq!(targets.len(), 3);
    let mut out = plan.outputs();
    plan.interpolate(0.5, &mut out);
    out.iter().for_each(|o| assert_finite(o));
}

#[test]
fn nine_dots_pair_with_their_translated_twins() {
    let plan = Plan::new(dots(9, 0.0, 0.0).samples(), dots(9, 2.0, 3.0).samples());
    assert_eq!(plan.items().len(), 9);
    for it in plan.items() {
        assert!(it.residual() < 1e-6);
        let d = it.target_centroid() - it.source_centroid();
        assert!((d.x - 2.0).abs() < 1e-9 && (d.y - 3.0).abs() < 1e-9);
    }
}

// ------------------------------------------------------------------ fitting

#[test]
fn fit_regrids_foreign_view_boxes() {
    let on20 = Icon::from_d("M2 2L18 18").unwrap();
    let fitted = on20.fit(20.0).unwrap();
    assert!(max_diff(&fitted.cubics()[0].points[..1], &[p(2.4, 2.4)]) < 1e-9);
    let same = icon("check").fit(24.0).unwrap();
    assert_eq!(same, icon("check"));
    let shifted = Icon::from_d("M10 10L20 20")
        .unwrap()
        .fit("10 10 24 24".parse::<ViewBox>().unwrap())
        .unwrap();
    assert_eq!(shifted.cubics()[0].points[0], p(0.0, 0.0));
    // non-square: 48×24 scales by 0.5 and centers vertically
    let wide = Icon::from_d("M0 0L48 24")
        .unwrap()
        .fit([0.0, 0.0, 48.0, 24.0])
        .unwrap();
    assert_eq!(wide.cubics()[0].points[0], p(0.0, 6.0));
    assert!(on20.fit(0.0).is_err());
    assert!("1 2 3".parse::<ViewBox>().is_err());
}

// -------------------------------------------------------------------- spring

#[test]
fn snappy_spring_settles_quickly() {
    let mut s = Spring {
        config: SpringConfig::SNAPPY,
        ..Spring::default()
    };
    s.start();
    let mut t = 0.0;
    let mut settled = false;
    while t < 5.0 && !settled {
        settled = s.step(1.0 / 60.0);
        t += 1.0 / 60.0;
    }
    assert!(settled && t < 1.5, "settled in {t}s");
}

#[test]
fn interruptions_replan_from_the_intermediate_shape() {
    let mut spring = Spring {
        config: SpringConfig::SNAPPY,
        ..Spring::default()
    };
    let mut plan = plan("menu", "x");
    let mut out = plan.outputs();
    spring.start();
    for next in ["check", "plus", "arrow-right", "x"] {
        for _ in 0..7 {
            spring.step(1.0 / 60.0);
        }
        plan.interpolate(spring.x, &mut out);
        out.iter().for_each(|o| assert_finite(o));
        let v = spring.v;
        let src: Vec<Sampled> = out
            .iter()
            .zip(plan.items())
            .map(|(o, it)| Sampled {
                points: o.clone(),
                closed: it.closed(),
            })
            .collect();
        plan = Plan::new(&src, icon(next).samples());
        out = plan.outputs();
        spring.start();
        assert_eq!(spring.v, v.clamp(-14.0, 14.0));
    }
    let mut settled = false;
    for _ in 0..300 {
        settled = spring.step(1.0 / 60.0);
        plan.interpolate(spring.x, &mut out);
        out.iter().for_each(|o| assert_finite(o));
        if settled {
            break;
        }
    }
    assert!(settled);
    plan.interpolate(1.0, &mut out);
    for (it, o) in plan.items().iter().zip(&out) {
        assert!(max_diff(o, it.target()) < 1e-9);
    }
}

// -------------------------------------------------------------------- driver

fn run(m: &mut Morph) -> usize {
    let mut frames = 0;
    let mut now = 0.0;
    while m.update(now) {
        assert!(!m.path_d().contains("NaN"));
        frames += 1;
        now += 1.0 / 60.0;
        assert!(frames < 600, "never settled");
    }
    frames
}

#[test]
fn morph_flies_then_snaps_to_canonical_d() {
    let mut m = Morph::new(icons::menu());
    assert_eq!(m.path_d(), icons::menu().d());
    m.morph_to(icons::x(), SpringConfig::SNAPPY);
    assert!(m.is_animating());
    assert!(m.path_d().contains('L'), "polyline in flight");
    let frames = run(&mut m);
    assert!(frames > 5);
    assert!(m.is_at_rest());
    assert_eq!(m.path_d(), icons::x().d());
    assert_eq!(m.progress(), 1.0);
}

#[test]
fn first_frame_of_a_flight_does_not_advance() {
    let mut m = Morph::new(icons::menu());
    m.update(100.0); // idle frames long before the flight
    m.morph_to(icons::x(), SpringConfig::SNAPPY);
    let before = m.path_d();
    m.update(250.0);
    assert_eq!(m.path_d(), before);
    assert_eq!(m.progress(), 0.0);
}

#[test]
fn morph_to_the_current_target_is_a_no_op() {
    let mut m = Morph::new(icons::menu());
    m.morph_to(icons::menu(), SpringConfig::SNAPPY);
    assert!(!m.is_animating());
    m.morph_to(icons::x(), SpringConfig::SNAPPY);
    m.tick(0.05);
    let d = m.path_d();
    m.morph_to(icons::x(), SpringConfig::SNAPPY);
    assert_eq!(m.path_d(), d, "already en route");
}

#[test]
fn interruption_replans_from_the_exact_intermediate_shape() {
    let mut m = Morph::new(icons::menu());
    m.morph_to(icons::x(), SpringConfig::SNAPPY);
    m.tick(0.05);
    m.tick(0.05);
    let mid = m.path_d();
    m.morph_to(icons::check(), SpringConfig::SNAPPY);
    assert_eq!(m.path_d(), mid, "re-planned from what was on screen");
    run(&mut m);
    assert_eq!(m.target(), &icons::check());
    assert_eq!(m.path_d(), icons::check().d());
}

#[test]
fn seek_is_frozen_and_deterministic() {
    let mut a = Morph::new(icons::menu());
    a.seek(icons::x(), 0.4);
    assert!(!a.is_animating());
    assert_eq!(a.progress(), 0.4);
    let mut b = Morph::new(icons::menu());
    b.seek(icons::x(), 0.4);
    assert_eq!(a.path_d(), b.path_d());
    assert!(!a.tick(0.1));
    assert_eq!(a.progress(), 0.4);
    a.set_progress(0.9);
    assert_eq!(a.progress(), 0.9);
    // a later morph_to takes off from the frozen shape
    let frozen = a.path_d();
    a.morph_to(icons::check(), SpringConfig::SMOOTH);
    assert_eq!(a.path_d(), frozen);
}

#[test]
fn set_jumps_and_cancels_the_flight() {
    let mut m = Morph::new(icons::menu());
    m.morph_to(icons::x(), SpringConfig::BOUNCY);
    m.tick(0.05);
    m.set(icons::check());
    assert!(!m.is_animating());
    assert!(m.is_at_rest());
    assert_eq!(m.path_d(), icons::check().d());
}

#[test]
fn reduced_motion_jumps() {
    let mut m = Morph::new(icons::menu());
    m.set_reduced_motion(true);
    m.morph_to(icons::x(), SpringConfig::SNAPPY);
    assert!(!m.is_animating());
    assert_eq!(m.path_d(), icons::x().d());
}

#[test]
fn closed_paths_fly_with_z_and_snap_back_to_curves() {
    let mut m = Morph::new(icons::circle());
    m.morph_to(icons::square(), SpringConfig::SNAPPY);
    m.tick(0.05);
    assert!(m.path_d().ends_with('Z'));
    run(&mut m);
    assert_eq!(m.path_d(), icons::square().d());
}

#[test]
fn custom_springs_settle() {
    let mut m = Morph::new(icons::play());
    m.morph_to(icons::pause(), SpringConfig::new(200.0, 20.0));
    run(&mut m);
    assert_eq!(m.target(), &icons::pause());
}

#[test]
fn every_builtin_pair_morphs_cleanly() {
    let all = icons::all();
    for (an, a) in &all {
        for (bn, b) in &all {
            let plan = Plan::new(a.samples(), b.samples());
            let mut out = plan.outputs();
            for t in [0.0, 0.3, 0.7, 1.0, 1.1] {
                plan.interpolate(t, &mut out);
                for o in &out {
                    assert!(o.iter().all(|q| q.is_finite()), "{an}→{bn} at {t}");
                }
            }
        }
    }
}

// ------------------------------------------------------------------- sinks

#[test]
fn flattener_respects_tolerance_and_closes_without_duplicates() {
    let lines = Morph::new(icons::circle()).flatten(0.01);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].closed);
    assert_ne!(lines[0].points.first(), lines[0].points.last());
    for q in &lines[0].points {
        assert!((q.distance(p(12.0, 12.0)) - 10.0).abs() < 0.02);
    }
    let menu = Morph::new(icons::menu()).flatten(0.1);
    assert_eq!(menu.len(), 3);
    assert!(
        menu.iter().all(|l| l.points.len() == 2),
        "straight lines stay 2 points"
    );
}

// -------------------------------------------------------------- controller

#[test]
fn controller_uncontrolled_animates_on_change_only() {
    let mut c = Controller::new(icons::menu().into());
    c.sync(&icons::menu().into(), SpringConfig::SNAPPY);
    assert!(!c.morph().is_animating());
    c.sync(&icons::x().into(), SpringConfig::SNAPPY);
    assert!(c.morph().is_animating());
    // an imperative call isn't overridden while the source stays the same
    c.set(icons::check());
    c.sync(&icons::x().into(), SpringConfig::SNAPPY);
    assert_eq!(c.morph().target(), &icons::check());
}

#[test]
fn controller_controlled_pair_and_handoff() {
    let between = |t| Source::Between {
        from: icons::menu(),
        to: icons::x(),
        progress: t,
    };
    let mut c = Controller::new(between(0.0));
    assert_eq!(c.morph().path_d(), icons::menu().d());
    c.sync(&between(0.5), SpringConfig::SNAPPY);
    let half = c.morph().path_d();
    assert!(!c.morph().is_animating());
    c.sync(&between(1.0), SpringConfig::SNAPPY);
    assert_eq!(c.morph().path_d(), icons::x().d());
    c.sync(&between(0.5), SpringConfig::SNAPPY);
    assert_eq!(c.morph().path_d(), half, "re-based on `from`");
    // dropping the pair hands the path back to the icon, animated
    c.sync(&icons::check().into(), SpringConfig::SNAPPY);
    assert!(c.morph().is_animating());
    assert_eq!(c.morph().target(), &icons::check());
}
