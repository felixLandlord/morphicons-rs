//! The morphicons playground, in iced (after morphicons.com):
//! `cargo run -p morphicons-iced --example iced_playground --release`
//!
//! A stage with the big morphing icon and the math readout, a scrubber that
//! freezes the pair at any t, your icon set, and a library of Lucide,
//! Heroicons and Tabler icons that all morph into each other.

use iced::alignment::{Horizontal, Vertical};
use iced::border::{self, Border};
use iced::font::{Family, Weight};
use iced::theme::Palette;
use iced::time::Instant;
use iced::widget::{
    button, center, column, container, hover, image, row, rule, scrollable, slider, space, stack,
    text, text_input, tooltip,
};
use iced::{
    Background, Center, Color, Element, Fill, Font, Length, Shadow, Subscription, Task, Theme,
    window,
};
use morphicons_gallery::{self as gallery, Lib, Readout, fonts};
use morphicons_iced::morphicons::{Icon, Morph, SpringConfig};
use morphicons_iced::{MorphIcon, morph_icon, seconds};

// The morphicons.com palette.
const BG: Color = Color::from_rgb8(10, 10, 10);
const INK: Color = Color::from_rgb8(237, 237, 237);
const BODY: Color = Color::from_rgb8(161, 161, 161);
const MUTE: Color = Color::from_rgb8(125, 125, 125);
const HAIRLINE: Color = Color::from_rgb8(38, 38, 38);
const CANVAS: Color = Color::from_rgb8(26, 26, 26);
const TILE: Color = Color::from_rgb8(19, 19, 19);

const GEIST: Font = Font::with_name("Geist");
const MONO: Font = Font::with_name("Geist Mono");
const GEIST_MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..GEIST
};
const GEIST_BOLD: Font = Font {
    weight: Weight::Semibold,
    family: Family::Name("Geist"),
    ..Font::DEFAULT
};

const SPRINGS: [(&str, SpringConfig); 3] = [
    ("smooth", SpringConfig::SMOOTH),
    ("snappy", SpringConfig::SNAPPY),
    ("bouncy", SpringConfig::BOUNCY),
];
const STROKES: [(&str, f32); 4] = [("1", 1.0), ("1.5", 1.5), ("2", 2.0), ("2.5", 2.5)];
const TABS: [(&str, Option<Lib>); 4] = [
    ("All", None),
    ("Lucide", Some(Lib::Lucide)),
    ("Heroicons", Some(Lib::Heroicons)),
    ("Tabler", Some(Lib::Tabler)),
];
const PER_ROW: usize = 8;
/// Widest a row of the set may get before wrapping.
const SET_ROW_WIDTH: f32 = 400.0;
/// How long each icon of the sequence holds the stage (as on the site).
const STEP: std::time::Duration = std::time::Duration::from_millis(1300);
const GRID_COLS: usize = 12;

fn main() -> iced::Result {
    iced::application(Playground::new, Playground::update, Playground::view)
        .subscription(Playground::subscription)
        .title("morphicons · iced playground")
        .font(fonts::GEIST)
        .font(fonts::GEIST_MONO)
        .default_font(GEIST)
        .theme(|_: &Playground| {
            Theme::custom(
                "morphicons",
                Palette {
                    background: BG,
                    text: INK,
                    primary: INK,
                    success: INK,
                    warning: INK,
                    danger: INK,
                },
            )
        })
        .window_size((1180.0, 840.0))
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    Library(usize),
    Remove(usize),
    StageClick,
    Scrub(f64),
    Tab(usize),
    Query(String),
    Spring(usize),
    Stroke(usize),
    TogglePlay,
    Tick,
    Copy,
    Frame(Instant),
}

