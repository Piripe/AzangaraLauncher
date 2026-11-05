//! Azangara Launcher: browse and download AzangaraMods levels, and play them
//! with any installed version of the game.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod api;
mod app;
mod icons;
mod library;
mod pages;
mod store;
mod theme;
mod widgets;
mod wine;

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png")).expect("valid icon");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Azangara Launcher")
            .with_app_id("azangara-launcher")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native("Azangara Launcher", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
