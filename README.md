# morphicons-rs

Universal morphing for stroke-based icons in Rust: **any icon morphs into any
other** with spring physics. Rotations are never declared by hand. They come out
of the math (closed-form 2D Procrustes plus polar interpolation):
arrow-right → arrow-down turns 90°, plus → x turns 45° and grows, and menu → x
folds each bar toward its diagonal.

This is a Rust port of [morphicons](https://github.com/guillermolg00/morphicons)
by Guillermo (MIT). The math, constants and invariants match upstream; the
architecture is adapted to Rust UI frameworks, where the framework owns the
frame loop.

| crate | what | status |
|---|---|---|
| [`morphicons`](crates/morphicons) | core: parse → normalize → resample → plan → interpolate, spring, driver. Zero dependencies, no rendering. | ✅ |
| [`morphicons-egui`](crates/morphicons-egui) | egui 0.36 widget | ✅ |
| [`morphicons-iced`](crates/morphicons-iced) | iced 0.14 widget | ✅ |
| [`morphicons-gallery`](crates/morphicons-gallery) | demo support, not published: ~245 Lucide / Heroicons / Tabler icons and the playground readout | demo |
| Bevy, GPUI, Freya, Dioxus, Leptos | see [Future bindings](#future-bindings) | planned |

## Try it

The playgrounds recreate [morphicons.com](https://www.morphicons.com): a
glowing stage with the math readout (θ per subpath, residual, and whether the
morph is a pure rotation, rotation + residual, or cell division), a scrubber
that freezes any pair at t, your icon set, and a library of about 245 Lucide,
Heroicons and Tabler icons that all morph into each other. As on the site, the
stage loops through your set every 1.3 s; the round button pauses and resumes
it, and scrubbing pauses it.

```bash
cargo run --release -p morphicons-egui --example egui_playground
cargo run --release -p morphicons-iced --example iced_playground
```

The smaller demos show the API's three modes side by side:

```bash
cargo run -p morphicons-egui --example egui_demo
cargo run -p morphicons-iced --example iced_demo
cargo test --workspace
```

## Usage

### egui

```rust
use morphicons_egui::{MorphIcon, morphicons::icons};

// Uncontrolled: pass the icon; when it changes, the widget animates.
let icon = if open { icons::x() } else { icons::menu() };
if ui.add(MorphIcon::new(icon).size(32.0).sense(egui::Sense::click())).clicked() {
    open = !open;
}

// Controlled: from → to frozen at a progress you drive (gestures, sliders).
ui.add(MorphIcon::between(icons::play(), icons::pause(), progress));

// Imperative: you own a Morph and call morph_to / set / seek on it.
ui.add(MorphIcon::morph(&mut my_morph));
```

The widget reads time from `ui.input(|i| i.time)` and only requests repaints
while something is moving. For static icons (toolbars, galleries),
`paint_icon(painter, rect, &icon, stroke, color)` draws without any widget
state, and `paint_morph` draws a `Morph` you tick yourself. Uncontrolled state lives in egui memory under the
widget's id; pass `.id_salt(...)` if the icon can move around in the layout.

### iced

```rust
use iced::widget::button;
use morphicons_iced::{morph_icon, MorphIcon, morphicons::icons};

// Uncontrolled: animates on its own. No subscription needed.
let icon = if self.open { icons::x() } else { icons::menu() };
button(morph_icon(icon).size(32.0)).on_press(Message::Toggle)

// Controlled
MorphIcon::between(icons::arrow_right(), icons::arrow_down(), self.progress)
```

The widget is an iced `canvas` that advances on each `RedrawRequested` event
and requests the next frame only while moving. At rest it reuses cached
geometry, so hundreds of static icons stay cheap while one of them animates. It depends on `iced_widget`, not
the `iced` facade, so it works with whatever renderer and executor your app
picks. For the imperative mode (`MorphIcon::morph(&morph)`), tick the morph
from `window::frames()` while `morph.is_animating()`, using
`morphicons_iced::seconds(now)`.

### Core, no framework

```rust
use morphicons::{icons, Icon, Morph, SpringConfig};

let mut morph = Morph::new(icons::menu());
morph.morph_to(icons::x(), SpringConfig::SNAPPY);

// Each frame, from your framework's clock (seconds):
let still_moving = morph.update(now);
morph.draw(&mut my_sink);        // or morph.path_d() for an SVG `d`
                                 // or morph.flatten(tol) for polylines
```

`my_sink` is anything implementing `PathSink` (`move_to`, `line_to`,
`cubic_to`, `close`), which is the only contract a renderer has to meet. At
rest you get the target's exact cubics; in flight you get 64-point polylines
per subpath.

### Icons

```rust
use morphicons::{Icon, Element};

let a = Icon::from_d("M4 6h16M4 12h16M4 18h16")?;          // SVG path data
let b = Icon::from_nodes(&[                                  // Lucide IconNode shape
    ("circle", &[("cx", "12"), ("cy", "12"), ("r", "10")]),
    ("path", &[("d", "m9 12 2 2 4-4")]),
])?;
let c = Icon::from_elements(&[Element::Circle { cx: 12.0, cy: 12.0, r: 3.0 }])?;
let d = Icon::from_d(carbon_d)?.fit(32.0)?;                  // re-grid a 32×32 pack onto 24
```

Supported primitives: `path`, `line`, `circle`, `ellipse`, `rect` (incl. rounded),
`polyline`, `polygon`. Icons must be stroke-drawn (Lucide, Tabler, Heroicons
outline, Iconoir, Feather…) and share a grid; use `Icon::fit` for other grids.
`Icon` is parsed and resampled once and is cheap to clone (`Arc`). Build icons
at startup or in a `LazyLock`, not per frame. `morphicons::icons` has a small
built-in set for toggles and demos.

## How it works

Same pipeline as upstream (see its README for the full derivation):

1. **Normalize**: every primitive becomes cubic Béziers (lines, quadratics by
   degree elevation, arcs via SVG F.6 center parametrization sliced at ≤ 90°).
2. **Resample**: each subpath becomes 64 points equidistant by arc length
   (8-point Gauss-Legendre), with corners anchored as exact samples.
3. **Correspond**: both traversal directions, all circular offsets for closed
   loops, and subpath matching by centroid + length cost (surjective when
   counts differ: leftovers duplicate instead of vanishing).
4. **Align**: closed-form Procrustes per subpath (θ, σ, residual), a
   minimal-rotation tie-break, and a global hybrid that makes congruent icons
   rotate as one rigid block.
5. **Interpolate** in polar space: `P(t) = c(t) + σᵗ·R(tθ)·lerp(aᶜ, b̃, t)`,
   exact at both endpoints, extrapolating naturally on spring overshoot.
6. **Spring**: semi-implicit Euler at 1/240 s substeps; interruptions re-plan
   from the on-screen shape and keep velocity.

### Differences from upstream

- **The host owns the clock.** There is no global `requestAnimationFrame`;
  `Morph::update(now)` / `tick(dt)` run inside whatever loop the framework has.
- **No string round-trip.** Renderers get points through `PathSink`; the SVG
  `d` string (`path_d()`) is only produced when a backend wants SVG.
- **`Icon` is a parsed value.** Upstream caches by object identity in
  `WeakMap`s; here parsing and resampling happen once in `Icon::from_*`.
- **Reduced motion is a flag** (`Morph::set_reduced_motion`), since there is
  no portable OS query. Feed it from your platform if you want to honor it.
- **`Controller`** (upstream's binding controller) lives in the core, so every
  binding shares one implementation of the uncontrolled/controlled lifecycle.

## Future bindings

Every binding is the same three pieces: **state** (a `Controller` per icon),
**time** (call `controller.update(now_seconds)` once per frame and keep frames
coming while it returns `true`), and **drawing** (implement `PathSink` for the
framework's path type, or use `path_d()` / `flatten()`). The egui and iced
crates are the reference implementations, at about 250 lines each.

Versions below were current on crates.io in September 2026. APIs in this
space move fast, so check each framework's docs before starting.

### Bevy (0.19)

- **State**: a `MorphIcon` component holding a `Controller`, plus a source
  component (`MorphIconSource(Source)`) the game mutates. A system with
  `Changed<MorphIconSource>` calls `controller.sync(...)`.
- **Time**: a system in `Update` calls `controller.update(time.elapsed_secs_f64())`
  with `Res<Time>`. Bevy renders every frame anyway, so there is nothing to
  request.
- **Drawing**, from quickest to best:
  1. `Gizmos::linestrip_2d` with `morph.flatten(tol)`. Good for prototypes,
     but gizmos are debug-quality lines.
  2. Stroke tessellation with `lyon_tessellation` (round caps and joins) into
     a `Mesh` on a `Mesh2d` with `MeshMaterial2d<ColorMaterial>`. Implement
     `PathSink` for a lyon `path::Builder`, and re-tessellate only while
     `is_animating()` or after `sync` changed something.
  3. `bevy_vello`, if you already render vector content with vello.
  For `bevy_ui`, rasterize into an `Image` (e.g. `tiny-skia`) shown with
  `ImageNode`, or use a `UiMaterial`.

### GPUI (0.2, Zed's UI framework)

- **State**: a `Controller` inside an `Entity<MorphIconState>`, or in a view
  struct; the component is a `RenderOnce` / `IntoElement` wrapper.
- **Time**: while animating, call `window.request_animation_frame()` from
  paint, and drive `update` from `std::time::Instant` relative to a stored
  epoch.
- **Drawing**: a `canvas(prepaint, paint)` element. Implement `PathSink` for
  `gpui::PathBuilder::stroke(width)` (map grid to bounds), then
  `window.paint_path(path, color)`.
- The crates.io release trails Zed's repo; many projects pin a git revision.
  Decide which one you target first.

### Freya (0.4, Dioxus-style components on Skia)

- **State**: `use_signal(|| Controller::new(...))` or `use_hook`. Compare the
  `icon` prop in the component body (or with `use_memo`) and call `sync`.
- **Time**: Freya has a frame-driven animation system (`use_animation`) and
  an async runtime. Run a loop that awaits the next frame and calls `update`
  while it returns `true`.
- **Drawing**: the canvas API hands you a Skia canvas. Implement `PathSink`
  for `skia_safe::Path` (`move_to`, `line_to`, `cubic_to`, `close`) and stroke
  it with a `Paint` using round caps and joins. The simplest fallback is the
  `svg` element fed `<svg viewBox="0 0 24 24"><path d="{morph.path_d()}" …/></svg>`
  bytes each frame, which is fine for a handful of icons.

### Dioxus (0.7)

- **Component**: `#[component] fn MorphIcon(icon: Icon, …)`. `Icon` is
  `Clone + PartialEq`, as props require. Render
  `svg { view_box: "0 0 24 24", path { d: "{d}", stroke_linecap: "round", … } }`.
- **State**: `use_signal(|| Controller::new(icon.clone().into()))`;
  `use_effect` on the `icon` prop calls `sync`, then starts the frame loop.
- **Time**: `spawn` a future that loops while `update` returns `true`,
  awaiting one frame per iteration: `requestAnimationFrame` via
  `web-sys`/`gloo-render` on web, `tokio::time::sleep(16ms)` on desktop.
  Write the new `path_d()` into a `Signal<String>`.
- **Perf note**: a signal write re-renders the component every frame, which
  is fine for a few icons. For many, grab the element with `onmounted` and set
  `d` directly on web, the way upstream's DOM driver does.
- **SSR/hydration**: render `icon.d()` (the canonical 4-decimal `d`) at rest,
  so the server and client bytes match.

### Leptos (0.8)

- **Component**: `#[component] fn MorphIcon(#[prop(into)] icon: Signal<Icon>)`
  returning `<svg viewBox="0 0 24 24"><path d=move || d.get() … /></svg>`.
- **State**: `StoredValue<Controller>` plus an `RwSignal<String>` for `d`
  (everything in core is `Send + Sync`, as Leptos 0.8 requires).
- **Time**: `Effect::new` tracks `icon`, calls `sync`, and starts a loop with
  `request_animation_frame` that calls `update(performance.now() / 1000.0)`,
  writes `d`, and reschedules while animating.
- **SSR/hydration**: start `d` at `icon.get_untracked().d()`. The effect only
  runs on the client, so the server markup is the static icon, exactly like
  upstream.

### Checklist for a new binding

- [ ] Uncontrolled, controlled (`Source::Between`) and imperative modes.
- [ ] Frames requested only while `update` returns `true`.
- [ ] Grid-to-bounds mapping (default 24), `size`, `stroke_width` (grid units
      by default, plus an absolute option), `color` defaulting to the theme's
      text color.
- [ ] Round caps and joins.
- [ ] An example with a toggle, a slider, and a gallery.

## Development notes

- The playground icon data is generated from the real npm packages by
  [`crates/morphicons-gallery/generate.mjs`](crates/morphicons-gallery/generate.mjs)
  (instructions at the top of the file). Licenses for the icon sets and the
  Geist fonts are in
  [`THIRD_PARTY_LICENSES.md`](crates/morphicons-gallery/THIRD_PARTY_LICENSES.md).
- `[profile.dev.package."*"] debug = false` in the workspace keeps `target/`
  smaller: dependencies build without debug info, while our crates keep it.
- Minimum Rust: core 1.85, egui crate 1.95 (egui's MSRV), iced crate 1.88.

## License

MIT, like upstream. See [LICENSE](LICENSE).
