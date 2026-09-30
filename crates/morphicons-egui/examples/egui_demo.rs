//! `cargo run -p morphicons-egui --example egui_demo`

use eframe::egui::{self, RichText, Sense};
use morphicons_egui::MorphIcon;
use morphicons_egui::morphicons::{Icon, Morph, SpringConfig, icons};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([560.0, 640.0]),
        ..Default::default()
    };

    let gallery = icons::all();
    let mut open = false;
    let mut playing = false;
    let mut progress = 0.35;
    let mut spring = SpringConfig::SNAPPY;
    let mut cursor = 0;
    let mut owned = Morph::new(gallery[0].1.clone());

    eframe::run_ui_native("morphicons · egui", options, move |ui, _frame| {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("morphicons for egui");
            ui.add_space(12.0);

            ui.label(RichText::new("Uncontrolled: click the icons").strong());
            ui.horizontal(|ui| {
                let menu = if open { icons::x() } else { icons::menu() };
                let menu_btn = MorphIcon::new(menu)
                    .size(48.0)
                    .spring(spring)
                    .sense(Sense::click());
                if ui.add(menu_btn).on_hover_text("menu ↔ x").clicked() {
                    open = !open;
                }
                let play = if playing {
                    icons::pause()
                } else {
                    icons::play()
                };
                let play_btn = MorphIcon::new(play)
                    .size(48.0)
                    .spring(spring)
                    .sense(Sense::click());
                if ui.add(play_btn).on_hover_text("play ↔ pause").clicked() {
                    playing = !playing;
                }
            });

            ui.add_space(16.0);
            ui.label(RichText::new("Spring").strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut spring, SpringConfig::SMOOTH, "smooth");
                ui.radio_value(&mut spring, SpringConfig::SNAPPY, "snappy");
                ui.radio_value(&mut spring, SpringConfig::BOUNCY, "bouncy");
            });

            ui.add_space(16.0);
            ui.label(RichText::new("Controlled: drag the slider").strong());
            ui.horizontal(|ui| {
                ui.add(
                    MorphIcon::between(icons::arrow_right(), icons::arrow_down(), progress)
                        .size(48.0),
                );
                ui.add(MorphIcon::between(icons::plus(), icons::x(), progress).size(48.0));
                ui.add(MorphIcon::between(icons::square(), icons::circle(), progress).size(48.0));
                ui.add(egui::Slider::new(&mut progress, 0.0..=1.0).text("progress"));
            });

            ui.add_space(16.0);
            ui.label(RichText::new("Imperative: an app-owned Morph").strong());
            ui.horizontal(|ui| {
                ui.add(MorphIcon::morph(&mut owned).size(96.0).stroke_width(1.5));
                ui.vertical(|ui| {
                    if ui.button("next icon").clicked() {
                        cursor = (cursor + 1) % gallery.len();
                        owned.morph_to(gallery[cursor].1.clone(), spring);
                    }
                    if ui.button("jump to check (no animation)").clicked() {
                        owned.set(icons::check());
                    }
                    ui.label(format!("progress {:.2}", owned.progress()));
                });
            });

            ui.add_space(16.0);
            ui.label(
                RichText::new("Any icon to any icon: click one to send the big icon there")
                    .strong(),
            );
            ui.horizontal_wrapped(|ui| {
                for (name, icon) in &gallery {
                    let tile = MorphIcon::new(icon.clone())
                        .size(32.0)
                        .sense(Sense::click());
                    if ui.add(tile).on_hover_text(*name).clicked() {
                        owned.morph_to(icon.clone(), spring);
                    }
                }
            });

            ui.add_space(16.0);
            ui.label(RichText::new("Custom icon from SVG path data").strong());
            let custom = Icon::from_d("M4 12a8 8 0 0 1 16 0M12 12v8").expect("valid path");
            let icon = if open { custom } else { icons::search() };
            ui.add(
                MorphIcon::new(icon)
                    .size(48.0)
                    .color(egui::Color32::from_rgb(90, 140, 255)),
            );
        });
    })
}