struct Playground {
    set: Vec<usize>,
    current: usize,
    prev: Option<usize>,
    /// The stage icon: an app-owned morph (the imperative mode), ticked
    /// from `window::frames()`.
    morph: Morph,
    readout: Option<Readout>,
    scrub: Option<f64>,
    spring: usize,
    stroke: usize,
    tab: usize,
    query: String,
    /// The stage loops through your set while playing.
    playing: bool,
    /// Bumped on every manual pick: restarts the loop's countdown.
    epoch: u64,
    copied_at: Option<Instant>,
    glow: image::Handle,
}

fn lucide(name: &str) -> Icon {
    gallery::find(Lib::Lucide, name)
        .expect("gallery icon")
        .icon
        .clone()
}

impl Playground {
    fn new() -> Self {
        let start = gallery::find(Lib::Lucide, "arrow-right")
            .expect("gallery icon")
            .id;
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
            epoch: 0,
            copied_at: None,
            glow: glow_image(224, 280),
        };
        p.select(
            gallery::find(Lib::Lucide, "arrow-down")
                .expect("gallery icon")
                .id,
        );
        p
    }

    fn spring(&self) -> SpringConfig {
        SPRINGS[self.spring].1
    }

    fn stroke(&self) -> f32 {
        STROKES[self.stroke].1
    }

    fn select(&mut self, id: usize) {
        if id != self.current {
            self.readout = Some(gallery::readout(
                gallery::entry(self.current),
                gallery::entry(id),
            ));
            self.prev = Some(self.current);
            self.current = id;
        }
        self.scrub = None;
        self.morph
            .morph_to(gallery::entry(id).icon.clone(), self.spring());
    }

    fn copied(&self) -> bool {
        self.copied_at
            .is_some_and(|t| t.elapsed().as_secs_f32() < 1.6)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Select(id) => {
                self.select(id);
                self.epoch += 1;
            }
            Message::Library(id) => {
                self.epoch += 1;
                if !self.set.contains(&id) {
                    if self.set.len() >= PER_ROW * 2 {
                        self.set.remove(0); // full: the oldest icon makes room
                    }
                    self.set.push(id);
                }
                self.select(id);
            }
            Message::Remove(i) => {
                if self.set.len() > 1 {
                    self.set.remove(i);
                }
            }
            Message::StageClick => {
                self.advance();
                self.epoch += 1;
            }
            Message::Tick => {
                if self.playing {
                    self.advance();
                }
            }
            Message::TogglePlay => {
                self.playing = !self.playing;
                self.epoch += 1;
            }
            Message::Scrub(t) => {
                if let Some(prev) = self.prev {
                    self.playing = false; // scrubbing takes over the stage
                    if self.scrub.is_none() {
                        self.morph.set(gallery::entry(prev).icon.clone()); // back to the pair's origin, once
                    }
                    self.scrub = Some(t);
                    self.morph
                        .seek(gallery::entry(self.current).icon.clone(), t);
                }
            }
            Message::Tab(i) => self.tab = i,
            Message::Query(q) => self.query = q,
            Message::Spring(i) => self.spring = i,
            Message::Stroke(i) => self.stroke = i,
            Message::Copy => {
                self.copied_at = Some(Instant::now());
                return iced::clipboard::write("cargo add morphicons".into());
            }
            Message::Frame(now) => {
                self.morph.update(seconds(now));
            }
        }
        Task::none()
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

    fn subscription(&self) -> Subscription<Message> {
        let frames = if self.morph.is_animating() || self.copied() {
            window::frames().map(Message::Frame)
        } else {
            Subscription::none()
        };
        let sequence = if self.playing && self.set.len() > 1 {
            Subscription::run_with(self.epoch, metronome)
        } else {
            Subscription::none()
        };
        Subscription::batch([frames, sequence])
    }

    fn view(&self) -> Element<'_, Message> {
        let title = column(
            ["Morph any SVG", "icon into any other."]
                .map(|line| text(line).size(62).font(GEIST_BOLD).color(INK).into()),
        )
        .align_x(Center);
        let subtitle = column(
            [
                "Animate Lucide, Tabler, Heroicons or any stroke icon set. Optimal",
                "rotation solved in closed form, spring physics, zero dependencies.",
            ]
            .map(|line| text(line).size(20).color(BODY).into()),
        )
        .align_x(Center)
        .spacing(4);

        let copy_icon = if self.copied() {
            lucide("check")
        } else {
            lucide("copy")
        };
        let install = container(
            row![
                text("$").font(MONO).size(18).color(MUTE),
                text("cargo add morphicons").font(MONO).size(18).color(INK),
                space::horizontal(),
                button(
                    morph_icon(copy_icon)
                        .size(22.0)
                        .stroke_width(1.8)
                        .color(MUTE)
                )
                .on_press(Message::Copy)
                .padding(4)
                .style(|_, _| button::Style::default()),
            ]
            .spacing(14)
            .align_y(Center),
        )
        .padding([14, 22])
        .width(400)
        .style(|_| boxed(8.0, BG));

        let card = container(column![
            row![
                self.stage(),
                rule::vertical(1).style(hairline_rule),
                self.library()
            ]
            .height(560),
            rule::horizontal(1).style(hairline_rule),
            self.bar(),
        ])
        .max_width(1120)
        .style(|_| boxed(16.0, BG))
        .clip(true);

        let page = column![title, subtitle, install, card]
            .spacing(26)
            .padding(iced::Padding {
                top: 44.0,
                bottom: 56.0,
                left: 32.0,
                right: 32.0,
            })
            .align_x(Center)
            .width(Fill);
        container(scrollable(page).height(Fill))
            .style(|_| container::Style::default().background(BG))
            .into()
    }

    fn stage(&self) -> Element<'_, Message> {
        let stroke = self.stroke();
        let big = button(
            MorphIcon::morph(&self.morph)
                .size(136.0)
                .stroke_width(stroke)
                .color(INK),
        )
        .on_press(Message::StageClick)
        .padding(16)
        .style(|_, _| button::Style::default());

        let (pair, math, verdict, pure) = match &self.readout {
            Some(r) => (r.pair.as_str(), r.math.as_str(), r.verdict.as_str(), r.pure),
            None => ("", "tap an icon to start", "", false),
        };
        let readout = column![
            text(pair).font(MONO).size(13).color(BODY),
            text(math).font(MONO).size(13).color(MUTE),
            text(verdict)
                .font(MONO)
                .size(13)
                .color(if pure { INK } else { MUTE }),
        ]
        .spacing(6)
        .align_x(Center);

        let t = self.scrub.unwrap_or(self.morph.progress()).clamp(0.0, 1.0);
        let scrubber = row![
            slider(0.0..=1.0, t, Message::Scrub)
                .step(0.001)
                .width(300)
                .style(|_, status| {
                    let knob =
                        if matches!(status, slider::Status::Hovered | slider::Status::Dragged) {
                            10.5
                        } else {
                            9.5
                        };
                    slider::Style {
                        rail: slider::Rail {
                            backgrounds: (
                                Color::from_rgb8(58, 58, 58).into(),
                                Color::from_rgb8(34, 34, 34).into(),
                            ),
                            width: 3.0,
                            border: Border::default().rounded(2),
                        },
                        handle: slider::Handle {
                            shape: slider::HandleShape::Circle { radius: knob },
                            background: INK.into(),
                            border_width: 1.0,
                            border_color: Color::from_rgba8(0, 0, 0, 0.3),
                        },
                    }
                }),
            text(format!("t={t:.2}")).font(MONO).size(13).color(MUTE),
        ]
        .spacing(16)
        .align_y(Center);

        // Your set: rows of tiles, each with a remove badge on hover.
        let tiles: Vec<Element<'_, Message>> = self
            .set
            .iter()
            .enumerate()
            .map(|(i, &id)| {
                let active = id == self.current;
                let tile = button(center(
                    morph_icon(gallery::entry(id).icon.clone())
                        .size(16.0)
                        .stroke_width(stroke)
                        .color(INK),
                ))
                .width(42)
                .height(42)
                .padding(0)
                .on_press(Message::Select(id))
                .style(move |_, status| {
                    tile_style(status, if active { INK } else { HAIRLINE }, 9.0)
                });
                let badge = container(
                    button(center(text("×").size(12).color(BG)))
                        .width(16)
                        .height(16)
                        .padding(0)
                        .on_press(Message::Remove(i))
                        .style(|_, _| button::Style {
                            background: Some(INK.into()),
                            border: Border::default().rounded(8),
                            ..button::Style::default()
                        }),
                )
                .width(Fill)
                .align_x(Horizontal::Right)
                .align_y(Vertical::Top);
                hover(tile, badge)
            })
            .collect();
        // Then a divider and the sequence's play/pause button, flowing with
        // the tiles like the site's wrapping row.
        let (label, icon) = if self.playing {
            ("Pause the sequence", lucide("pause"))
        } else {
            ("Play the sequence", lucide("play"))
        };
        let play = button(center(
            morph_icon(icon)
                .size(22.0)
                .spring(self.spring())
                .stroke_width(stroke)
                .color(INK),
        ))
        .width(48)
        .height(48)
        .padding(0)
        .on_press(Message::TogglePlay)
        .style(|_, status| tile_style(status, HAIRLINE, 24.0));
        let tip = container(text(label).font(MONO).size(13).color(INK))
            .padding([4, 8])
            .style(|_| boxed(6.0, CANVAS));
        let mut items: Vec<(Element<'_, Message>, f32)> =
            tiles.into_iter().map(|t| (t, 42.0)).collect();
        // The divider and the button wrap together, as one unit.
        items.push((
            row![
                container(rule::vertical(1).style(hairline_rule))
                    .height(22)
                    .padding([0, 4]),
                tooltip(play, tip, tooltip::Position::Bottom).gap(6),
            ]
            .spacing(9)
            .align_y(Center)
            .into(),
            9.0 + 9.0 + 48.0,
        ));

        let mut set_rows = column![].spacing(9).align_x(Center);
        let mut current = row![].spacing(9).align_y(Center);
        let mut used = 0.0;
        for (item, width) in items {
            if used > 0.0 && used + 9.0 + width > SET_ROW_WIDTH {
                set_rows = set_rows.push(current);
                current = row![].spacing(9).align_y(Center);
                used = 0.0;
            }
            used += if used > 0.0 { 9.0 } else { 0.0 } + width;
            current = current.push(item);
        }
        set_rows = set_rows.push(current);

        let content = column![big, readout, scrubber, set_rows]
            .spacing(22)
            .align_x(Center)
            .padding([24, 16])
            .width(Fill);
        let glow = image(self.glow.clone())
            .width(Fill)
            .height(Fill)
            .content_fit(iced::ContentFit::Fill);
        container(stack![glow, content])
            .width(Length::FillPortion(2))
            .height(Fill)
            .into()
    }

    fn library(&self) -> Element<'_, Message> {
        let tabs = segmented(&TABS.map(|t| t.0), self.tab, 16, Message::Tab);
        let search = container(
            row![
                morph_icon(lucide("search")).size(17.0).color(MUTE),
                text_input("Search icons", &self.query)
                    .on_input(Message::Query)
                    .size(16)
                    .padding(0)
                    .style(|_, _| {
                        text_input::Style {
                            background: Color::TRANSPARENT.into(),
                            border: Border::default(),
                            icon: MUTE,
                            placeholder: MUTE,
                            value: INK,
                            selection: Color::from_rgb8(40, 60, 110),
                        }
                    }),
            ]
            .spacing(10)
            .align_y(Center),
        )
        .padding([10, 14])
        .width(Fill)
        .style(|_| boxed(8.0, BG));

        let stroke = self.stroke();
        let entries = gallery::search(TABS[self.tab].1, &self.query);
        let cell = |e: &'static gallery::Entry| -> Element<'_, Message> {
            let in_set = self.set.contains(&e.id);
            let border = if e.id == self.current {
                INK
            } else if in_set {
                HAIRLINE
            } else {
                Color::TRANSPARENT
            };
            let fill = if in_set { TILE } else { BG };
            let icon = button(center(
                morph_icon(e.icon.clone())
                    .size(22.0)
                    .stroke_width(stroke)
                    .color(INK),
            ))
            .width(46)
            .height(46)
            .padding(0)
            .on_press(Message::Library(e.id))
            .style(move |_, status| {
                let bg = if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    CANVAS
                } else {
                    fill
                };
                button::Style {
                    background: Some(bg.into()),
                    border: Border {
                        color: border,
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    ..button::Style::default()
                }
            });
            let tip = container(text(e.key()).font(MONO).size(13).color(INK))
                .padding([4, 8])
                .style(|_| boxed(6.0, CANVAS));
            tooltip(icon, tip, tooltip::Position::Bottom).gap(4).into()
        };
        let grid = column(
            entries
                .chunks(GRID_COLS)
                .map(|chunk| row(chunk.iter().map(|&e| cell(e))).spacing(4).into()),
        )
        .spacing(4);
        let grid: Element<'_, Message> = if entries.is_empty() {
            text("no icons match")
                .font(MONO)
                .size(13)
                .color(MUTE)
                .into()
        } else {
            grid.into()
        };

        column![
            row![tabs, search].spacing(14).align_y(Center),
            scrollable(grid)
                .direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new().width(4).scroller_width(4)))
                .height(Fill)
                .width(Fill),
            text(
                "Click an icon to add it to your set. Icons from different libraries morph into each other: \
                 they all share the 24×24 grid."
            )
            .size(15)
            .color(BODY),
        ]
        .spacing(18)
        .padding(26)
        .width(Length::FillPortion(3))
        .into()
    }

    fn bar(&self) -> Element<'_, Message> {
        row![
            text("spring").font(MONO).size(13).color(MUTE),
            segmented(&SPRINGS.map(|s| s.0), self.spring, 16, Message::Spring),
            space::horizontal().width(30),
            text("stroke").font(MONO).size(13).color(MUTE),
            segmented(&STROKES.map(|s| s.0), self.stroke, 16, Message::Stroke),
        ]
        .spacing(14)
        .align_y(Center)
        .padding([12, 34])
        .into()
    }
}

