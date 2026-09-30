//! The morphicons playground, in egui (after morphicons.com):
//! `cargo run -p morphicons-egui --example egui_playground --release`
//!
//! A stage with the big morphing icon and the math readout, a scrubber that
//! freezes the pair at any t, your icon set, and a library of Lucide,
//! Heroicons and Tabler icons that all morph into each other.

use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, FontTweak, Mesh, Painter,
    Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
    epaint::text::VariationCoords, pos2, vec2,
};
use morphicons_egui::morphicons::{Icon, Morph, SpringConfig};
use morphicons_egui::{MorphIcon, paint_icon, paint_morph};
use morphicons_gallery::{self as gallery, Lib, Readout, fonts};

// The morphicons.com palette.
const BG: Color32 = Color32::from_rgb(10, 10, 10);
const INK: Color32 = Color32::from_rgb(237, 237, 237);
const BODY: Color32 = Color32::from_rgb(161, 161, 161);
const MUTE: Color32 = Color32::from_rgb(125, 125, 125);
const HAIRLINE: Color32 = Color32::from_rgb(38, 38, 38);
const CANVAS: Color32 = Color32::from_rgb(26, 26, 26);
const TILE: Color32 = Color32::from_rgb(19, 19, 19);

const SPRINGS: [(&str, SpringConfig); 3] = [
    ("smooth", SpringConfig::SMOOTH),
    ("snappy", SpringConfig::SNAPPY),
    ("bouncy", SpringConfig::BOUNCY),
];
const STROKES: [(&str, f32); 4] = [("1", 1.0), ("1.5", 1.5), ("2", 2.0), ("2.5", 2.5)];
/// How long each icon of the sequence holds the stage (as on the site).
const STEP: f64 = 1.3;
const TABS: [(&str, Option<Lib>); 4] = [
    ("All", None),
    ("Lucide", Some(Lib::Lucide)),
    ("Heroicons", Some(Lib::Heroicons)),
    ("Tabler", Some(Lib::Tabler)),
];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("morphicons · egui playground")
            .with_inner_size([1180.0, 840.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "morphicons-playground",
        options,
        Box::new(|cc| {
            install_style(&cc.egui_ctx);
            Ok(Box::new(Playground::new()))
        }),
    )
}

fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn install_style(ctx: &egui::Context) {
    let weighted = |bytes: &'static [u8], wght: f32| {
        Arc::new(FontData::from_static(bytes).tweak(FontTweak {
            coords: VariationCoords::new([("wght", wght)]),
            ..Default::default()
        }))
    };
    let mut defs = FontDefinitions::default();
    let fallbacks = defs.families[&FontFamily::Proportional].clone();
    for (name, bytes, wght) in [
        ("geist", fonts::GEIST, 400.0),
        ("geist-medium", fonts::GEIST, 500.0),
        ("geist-bold", fonts::GEIST, 640.0),
        ("geist-mono", fonts::GEIST_MONO, 400.0),
    ] {
        defs.font_data.insert(name.into(), weighted(bytes, wght));
    }
    let family = |first: &str| [vec![first.to_string()], fallbacks.clone()].concat();
    defs.families
        .insert(FontFamily::Proportional, family("geist"));
    defs.families
        .insert(FontFamily::Name("medium".into()), family("geist-medium"));
    defs.families
        .insert(FontFamily::Name("bold".into()), family("geist-bold"));
    let mono_fallbacks = defs.families[&FontFamily::Monospace].clone();
    defs.families.insert(
        FontFamily::Monospace,
        [vec!["geist-mono".to_string()], mono_fallbacks].concat(),
    );
    ctx.set_fonts(defs);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = CANVAS;
    visuals.window_stroke = Stroke::new(1.0, HAIRLINE);
    visuals.popup_shadow = egui::Shadow::NONE;
    visuals.override_text_color = Some(INK);
    visuals.extreme_bg_color = BG;
    visuals.selection.bg_fill = Color32::from_rgb(40, 60, 110);
    ctx.set_visuals(visuals);
}

