//! Settings page: global display settings, the AzangaraMods API, Wine and folders.

use eframe::egui::{self, Align, Layout, Margin, RichText};

use crate::api::{self, Client};
use crate::app::{App, Msg};
use crate::icons::Icon;
use crate::pages::display::display_editor;
use crate::pages::levels::open_path;
use crate::store::{WineSettings, DEFAULT_API_URL};
use crate::theme::*;
use crate::widgets::{self, Button};
use crate::wine;

impl App {
    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        widgets::heading(ui, "Settings");
        ui.add_space(16.0);

        widgets::card(ui, |ui| {
            widgets::title(ui, "Display");
            widgets::muted(ui, "Used by every version that has no display settings of its own (see Versions).");
            ui.add_space(8.0);
            display_editor(ui, &mut self.settings.display, "global");
        });
        ui.add_space(12.0);

        widgets::card(ui, |ui| {
            widgets::title(ui, "AzangaraMods");
            widgets::muted(ui, "Where levels are browsed and downloaded from.");
            ui.add_space(8.0);
            let before = self.settings.api_url.clone();
            let mut edited = false;
            widgets::field_row(ui, "API URL", None, |ui| {
                edited = widgets::text_field(ui, &mut self.settings.api_url, DEFAULT_API_URL, 360.0).lost_focus();
                if self.settings.api_url != DEFAULT_API_URL && Button::text("Reset").ui(ui).clicked() {
                    self.settings.api_url = DEFAULT_API_URL.into();
                    edited = true;
                }
            });
            if self.settings.api_url != before {
                self.ping = None;
                // Sessions belong to one server.
                self.settings.token = None;
                self.user = None;
            }
            if edited && self.settings.api_url != self.browse.api_url {
                self.browse.result = None;
                self.browse.all_tags.clear();
                self.browse.tags.clear();
                self.search();
                self.load_tags();
            }
            ui.horizontal(|ui| {
                ui.add_space(228.0);
                if Button::tonal("Test connection").icon(Icon::Refresh).small().ui(ui).clicked() {
                    self.ping = None;
                    let client = Client::new(&self.settings.api_url, None);
                    self.spawn(move || Msg::Ping(client.ping()));
                }
                match &self.ping {
                    Some(Ok(())) => {
                        Icon::Check.show(ui, 18.0, PRIMARY);
                        ui.label(RichText::new("Connected").color(PRIMARY));
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(ERROR));
                    }
                    None => {}
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(228.0);
                let site = api::website_url(&self.settings.api_url);
                if Button::text(format!("Open {site}")).small().ui(ui).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(site));
                }
            });
        });
        ui.add_space(12.0);

        widgets::card(ui, |ui| {
            widgets::title(ui, "Controls");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Same key bindings in every version").font(font_bold(14.0)));
                    widgets::muted(ui, "Bindings changed in game are copied to the other versions when you play them.");
                    widgets::muted(ui, "Keys with no equivalent between the Direct3D and OpenGL builds are left as they are.");
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut on = self.settings.sync_keys;
                    if widgets::switch(ui, &mut on).changed() {
                        if on {
                            self.enable_key_sync();
                        } else {
                            self.settings.sync_keys = false;
                        }
                    }
                });
            });
            if let (true, Some(keys)) = (self.settings.sync_keys, &self.settings.keybinds) {
                widgets::muted(ui, format!("{} bindings shared.", keys.binds.len()));
            }
        });
        ui.add_space(12.0);

        if wine::NEEDED {
            self.wine_card(ui);
            ui.add_space(12.0);
        }

        widgets::card(ui, |ui| {
            widgets::title(ui, "Folders");
            for (label, path) in [
                ("Launcher", self.paths.root.clone()),
                ("Game versions", self.paths.versions.clone()),
                ("Levels", self.paths.levels.clone()),
            ] {
                widgets::field_row(ui, label, None, |ui| {
                    ui.label(RichText::new(path.display().to_string()).font(font(13.0)).color(ON_SURFACE_VARIANT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::icon_button(ui, Icon::Folder, "Open").clicked() {
                            open_path(path.clone());
                        }
                    });
                });
            }
        });
    }

    fn wine_card(&mut self, ui: &mut egui::Ui) {
        widgets::card(ui, |ui| {
            widgets::title(ui, "Wine");
            widgets::muted(ui, "Azangara is a Windows game; on this system it runs through Wine.");
            ui.add_space(8.0);

            let w = &mut self.settings.wine;
            let builds = self.wine_builds.clone();
            let known = builds.iter().find(|b| b.command == w.wine);
            let selected = known.map_or_else(|| "Custom".to_string(), |b| b.label.clone());
            widgets::field_row(ui, "Wine version", Some("System Wine by default"), |ui| {
                egui::ComboBox::from_id_salt("wine").width(340.0).selected_text(selected).show_ui(ui, |ui| {
                    for b in &builds {
                        ui.selectable_value(&mut w.wine, b.command.clone(), &b.label);
                    }
                    if ui.selectable_label(known.is_none(), "Custom…").clicked() && known.is_some() {
                        w.wine = String::new();
                    }
                });
                if widgets::icon_button(ui, Icon::Refresh, "Look for Wine builds again").clicked() {
                    self.wine_builds = wine::detect();
                }
            });
            if self.settings.wine.wine.is_empty() || !self.wine_builds.iter().any(|b| b.command == self.settings.wine.wine) {
                widgets::field_row(ui, "Wine executable", Some("Path, or a command in PATH"), |ui| {
                    widgets::text_field(ui, &mut self.settings.wine.wine, "/path/to/bin/wine", 340.0);
                });
            }
            let w = &mut self.settings.wine;
            widgets::field_row(ui, "Prefix", Some("Relative to the launcher folder"), |ui| {
                widgets::text_field(ui, &mut w.prefix, "compat", 340.0);
                if w.prefix != WineSettings::default().prefix && Button::text("Reset").ui(ui).clicked() {
                    w.prefix = WineSettings::default().prefix;
                }
            });
            widgets::field_row(ui, "Wine arguments", Some("Put before the game executable"), |ui| {
                widgets::text_field(ui, &mut w.args, "", 340.0);
            });
            widgets::field_row(ui, "Environment", Some("One KEY=VALUE per line"), |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut w.env)
                        .desired_width(340.0)
                        .desired_rows(3)
                        .hint_text(RichText::new("DXVK_HUD=fps").color(ON_SURFACE_VARIANT))
                        .margin(Margin::symmetric(12, 10))
                        .font(egui::FontId::monospace(13.0)),
                );
            });
            ui.horizontal(|ui| {
                ui.add_space(228.0);
                widgets::muted(ui, wine::prefix(&self.settings.wine, &self.paths).display().to_string());
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(228.0);
                if Button::tonal("Set up prefix").icon(Icon::Update).small().ui(ui).on_hover_text("Creates or updates the prefix (wineboot -u)").clicked() {
                    let (settings, paths) = (self.settings.wine.clone(), self.paths.clone());
                    self.notify("Setting up the Wine prefix…", false);
                    self.spawn(move || match wine::init_prefix(&settings, &paths) {
                        Ok(()) => Msg::Notify { text: "Wine prefix ready".into(), error: false },
                        Err(e) => Msg::Notify { text: format!("Can't set up the prefix: {e}"), error: true },
                    });
                }
                if Button::outlined("Wine configuration").icon(Icon::Settings).small().ui(ui).clicked() {
                    let (settings, paths) = (self.settings.wine.clone(), self.paths.clone());
                    self.spawn(move || match wine::winecfg(&settings, &paths) {
                        Ok(()) => Msg::Notify { text: "Wine configuration closed".into(), error: false },
                        Err(e) => Msg::Notify { text: format!("Can't open winecfg: {e}"), error: true },
                    });
                }
                if Button::text("Reset Wine settings").small().ui(ui).clicked() {
                    self.settings.wine = WineSettings::default();
                }
            });
        });
    }
}