/// A pill group: the selected option is inverted (ink background).
fn segmented<'a>(
    labels: &[&'static str],
    selected: usize,
    size: u32,
    on_press: fn(usize) -> Message,
) -> Element<'a, Message> {
    let buttons = labels.iter().enumerate().map(|(i, &label)| {
        let active = i == selected;
        button(
            text(label)
                .size(size)
                .font(if active { GEIST_MEDIUM } else { GEIST }),
        )
        .padding([6, 16])
        .on_press(on_press(i))
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if active {
                    INK
                } else if hovered {
                    CANVAS
                } else {
                    Color::TRANSPARENT
                })),
                text_color: if active {
                    BG
                } else if hovered {
                    INK
                } else {
                    BODY
                },
                border: Border::default().rounded(6),
                ..button::Style::default()
            }
        })
        .into()
    });
    container(row(buttons))
        .padding(2)
        .style(|_| boxed(8.0, BG))
        .into()
}

fn tile_style(status: button::Status, border: Color, radius: f32) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(if hovered { CANVAS } else { BG }.into()),
        border: Border {
            color: border,
            width: 1.0,
            radius: radius.into(),
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

fn boxed(radius: f32, fill: Color) -> container::Style {
    container::Style {
        background: Some(fill.into()),
        border: Border {
            color: HAIRLINE,
            width: 1.0,
            radius: border::radius(radius),
        },
        ..container::Style::default()
    }
}

fn hairline_rule(_: &Theme) -> rule::Style {
    rule::Style {
        color: HAIRLINE,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

/// The stage's color mesh, rasterized once: five soft radial blobs like the
/// site's `radial-gradient` stack under `blur(36px)` at 60% opacity.
fn glow_image(width: u32, height: u32) -> image::Handle {
    // (radius x, radius y, center x, center y) as fractions, color, alpha; bottom layer first.
    const BLOBS: [(f32, f32, f32, f32, [f32; 3], f32); 5] = [
        (0.30, 0.38, 0.52, 0.50, [249.0, 203.0, 40.0], 0.25),
        (0.36, 0.44, 0.76, 0.74, [255.0, 0.0, 128.0], 0.32),
        (0.40, 0.48, 0.30, 0.78, [121.0, 40.0, 202.0], 0.38),
        (0.35, 0.42, 0.78, 0.24, [0.0, 223.0, 216.0], 0.40),
        (0.38, 0.45, 0.22, 0.28, [0.0, 124.0, 240.0], 0.50),
    ];
    const OPACITY: f32 = 0.6;
    const SPREAD: f32 = 0.35; // blur, as a fraction of the area
    // The blobs live in the stage inset by 40px (≈9% × 7% of a 448×560
    // stage); the blur carries color past that box, fading out at the edges.
    const INSET: (f32, f32) = (0.09, 0.07);
    let fade = |t: f32| {
        let t = (t / 0.1).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for py in 0..height {
        for px in 0..width {
            let (x, y) = (
                (px as f32 + 0.5) / width as f32,
                (py as f32 + 0.5) / height as f32,
            );
            let (u, v) = (
                (x - INSET.0) / (1.0 - 2.0 * INSET.0),
                (y - INSET.1) / (1.0 - 2.0 * INSET.1),
            );
            let edge = fade(x) * fade(1.0 - x) * fade(y) * fade(1.0 - y);
            let (mut rgb, mut a) = ([0.0f32; 3], 0.0f32);
            for (rx, ry, cx, cy, color, alpha) in BLOBS {
                let (reach_x, reach_y) = (0.7 * rx + SPREAD * 0.5, 0.7 * ry + SPREAD * 0.5);
                let d = (((u - cx) / reach_x).powi(2) + ((v - cy) / reach_y).powi(2))
                    .sqrt()
                    .min(1.0);
                let s = alpha * OPACITY * (1.0 - d * d * (3.0 - 2.0 * d));
                // Straight-alpha "over".
                let out = s + a * (1.0 - s);
                if out > 0.0 {
                    for k in 0..3 {
                        rgb[k] = (color[k] * s + rgb[k] * a * (1.0 - s)) / out;
                    }
                }
                a = out;
            }
            pixels.extend([
                rgb[0] as u8,
                rgb[1] as u8,
                rgb[2] as u8,
                (a * edge * 255.0) as u8,
            ]);
        }
    }
    image::Handle::from_rgba(width, height, pixels)
}

/// The sequence's metronome: a tick every [`STEP`]. The default executor has
/// no timer, so a plain thread sleeps and hands ticks to the subscription; it
/// exits once the subscription is dropped (pause, or a new `epoch`).
fn metronome(_epoch: &u64) -> impl iced::futures::Stream<Item = Message> + use<> {
    use iced::futures::channel::mpsc;
    use iced::futures::{SinkExt, StreamExt};

    iced::stream::channel(1, async |mut output: mpsc::Sender<Message>| {
        let (mut tick_tx, mut tick_rx) = mpsc::channel::<()>(1);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(STEP);
                if tick_tx.try_send(()).is_err_and(|e| e.is_disconnected()) {
                    break;
                }
            }
        });
        while tick_rx.next().await.is_some() {
            if output.send(Message::Tick).await.is_err() {
                break;
            }
        }
    })
}