struct Playground {
    /// "Your set": gallery ids, in order.
    set: Vec<usize>,
    current: usize,
    prev: Option<usize>,
    /// The stage icon: an app-owned morph (the imperative mode).
    morph: Morph,
    readout: Option<Readout>,
    /// `Some(t)` while the pair is frozen by the scrubber.
    scrub: Option<f64>,
    spring: usize,
    stroke: usize,
    tab: usize,
    query: String,
    /// The stage loops through your set while playing.
    playing: bool,
    /// When the sequence moves on next (egui time, seconds).
    next_at: f64,
    /// This frame's time, for picks made while laying out.
    now: f64,
    copied_at: Option<f64>,
    /// How many icons the set rows can hold at the current stage width.
    set_capacity: usize,
}

impl Playground {
    fn new() -> Self {
        let lucide = |n| gallery::find(Lib::Lucide, n).expect("gallery icon").id;
        let start = lucide("arrow-right");
        let mut p = Self {
            set: gallery::default_set(),
            current: start,
            prev: None,
            morph: Morph::new(gallery::entry(start).icon.clone()),
            readout: None,
            scrub: None,
            spring: 1,
            stroke: 2,
            tab: 0,
            query: String::new(),
            playing: true,
            next_at: STEP,
            now: 0.0,
            copied_at: None,
            set_capacity: 16,
        };
        p.select(lucide("arrow-down")); // an opening move
        p
    }

    fn spring(&self) -> SpringConfig {
        SPRINGS[self.spring].1
    }

    fn stroke(&self) -> f32 {
        STROKES[self.stroke].1
    }

    fn select(&mut self, id: usize) {
        let icon = gallery::entry(id).icon.clone();
        if id != self.current {
            self.readout = Some(gallery::readout(
                gallery::entry(self.current),
                gallery::entry(id),
            ));
            self.prev = Some(self.current);
            self.current = id;
        }
        self.scrub = None;
        self.morph.morph_to(icon, self.spring());
    }

    /// A manual pick: morph there and restart the sequence's countdown.
    fn pick(&mut self, id: usize) {
        self.select(id);
        self.next_at = self.now + STEP;
    }

    /// Morphs the stage to the icon after the current one in your set.
    fn advance(&mut self) {
        if self.set.is_empty() {
            return;
        }
        let at = self
            .set
            .iter()
            .position(|&id| id == self.current)
            .map_or(0, |i| (i + 1) % self.set.len());
        self.select(self.set[at]);
    }

    fn scrub_to(&mut self, t: f64) {
        let Some(prev) = self.prev else { return };
        self.playing = false; // scrubbing takes over the stage
        if self.scrub.is_none() {
            self.morph.set(gallery::entry(prev).icon.clone()); // back to the pair's origin, once
        }
        self.scrub = Some(t);
        self.morph
            .seek(gallery::entry(self.current).icon.clone(), t);
    }
}

impl eframe::App for Playground {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let now = ui.input(|i| i.time);
        self.now = now;
        if self.playing && self.set.len() > 1 {
            if now >= self.next_at {
                self.advance();
                self.next_at = now + STEP;
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(
                    (self.next_at - now).max(0.0),
                ));
        }
        if self.morph.update(now) {
            ui.ctx().request_repaint();
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0.0, BG);
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| self.page(ui, now));
        });
    }
}

impl Playground {
    fn page(&mut self, ui: &mut Ui, now: f64) {
        ui.add_space(44.0);
        ui.vertical_centered(|ui| {
            for line in ["Morph any SVG", "icon into any other."] {
                ui.label(
                    RichText::new(line)
                        .font(bold(62.0))
                        .color(INK)
                        .extra_letter_spacing(-2.2),
                );
            }
            ui.add_space(18.0);
            for line in [
                "Animate Lucide, Tabler, Heroicons or any stroke icon set. Optimal",
                "rotation solved in closed form, spring physics, zero dependencies.",
            ] {
                ui.label(RichText::new(line).font(font(20.0)).color(BODY));
            }
            ui.add_space(30.0);
            self.install_box(ui, now);
            ui.add_space(44.0);
            let width = (ui.available_width() - 64.0).clamp(820.0, 1120.0);
            let (card, _) = ui.allocate_exact_size(vec2(width, 624.0), Sense::hover());
            self.card(ui, card);
            ui.add_space(56.0);
        });
    }

