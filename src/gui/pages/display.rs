//! Editor for the game's display settings.

use eframe::egui::{self, RichText};

use crate::store::Display;
use crate::theme::*;
use crate::widgets::{self, Button};

const RESOLUTIONS: [(u32, u32); 10] = [
    (640, 480),
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 1024),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];
const REFRESH_RATES: [u32; 7] = [0, 60, 75, 120, 144, 165, 240];

fn refresh_label(hz: u32) -> String {
    if hz == 0 { "Default".into() } else { format!("{hz} Hz") }
}

pub fn display_editor(ui: &mut egui::Ui, d: &mut Display, id: &str) {
    ui.push_id(id, |ui| {
        widgets::field_row(ui, "Resolution", None, |ui| {
            egui::ComboBox::from_id_salt("resolution")
                .width(150.0)
                .selected_text(format!("{} × {}", d.width, d.height))
                .show_ui(ui, |ui| {
                    for (w, h) in RESOLUTIONS {
                        if ui.selectable_label(d.width == w && d.height == h, format!("{w} × {h}")).clicked() {
                            d.width = w;
                            d.height = h;
                        }
                    }
                });
            ui.add(egui::DragValue::new(&mut d.width).range(320..=7680).suffix(" px"));
            ui.label(RichText::new("×").color(ON_SURFACE_VARIANT));
            ui.add(egui::DragValue::new(&mut d.height).range(200..=4320).suffix(" px"));
        });
        widgets::field_row(ui, "Mode", None, |ui| {
            widgets::segmented(ui, &mut d.fullscreen, &[(false, "Windowed"), (true, "Fullscreen")]);
        });
        widgets::field_row(ui, "Refresh rate", Some("Fullscreen only"), |ui| {
            egui::ComboBox::from_id_salt("refresh").width(150.0).selected_text(refresh_label(d.refresh)).show_ui(ui, |ui| {
                for hz in REFRESH_RATES {
                    ui.selectable_value(&mut d.refresh, hz, refresh_label(hz));
                }
            });
        });
        widgets::field_row(ui, "Vertical sync", None, |ui| {
            widgets::switch(ui, &mut d.vsync);
        });
        widgets::field_row(ui, "Frame rate limit", Some("0 = unlimited"), |ui| {
            ui.add(egui::DragValue::new(&mut d.max_fps).range(0..=1000).suffix(" fps"));
        });
        widgets::field_row(ui, "Show FPS", None, |ui| {
            widgets::switch(ui, &mut d.show_fps);
        });
        widgets::field_row(ui, "Colour depth", None, |ui| {
            widgets::segmented(ui, &mut d.color_bits, &[(16, "16-bit"), (32, "32-bit")]);
        });
        widgets::field_row(ui, "Texture depth", None, |ui| {
            widgets::segmented(ui, &mut d.texture_bits, &[(16, "16-bit"), (32, "32-bit")]);
        });
        widgets::field_row(ui, "Texture filtering", None, |ui| {
            widgets::switch(ui, &mut d.texture_filter);
        });
        widgets::field_row(ui, "Brightness", None, |ui| {
            ui.add(egui::Slider::new(&mut d.brightness, 0.0..=1.0).fixed_decimals(2));
        });
        widgets::field_row(ui, "Field of view", None, |ui| {
            ui.add(egui::Slider::new(&mut d.fov, 30.0..=120.0).fixed_decimals(0).suffix("°"));
        });
        ui.add_space(4.0);
        if Button::text("Reset to defaults").ui(ui).clicked() {
            *d = Display::default();
        }
    });
}
