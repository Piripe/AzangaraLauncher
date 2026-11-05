//! Versions page: the game builds in `versions/`, the default one, their own
//! display settings, and starting them normally (config dialog skipped).

use eframe::egui::{self, Align, Layout, RichText, Vec2};

use azangara::{Engine, Install};

use crate::app::App;
use crate::icons::Icon;
use crate::pages::display::display_editor;
use crate::pages::levels::open_path;
use crate::theme::*;
use crate::widgets::{self, menu_item, Button, ChipStyle};

impl App {
    pub fn versions_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            widgets::heading(ui, "Versions");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, Icon::Refresh, "Rescan the versions folder").clicked() {
                    self.refresh_versions();
                }
                if Button::outlined("Open folder").icon(Icon::Folder).ui(ui).clicked() {
                    open_path(self.paths.versions.clone());
                }
            });
        });
        widgets::muted(ui, format!("Each folder in {} is a version of the game.", self.paths.versions.display()));
        ui.add_space(16.0);

        if self.versions.is_empty() {
            widgets::card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    Icon::Gamepad.show(ui, 40.0, OUTLINE);
                    ui.label(RichText::new("No game version found").font(font_bold(16.0)));
                    widgets::muted(ui, "Copy each version of Azangara (the folder with game.exe or azangara.exe) into the versions folder.");
                    ui.add_space(12.0);
                });
            });
            return;
        }
        for version in self.versions.clone() {
            self.version_card(ui, &version);
            ui.add_space(4.0);
        }
    }

    fn version_card(&mut self, ui: &mut egui::Ui, version: &Install) {
        let name = version.name();
        let is_default = self.settings.default_version.as_deref() == Some(&name);
        let custom = self.settings.version_display.contains_key(&name);
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 24.0, SECONDARY_CONTAINER);
                Icon::Gamepad.paint(ui, egui::Rect::from_center_size(rect.center(), Vec2::splat(26.0)), ON_SECONDARY_CONTAINER);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        widgets::title(ui, &name);
                    });
                    ui.horizontal(|ui| {
                        let engine = match version.engine {
                            Engine::Direct3D => "Direct3D 8",
                            Engine::OpenGL => "OpenGL",
                        };
                        widgets::chip(ui, Some(Icon::Settings), engine, ChipStyle::Plain);
                        let exe = version.exe.file_name().unwrap_or_default().to_string_lossy().into_owned();
                        widgets::chip(ui, Some(Icon::Folder), &exe, ChipStyle::Plain);
                        if custom {
                            widgets::chip(ui, Some(Icon::Image), "Custom display", ChipStyle::Highlight);
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let more = widgets::icon_button(ui, Icon::MoreVert, "More");
                    egui::Popup::menu(&more).show(|ui| {
                        ui.set_min_width(220.0);
                        if menu_item(ui, Icon::Settings, "Display settings…", ON_SURFACE).clicked() {
                            self.display_popup = Some(name.clone());
                        }
                        if !is_default && menu_item(ui, Icon::CheckCircle, "Make default", ON_SURFACE).clicked() {
                            self.settings.default_version = Some(name.clone());
                        }
                        if menu_item(ui, Icon::Folder, "Open folder", ON_SURFACE).clicked() {
                            open_path(version.dir.clone());
                        }
                        ui.separator();
                        if menu_item(ui, Icon::Delete, "Delete version…", ERROR).clicked() {
                            self.confirm_delete_version = Some(version.clone());
                        }
                    });
                    let tip = if self.running.is_some() { "The game is already running" } else { "Start the game (the setup screen is skipped)" };
                    if Button::filled("Play").icon(Icon::Play).enabled(self.running.is_none()).ui(ui).on_hover_text(tip).clicked() {
                        self.play(version, None);
                    }
                    if is_default {
                        let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
                        Icon::CheckCircle.paint(ui, egui::Rect::from_center_size(rect.center(), Vec2::splat(24.0)), PRIMARY);
                        response.on_hover_text("Default version: used to play levels");
                    }
                });
            });
        });
    }

    fn delete_version_modal_inner(&mut self, ctx: &egui::Context) -> bool {
        let Some(version) = self.confirm_delete_version.clone() else { return false };
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("delete_version"))
            .frame(egui::Frame::new().fill(SURFACE_CONTAINER_HIGH).corner_radius(egui::CornerRadius::same(28)).inner_margin(egui::Margin::same(24)))
            .backdrop_color(alpha(egui::Color32::BLACK, 0.5))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new(format!("Delete {}?", version.name())).font(font_bold(22.0)));
                let is_link = version.dir.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
                let text = if is_link {
                    "This removes the link from the versions folder; the folder it points to is kept.".to_string()
                } else {
                    format!("The whole folder {} will be deleted, saves and settings included. This can't be undone.", version.dir.display())
                };
                ui.add(egui::Label::new(RichText::new(text).color(ON_SURFACE_VARIANT)).wrap());
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let running = self.running.as_ref().is_some_and(|r| r.version == version.name());
                    if Button::danger("Delete").icon(Icon::Delete).enabled(!running).ui(ui).clicked() {
                        let result = if is_link { std::fs::remove_file(&version.dir) } else { std::fs::remove_dir_all(&version.dir) };
                        match result {
                            Ok(()) => {
                                self.settings.version_display.remove(&version.name());
                                self.notify(format!("Deleted {}", version.name()), false);
                            }
                            Err(e) => self.notify(format!("Can't delete {}: {e}", version.name()), true),
                        }
                        self.refresh_versions();
                        close = true;
                    }
                    if Button::text("Cancel").ui(ui).clicked() {
                        close = true;
                    }
                });
            });
        close || modal.should_close()
    }

    pub fn delete_version_modal(&mut self, ctx: &egui::Context) {
        if self.delete_version_modal_inner(ctx) {
            self.confirm_delete_version = None;
        }
    }

    /// Pop-up with a version's own display settings.
    pub fn version_display_modal(&mut self, ctx: &egui::Context) {
        let Some(name) = self.display_popup.clone() else { return };
        let mut close = false;
        // The id changes with the switch so the pop-up resizes when the editor goes away.
        let custom_now = self.settings.version_display.contains_key(&name);
        let modal = egui::Modal::new(egui::Id::unique(("version_display", &name, custom_now)))
            .frame(egui::Frame::new().fill(SURFACE_CONTAINER_HIGH).corner_radius(egui::CornerRadius::same(28)).inner_margin(egui::Margin::same(24)))
            .backdrop_color(alpha(egui::Color32::BLACK, 0.5))
            .show(ctx, |ui| {
                ui.set_width(620.0);
                ui.label(RichText::new(format!("{name} display")).font(font_bold(22.0)));
                ui.add_space(4.0);
                let mut custom = self.settings.version_display.contains_key(&name);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Custom display settings").font(font_bold(14.0)));
                        widgets::muted(ui, if custom { "Only used by this version." } else { "This version uses the display settings from Settings." });
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::switch(ui, &mut custom).changed() {
                            if custom {
                                self.settings.version_display.insert(name.clone(), self.settings.display.clone());
                            } else {
                                self.settings.version_display.remove(&name);
                            }
                        }
                    });
                });
                if let Some(display) = self.settings.version_display.get_mut(&name) {
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(ctx.content_rect().height() - 260.0).show(ui, |ui| {
                        display_editor(ui, display, &name);
                    });
                }
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Button::filled("Done").ui(ui).clicked() {
                        close = true;
                    }
                });
            });
        if close || modal.should_close() {
            self.display_popup = None;
        }
    }
}