    fn install_box(&mut self, ui: &mut Ui, now: f64) {
        let (rect, _) = ui.allocate_exact_size(vec2(400.0, 62.0), Sense::hover());
        let p = ui.painter();
        p.rect(
            rect,
            8.0,
            BG,
            Stroke::new(1.0, HAIRLINE),
            StrokeKind::Inside,
        );
        let text_pos = pos2(rect.left() + 26.0, rect.center().y);
        let dollar = p.text(text_pos, Align2::LEFT_CENTER, "$", mono(18.0), MUTE);
        p.text(
            pos2(dollar.right() + 14.0, text_pos.y),
            Align2::LEFT_CENTER,
            "cargo add morphicons",
            mono(18.0),
            INK,
        );

        // Copy button: the icon itself morphs copy → check, then back.
        let copied = self.copied_at.is_some_and(|t| now - t < 1.6);
        if copied {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }
        let lucide = |n| {
            gallery::find(Lib::Lucide, n)
                .expect("gallery icon")
                .icon
                .clone()
        };
        let icon = if copied {
            lucide("check")
        } else {
            lucide("copy")
        };
        let button = Rect::from_center_size(
            pos2(rect.right() - 34.0, rect.center().y),
            Vec2::splat(22.0),
        );
        let widget = MorphIcon::new(icon)
            .size(22.0)
            .stroke_width(1.8)
            .color(MUTE)
            .sense(Sense::click());
        let response = ui
            .put(button, widget)
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            ui.ctx().copy_text("cargo add morphicons".into());
            self.copied_at = Some(now);
        }
    }

    fn card(&mut self, ui: &mut Ui, rect: Rect) {
        ui.painter().rect(
            rect,
            16.0,
            BG,
            Stroke::new(1.0, HAIRLINE),
            StrokeKind::Inside,
        );
        let bar_h = 64.0;
        let body = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - bar_h));
        let split = body.left() + (body.width() * 0.4).round();
        let stage = Rect::from_min_max(body.min, pos2(split, body.max.y));
        let library = Rect::from_min_max(pos2(split, body.min.y), body.max);
        let bar = Rect::from_min_max(pos2(rect.min.x, body.max.y), rect.max);
        let p = ui.painter();
        p.vline(split, body.y_range(), Stroke::new(1.0, HAIRLINE));
        p.hline(rect.x_range(), body.max.y, Stroke::new(1.0, HAIRLINE));
        self.stage(ui, stage);
        self.library(ui, library);
        self.bar(ui, bar);
    }

    fn stage(&mut self, ui: &mut Ui, rect: Rect) {
        let clip = rect.shrink(1.0);
        glow(&ui.painter().with_clip_rect(clip), rect.shrink(40.0));

        // The big icon. Clicking it steps through your set.
        let icon_rect = Rect::from_center_size(
            pos2(rect.center().x, rect.top() + 132.0),
            Vec2::splat(168.0),
        );
        let response = ui.interact(icon_rect, ui.id().with("stage"), Sense::click());
        if response.clicked() {
            self.advance();
            self.next_at = self.now + STEP;
        }
        paint_morph(
            ui.painter(),
            icon_rect.shrink(16.0),
            &self.morph,
            self.stroke(),
            INK,
        );

        // Readout.
        let p = ui.painter();
        let mut y = icon_rect.bottom() + 36.0;
        let cx = rect.center().x;
        match &self.readout {
            Some(r) => {
                p.text(
                    pos2(cx, y),
                    Align2::CENTER_CENTER,
                    &r.pair,
                    mono(13.0),
                    BODY,
                );
                p.text(
                    pos2(cx, y + 22.0),
                    Align2::CENTER_CENTER,
                    &r.math,
                    mono(13.0),
                    MUTE,
                );
                p.text(
                    pos2(cx, y + 44.0),
                    Align2::CENTER_CENTER,
                    &r.verdict,
                    mono(13.0),
                    if r.pure { INK } else { MUTE },
                );
            }
            None => {
                p.text(
                    pos2(cx, y + 24.0),
                    Align2::CENTER_CENTER,
                    "tap an icon to start",
                    mono(13.0),
                    MUTE,
                );
            }
        }
        y += 92.0;

        // Scrubber: freezes prev → current at t.
        let track = Rect::from_center_size(pos2(cx - 30.0, y), vec2(300.0, 20.0));
        let t = self.scrub.unwrap_or(self.morph.progress()).clamp(0.0, 1.0);
        if let Some(t) = scrubber(ui, track, t) {
            self.scrub_to(t);
        }
        ui.painter().text(
            pos2(track.right() + 46.0, y),
            Align2::CENTER_CENTER,
            format!("t={t:.2}"),
            mono(13.0),
            MUTE,
        );
        y += 40.0;

        // Your set, then a divider and the sequence's play/pause button, all
        // flowing in centered rows like the site's wrapping row.
        let tile = 42.0;
        let gap = 9.0;
        let divider = 9.0;
        let button = 48.0;
        let max_w = rect.width() - 28.0;
        let per_row = (((max_w + gap) / (tile + gap)).floor() as usize).max(1);
        self.set_capacity = per_row * 2;
        // The divider and the button wrap together, as one unit.
        let controls = divider + gap + button;
        let widths: Vec<f32> = self.set.iter().map(|_| tile).chain([controls]).collect();
        // Greedy line breaking, then center each line.
        let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
        let mut used = 0.0;
        for (i, w) in widths.iter().enumerate() {
            let line = lines.last_mut().expect("one line");
            if !line.is_empty() && used + gap + w > max_w {
                lines.push(vec![i]);
                used = *w;
            } else {
                used += if line.is_empty() { 0.0 } else { gap } + w;
                lines.last_mut().expect("one line").push(i);
            }
        }
        let mut slots = vec![Rect::NOTHING; widths.len()];
        for (row, line) in lines.iter().enumerate() {
            let line_w: f32 =
                line.iter().map(|&i| widths[i]).sum::<f32>() + gap * (line.len() as f32 - 1.0);
            let mut x = cx - line_w / 2.0;
            let top = y + row as f32 * (button + gap);
            for &i in line {
                slots[i] = Rect::from_min_size(pos2(x, top), vec2(widths[i], button));
                x += widths[i] + gap;
            }
        }

        let mut remove = None;
        let mut pick = None;
        for (i, &id) in self.set.iter().enumerate() {
            let r = Rect::from_center_size(slots[i].center(), Vec2::splat(tile));
            let response = ui.interact(r, ui.id().with(("set", id)), Sense::click());
            let active = id == self.current;
            let fill = if response.hovered() { CANVAS } else { BG };
            let border = if active { INK } else { HAIRLINE };
            ui.painter()
                .rect(r, 9.0, fill, Stroke::new(1.0, border), StrokeKind::Inside);
            paint_icon(
                ui.painter(),
                r.shrink(13.0),
                &gallery::entry(id).icon,
                self.stroke(),
                INK,
            );
            if response.clicked() {
                pick = Some(id);
            }
            // Remove badge on hover.
            if ui.rect_contains_pointer(r.expand(8.0)) && self.set.len() > 1 {
                let badge =
                    Rect::from_center_size(r.right_top() + vec2(-2.0, 2.0), Vec2::splat(18.0));
                let b = ui.interact(badge, ui.id().with(("remove", id)), Sense::click());
                ui.painter().circle_filled(
                    badge.center(),
                    9.0,
                    if b.hovered() { Color32::WHITE } else { INK },
                );
                ui.painter()
                    .text(badge.center(), Align2::CENTER_CENTER, "×", font(13.0), BG);
                if b.clicked() {
                    remove = Some(i);
                    pick = None;
                }
            }
        }
        if let Some(i) = remove {
            self.set.remove(i);
        }
        if let Some(id) = pick {
            self.pick(id);
        }

        let unit = slots[self.set.len() + remove.map_or(0, |_| 1)];
        let line_x = unit.left() + divider / 2.0;
        ui.painter().vline(
            line_x,
            (unit.center().y - 11.0)..=(unit.center().y + 11.0),
            Stroke::new(1.0, HAIRLINE),
        );
        let button_rect =
            Rect::from_min_size(pos2(unit.right() - button, unit.top()), Vec2::splat(button));
        let lucide = |n| {
            gallery::find(Lib::Lucide, n)
                .expect("gallery icon")
                .icon
                .clone()
        };
        let (label, icon) = if self.playing {
            ("Pause the sequence", lucide("pause"))
        } else {
            ("Play the sequence", lucide("play"))
        };
        let spring = self.spring();
        let stroke = self.stroke();
        if toggle_tile(ui, button_rect, button / 2.0, icon, spring, stroke)
            .on_hover_ui(|ui| {
                ui.label(RichText::new(label).font(mono(13.0)).color(INK));
            })
            .clicked()
        {
            self.playing = !self.playing;
            self.scrub = None;
            self.next_at = self.now + STEP;
        }
    }

    fn library(&mut self, ui: &mut Ui, rect: Rect) {
        let inner = rect.shrink2(vec2(26.0, 26.0));

        // Tabs + search.
        let labels: Vec<&str> = TABS.iter().map(|t| t.0).collect();
        let tabs = segmented(
            ui,
            inner.left_top(),
            40.0,
            &labels,
            self.tab,
            font(16.0),
            "tabs",
        );
        if let Some(i) = tabs.clicked {
            self.tab = i;
        }
        let search = Rect::from_min_max(
            pos2(tabs.rect.right() + 14.0, inner.top()),
            pos2(inner.right(), inner.top() + 40.0),
        );
        ui.painter().rect(
            search,
            8.0,
            BG,
            Stroke::new(1.0, HAIRLINE),
            StrokeKind::Inside,
        );
        let magnifier = Rect::from_center_size(
            pos2(search.left() + 22.0, search.center().y),
            Vec2::splat(17.0),
        );
        let lucide_search = &gallery::find(Lib::Lucide, "search")
            .expect("gallery icon")
            .icon;
        paint_icon(ui.painter(), magnifier, lucide_search, 2.0, MUTE);
        let text_rect = Rect::from_min_max(
            pos2(magnifier.right() + 10.0, search.top()),
            pos2(search.right() - 10.0, search.bottom()),
        );
        ui.put(
            text_rect,
            egui::TextEdit::singleline(&mut self.query)
                .hint_text(RichText::new("Search icons").color(MUTE))
                .font(font(16.0))
                .text_color(INK)
                .frame(egui::Frame::NONE)
                .vertical_align(egui::Align::Center),
        );

        // Grid.
        let footer_h = 58.0;
        let grid = Rect::from_min_max(
            pos2(inner.left(), inner.top() + 58.0),
            pos2(inner.right(), inner.bottom() - footer_h),
        );
        let entries = gallery::search(TABS[self.tab].1, &self.query);
        let cols = ((grid.width() - 10.0) / 50.0).floor().max(1.0) as usize;
        let cell = ((grid.width() - 10.0) / cols as f32).floor();
        let mut clicked = None;
        let set = &self.set;
        let current = self.current;
        let stroke = self.stroke();
        ui.scope_builder(UiBuilder::new().max_rect(grid), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("library")
                .max_height(grid.height())
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    for row in entries.chunks(cols) {
                        ui.horizontal(|ui| {
                            for e in row {
                                let (r, response) =
                                    ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                                if !ui.is_rect_visible(r) {
                                    continue;
                                }
                                let tile = r.shrink(3.0);
                                let in_set = set.contains(&e.id);
                                let fill = if response.hovered() {
                                    CANVAS
                                } else if in_set {
                                    TILE
                                } else {
                                    BG
                                };
                                let border = if e.id == current {
                                    INK
                                } else if in_set {
                                    HAIRLINE
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter().rect(
                                    tile,
                                    10.0,
                                    fill,
                                    Stroke::new(1.0, border),
                                    StrokeKind::Inside,
                                );
                                paint_icon(
                                    ui.painter(),
                                    tile.shrink(tile.width() * 0.27),
                                    &e.icon,
                                    stroke,
                                    INK,
                                );
                                let response = response.on_hover_ui(|ui| {
                                    ui.label(RichText::new(e.key()).font(mono(13.0)).color(INK));
                                });
                                if response.clicked() {
                                    clicked = Some(e.id);
                                }
                            }
                        });
                    }
                    if entries.is_empty() {
                        ui.add_space(20.0);
                        ui.label(RichText::new("no icons match").font(mono(13.0)).color(MUTE));
                    }
                });
        });
        if let Some(id) = clicked {
            if !self.set.contains(&id) {
                if self.set.len() >= self.set_capacity {
                    self.set.remove(0); // full: the oldest icon makes room
                }
                self.set.push(id);
            }
            self.pick(id);
        }

        let footer = Rect::from_min_max(pos2(inner.left(), grid.bottom() + 10.0), inner.max);
        ui.scope_builder(UiBuilder::new().max_rect(footer).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
          ui.add(
            egui::Label::new(
                RichText::new(
                    "Click an icon to add it to your set. Icons from different libraries morph into each other: \
                     they all share the 24×24 grid.",
                )
                .font(font(15.0))
                .color(BODY),
            )
            .wrap(),
          );
        });
    }

    fn bar(&mut self, ui: &mut Ui, rect: Rect) {
        let y = rect.center().y;
        let mut x = rect.left() + 34.0;
        let label = ui
            .painter()
            .text(pos2(x, y), Align2::LEFT_CENTER, "spring", mono(13.0), MUTE);
        x = label.right() + 14.0;
        let names: Vec<&str> = SPRINGS.iter().map(|s| s.0).collect();
        let springs = segmented(
            ui,
            pos2(x, y - 19.0),
            38.0,
            &names,
            self.spring,
            font(16.0),
            "springs",
        );
        if let Some(i) = springs.clicked {
            self.spring = i;
        }
        x = springs.rect.right() + 44.0;
        let label = ui
            .painter()
            .text(pos2(x, y), Align2::LEFT_CENTER, "stroke", mono(13.0), MUTE);
        x = label.right() + 14.0;
        let names: Vec<&str> = STROKES.iter().map(|s| s.0).collect();
        let strokes = segmented(
            ui,
            pos2(x, y - 19.0),
            38.0,
            &names,
            self.stroke,
            font(16.0),
            "strokes",
        );
        if let Some(i) = strokes.clicked {
            self.stroke = i;
        }
    }
}

