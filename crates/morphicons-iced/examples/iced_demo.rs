//! `cargo run -p morphicons-iced --example iced_demo`

use iced::time::Instant;
use iced::widget::{button, column, radio, row, slider, text};
use iced::{Color, Element, Subscription, window};
use morphicons_iced::morphicons::{Icon, Morph, SpringConfig, icons};
use morphicons_iced::{MorphIcon, morph_icon, seconds};

fn main() -> iced::Result {
    iced::application(Demo::new, Demo::update, Demo::view)
        .subscription(Demo::subscription)
        .title("morphicons · iced")
        .window_size((560.0, 680.0))
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Preset {
    Smooth,
    Snappy,
    Bouncy,
}

impl Preset {
    fn spring(self) -> SpringConfig {
        match self {
            Preset::Smooth => SpringConfig::SMOOTH,
            Preset::Snappy => SpringConfig::SNAPPY,
            Preset::Bouncy => SpringConfig::BOUNCY,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    ToggleMenu,
    TogglePlay,
    Progress(f64),
    Preset(Preset),
    Next,
    Jump,
    Pick(usize),
    Frame(Instant),
}

struct Demo {
    open: bool,
    playing: bool,
    progress: f64,
    preset: Preset,
    gallery: Vec<(&'static str, Icon)>,
    cursor: usize,
    /// The imperative example: an app-owned morph, ticked from `window::frames()`.
    owned: Morph,
}

impl Demo {
    fn new() -> Self {
        let gallery = icons::all();
        let owned = Morph::new(gallery[0].1.clone());
        Self {
            open: false,
            playing: false,
            progress: 0.35,
            preset: Preset::Snappy,
            gallery,
            cursor: 0,
            owned,
        }
    }

    fn update(&mut self, message: Message) {
        let spring = self.preset.spring();
        match message {
            Message::ToggleMenu => self.open = !self.open,
            Message::TogglePlay => self.playing = !self.playing,
            Message::Progress(p) => self.progress = p,
            Message::Preset(p) => self.preset = p,
            Message::Next => {
                self.cursor = (self.cursor + 1) % self.gallery.len();
                self.owned
                    .morph_to(self.gallery[self.cursor].1.clone(), spring);
            }
            Message::Jump => self.owned.set(icons::check()),
            Message::Pick(i) => self.owned.morph_to(self.gallery[i].1.clone(), spring),
            Message::Frame(now) => {
                self.owned.update(seconds(now));
            }
        }
    }

    /// Only the imperative morph needs frames from the app; the widget-owned
    /// ones animate themselves.
    fn subscription(&self) -> Subscription<Message> {
        if self.owned.is_animating() {
            window::frames().map(Message::Frame)
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let spring = self.preset.spring();

        let menu = if self.open { icons::x() } else { icons::menu() };
        let play = if self.playing {
            icons::pause()
        } else {
            icons::play()
        };
        let uncontrolled = row![
            button(morph_icon(menu).size(48.0).spring(spring)).on_press(Message::ToggleMenu),
            button(morph_icon(play).size(48.0).spring(spring)).on_press(Message::TogglePlay),
        ]
        .spacing(12);

        let presets = row![
            radio("smooth", Preset::Smooth, Some(self.preset), Message::Preset),
            radio("snappy", Preset::Snappy, Some(self.preset), Message::Preset),
            radio("bouncy", Preset::Bouncy, Some(self.preset), Message::Preset),
        ]
        .spacing(16);

        let p = self.progress;
        let controlled = row![
            MorphIcon::between(icons::arrow_right(), icons::arrow_down(), p).size(48.0),
            MorphIcon::between(icons::plus(), icons::x(), p).size(48.0),
            MorphIcon::between(icons::square(), icons::circle(), p).size(48.0),
            slider(0.0..=1.0, p, Message::Progress).step(0.001),
        ]
        .spacing(12);

        let imperative = row![
            MorphIcon::morph(&self.owned).size(96.0).stroke_width(1.5),
            column![
                button("next icon").on_press(Message::Next),
                button("jump to check (no animation)").on_press(Message::Jump),
                text(format!("progress {:.2}", self.owned.progress())),
            ]
            .spacing(8),
        ]
        .spacing(16);

        let gallery = row(self.gallery.iter().enumerate().map(|(i, (_, icon))| {
            button(morph_icon(icon.clone()).size(28.0))
                .on_press(Message::Pick(i))
                .into()
        }))
        .spacing(4)
        .wrap();

        let custom = Icon::from_d("M4 12a8 8 0 0 1 16 0M12 12v8").expect("valid path");
        let tinted = morph_icon(if self.open { custom } else { icons::search() })
            .size(48.0)
            .color(Color::from_rgb8(90, 140, 255));

        column![
            text("morphicons for iced").size(24),
            text("Uncontrolled: click the icons"),
            uncontrolled,
            text("Spring"),
            presets,
            text("Controlled: drag the slider"),
            controlled,
            text("Imperative: an app-owned Morph"),
            imperative,
            text("Any icon to any icon: click one to send the big icon there"),
            gallery,
            text("Custom icon from SVG path data (follows the menu toggle)"),
            Element::from(tinted),
        ]
        .spacing(12)
        .padding(20)
        .into()
    }
}