/// A bordered tile holding an uncontrolled, clickable morph icon.
fn toggle_tile(
    ui: &mut Ui,
    rect: Rect,
    radius: f32,
    icon: Icon,
    spring: SpringConfig,
    stroke: f32,
) -> Response {
    let hovered = ui.rect_contains_pointer(rect);
    ui.painter().rect(
        rect,
        radius,
        if hovered { CANVAS } else { BG },
        Stroke::new(1.0, HAIRLINE),
        StrokeKind::Inside,
    );
    let size = rect.width() * 0.46;
    let widget = MorphIcon::new(icon)
        .size(size)
        .spring(spring)
        .stroke_width(stroke)
        .color(INK)
        .sense(Sense::click());
    ui.put(
        Rect::from_center_size(rect.center(), Vec2::splat(size)),
        widget,
    );
    ui.interact(
        rect,
        ui.id()
            .with(("toggle", rect.min.x as i32, rect.min.y as i32)),
        Sense::click(),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

struct Segmented {
    rect: Rect,
    clicked: Option<usize>,
}

/// A pill group: the selected option is inverted (ink background).
fn segmented(
    ui: &mut Ui,
    origin: Pos2,
    height: f32,
    labels: &[&str],
    selected: usize,
    font_id: FontId,
    salt: &str,
) -> Segmented {
    let pad = 16.0;
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(l.to_string(), font_id.clone(), INK)
                .size()
                .x
                + pad * 2.0
        })
        .collect();
    let rect = Rect::from_min_size(origin, vec2(widths.iter().sum::<f32>() + 4.0, height));
    ui.painter().rect(
        rect,
        8.0,
        BG,
        Stroke::new(1.0, HAIRLINE),
        StrokeKind::Inside,
    );
    let mut x = rect.left() + 2.0;
    let mut clicked = None;
    for (i, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(*w, height - 4.0));
        let response = ui.interact(r, ui.id().with((salt, i)), Sense::click());
        let active = i == selected;
        if active {
            ui.painter().rect_filled(r, 6.0, INK);
        } else if response.hovered() {
            ui.painter().rect_filled(r, 6.0, CANVAS);
        }
        let color = if active {
            BG
        } else if response.hovered() {
            INK
        } else {
            BODY
        };
        let f = if active {
            medium(font_id.size)
        } else {
            font_id.clone()
        };
        ui.painter()
            .text(r.center(), Align2::CENTER_CENTER, *label, f, color);
        if response.clicked() {
            clicked = Some(i);
        }
        x += w;
    }
    Segmented { rect, clicked }
}

/// A thin track with a white knob. Returns the new t while dragged/clicked.
fn scrubber(ui: &mut Ui, rect: Rect, t: f64) -> Option<f64> {
    let response = ui.interact(rect, ui.id().with("scrubber"), Sense::click_and_drag());
    let p = ui.painter();
    let y = rect.center().y;
    p.line_segment(
        [pos2(rect.left(), y), pos2(rect.right(), y)],
        Stroke::new(3.0, Color32::from_rgb(34, 34, 34)),
    );
    let x = rect.left() + rect.width() * t as f32;
    p.line_segment(
        [pos2(rect.left(), y), pos2(x, y)],
        Stroke::new(3.0, Color32::from_rgb(58, 58, 58)),
    );
    let knob = if response.hovered() || response.dragged() {
        10.5
    } else {
        9.5
    };
    p.circle(
        pos2(x, y),
        knob,
        INK,
        Stroke::new(1.0, Color32::from_black_alpha(80)),
    );
    if response.dragged() || response.clicked() {
        let px = response.interact_pointer_pos()?.x;
        return Some(((px - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64);
    }
    None
}

/// The stage's color mesh: five soft radial blobs, like the site's
/// `radial-gradient` stack under `blur(36px)` at 60% opacity.
fn glow(painter: &Painter, area: Rect) {
    // (radius x, radius y, center x, center y) as fractions of the area, color, alpha.
    const BLOBS: [(f32, f32, f32, f32, [u8; 3], f32); 5] = [
        (0.30, 0.38, 0.52, 0.50, [249, 203, 40], 0.25),
        (0.36, 0.44, 0.76, 0.74, [255, 0, 128], 0.32),
        (0.40, 0.48, 0.30, 0.78, [121, 40, 202], 0.38),
        (0.35, 0.42, 0.78, 0.24, [0, 223, 216], 0.40),
        (0.38, 0.45, 0.22, 0.28, [0, 124, 240], 0.50),
    ];
    const RINGS: usize = 28;
    const SEGMENTS: usize = 72;
    const OPACITY: f32 = 0.6;
    const BLUR: f32 = 36.0;
    for (rx, ry, cx, cy, [r, g, b], alpha) in BLOBS {
        let center = pos2(
            area.left() + cx * area.width(),
            area.top() + cy * area.height(),
        );
        // Color reaches zero at 70% of the radius; the blur spreads it further.
        let reach_x = 0.7 * rx * area.width() + BLUR * 1.6;
        let reach_y = 0.7 * ry * area.height() + BLUR * 1.6;
        let mut mesh = Mesh::default();
        let color_at = |s: f32| {
            let fall = 1.0 - s * s * (3.0 - 2.0 * s); // smoothstep falloff
            Color32::from_rgba_unmultiplied(r, g, b, (alpha * OPACITY * fall * 255.0) as u8)
        };
        mesh.colored_vertex(center, color_at(0.0));
        for ring in 1..=RINGS {
            let s = ring as f32 / RINGS as f32;
            for seg in 0..SEGMENTS {
                let a = seg as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
                mesh.colored_vertex(
                    center + vec2(a.cos() * reach_x * s, a.sin() * reach_y * s),
                    color_at(s),
                );
            }
        }
        let idx = |ring: usize, seg: usize| (1 + (ring - 1) * SEGMENTS + seg % SEGMENTS) as u32;
        for seg in 0..SEGMENTS {
            mesh.add_triangle(0, idx(1, seg), idx(1, seg + 1));
        }
        for ring in 1..RINGS {
            for seg in 0..SEGMENTS {
                let (a, b) = (idx(ring, seg), idx(ring, seg + 1));
                let (c, d) = (idx(ring + 1, seg), idx(ring + 1, seg + 1));
                mesh.add_triangle(a, c, b);
                mesh.add_triangle(b, c, d);
            }
        }
        painter.add(mesh);
    }
}
