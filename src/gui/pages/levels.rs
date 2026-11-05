//! Levels page: the AzangaraMods browser (search, filters, download) and the
//! levels installed in `levels/`, both playable with any game version.

use eframe::egui::{self, Align, CornerRadius, Layout, Margin, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2};

use crate::api::{self, Level};
use crate::app::{App, LevelsTab, PAGE_SIZE};
use crate::icons::Icon;
use crate::library::InstalledLevel;
use crate::theme::*;
use crate::widgets::{self, Button, ChipStyle};

const THUMB: Vec2 = Vec2::new(176.0, 99.0);

impl App {
    pub fn levels_page(&mut self, ui: &mut egui::Ui) {
        if self.detail.is_some() {
            self.level_detail(ui);
            return;
        }
        widgets::heading(ui, "Levels");
        ui.add_space(4.0);
        let tabs = [
            "Browse".to_string(),
            if self.installed.is_empty() { "Installed".into() } else { format!("Installed ({})", self.installed.len()) },
        ];
        let current = match self.levels_tab {
            LevelsTab::Browse => 0,
            LevelsTab::Installed => 1,
        };
        if let Some(t) = widgets::tabs(ui, current, &tabs) {
            self.levels_tab = if t == 0 { LevelsTab::Browse } else { LevelsTab::Installed };
        }
        ui.add_space(16.0);
        match self.levels_tab {
            LevelsTab::Browse => self.browse_tab(ui),
            LevelsTab::Installed => self.installed_tab(ui),
        }
    }

    /// Filter sidebar on the left (as the website's `levelBrowser`), results on the right.
    fn browse_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(250.0, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(250.0);
                self.filter_sidebar(ui);
            });
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), Layout::top_down(Align::Min), |ui| {
                self.browse_results(ui);
            });
        });
    }

    fn filter_sidebar(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        widgets::card_frame().inner_margin(Margin::same(20)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let group = |ui: &mut egui::Ui, title: &str| {
                ui.label(RichText::new(title).font(font_bold(16.0)));
            };
            let wrapped = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    ui.spacing_mut().interact_size.y = 28.0;
                    add(ui);
                });
            };

            group(ui, "Difficulty");
            wrapped(ui, &mut |ui| {
                for (value, name) in api::DIFFICULTIES {
                    let selected = self.browse.difficulties.contains(&value);
                    if widgets::filter_chip_small(ui, selected, Icon::Terrain, name, None).clicked() {
                        toggle(&mut self.browse.difficulties, value);
                        changed = true;
                    }
                }
            });
            ui.add_space(12.0);

            group(ui, "Size");
            wrapped(ui, &mut |ui| {
                for (id, label, rooms) in api::LEVEL_SIZES {
                    let selected = self.browse.sizes.contains(&id);
                    let chip = widgets::filter_chip_small(ui, selected, Icon::Rooms, label, None).on_hover_text(format!("{rooms} rooms"));
                    if chip.clicked() {
                        toggle(&mut self.browse.sizes, id);
                        changed = true;
                    }
                }
            });
            ui.add_space(12.0);

            group(ui, "Tags");
            const SHOWN_TAGS: usize = 12;
            let all = &self.browse.all_tags;
            if all.is_empty() {
                widgets::muted(ui, "No tags yet");
            } else {
                // Selected tags stay visible even when they fall outside the shown ones.
                let shown: Vec<api::TagCount> = if self.browse.show_all_tags {
                    all.clone()
                } else {
                    all.iter()
                        .enumerate()
                        .filter(|(i, t)| *i < SHOWN_TAGS || self.browse.tags.contains(&t.tag))
                        .map(|(_, t)| t.clone())
                        .collect()
                };
                let total = all.len();
                wrapped(ui, &mut |ui| {
                    for t in &shown {
                        let selected = self.browse.tags.contains(&t.tag);
                        if widgets::filter_chip_small(ui, selected, Icon::Tag, &t.tag, Some(t.count)).clicked() {
                            toggle(&mut self.browse.tags, t.tag.clone());
                            changed = true;
                        }
                    }
                });
                if total > SHOWN_TAGS {
                    let label = if self.browse.show_all_tags { "Show less".to_string() } else { format!("Show all {total} tags") };
                    if Button::text(label).small().ui(ui).clicked() {
                        self.browse.show_all_tags = !self.browse.show_all_tags;
                    }
                }
            }

            if self.filter_count() > 0 {
                ui.add_space(12.0);
                if Button::outlined("Clear filters").icon(Icon::Close).small().ui(ui).clicked() {
                    self.browse.difficulties.clear();
                    self.browse.sizes.clear();
                    self.browse.tags.clear();
                    changed = true;
                }
            }
        });
        if changed {
            self.browse.page = 0;
            self.search();
        }
    }

    fn browse_results(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            search_field(ui, &mut self.browse.query, ui.available_width() - 260.0);
            egui::ComboBox::from_id_salt("sort")
                .width(190.0)
                .selected_text(self.browse.sort.label())
                .show_ui(ui, |ui| {
                    for s in api::Sort::ALL {
                        if ui.selectable_label(self.browse.sort == s, s.label()).clicked() && self.browse.sort != s {
                            self.browse.sort = s;
                            self.browse.page = 0;
                            self.search();
                        }
                    }
                });
            if widgets::icon_button(ui, Icon::Refresh, "Refresh").clicked() {
                self.search();
            }
        });
        // Search as the user types, once they pause.
        if self.browse.query != self.browse.sent_query {
            let typed = *self.browse.typed_at.get_or_insert_with(std::time::Instant::now);
            if typed.elapsed().as_millis() > 400 {
                self.browse.page = 0;
                self.search();
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }

        ui.add_space(8.0);

        if let Some(e) = self.browse.error.clone() {
            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    Icon::Warning.show(ui, 20.0, ERROR);
                    ui.label(RichText::new(e).color(ERROR));
                });
                if Button::tonal("Retry").icon(Icon::Refresh).ui(ui).clicked() {
                    self.search();
                }
            });
            return;
        }
        let Some(result) = self.browse.result.clone() else {
            ui.add(egui::Spinner::new().size(32.0).color(PRIMARY));
            return;
        };
        ui.horizontal(|ui| {
            widgets::muted(ui, format!("{} level{}", result.total_hits, if result.total_hits == 1 { "" } else { "s" }));
            if self.browse.loading {
                ui.add(egui::Spinner::new().size(14.0).color(PRIMARY));
            }
        });
        if result.hits.is_empty() {
            empty_state(ui, Icon::Search, "No level found", "Try other words or remove some filters.");
            return;
        }
        for level in &result.hits {
            self.level_card(ui, level);
            ui.add_space(4.0);
        }

        let pages = result.total_hits.div_ceil(PAGE_SIZE).max(1);
        if pages > 1 {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let page = self.browse.page;
                if Button::outlined("Previous").enabled(page > 0).ui(ui).clicked() {
                    self.browse.page -= 1;
                    self.search();
                }
                widgets::muted(ui, format!("Page {} of {pages}", page + 1));
                if Button::outlined("Next").enabled(page + 1 < pages).ui(ui).clicked() {
                    self.browse.page += 1;
                    self.search();
                }
            });
        }
    }

    /// `levelSearchCard.tsx`
    fn level_card(&mut self, ui: &mut egui::Ui, level: &Level) {
        let bg = ui.painter().add(Shape::Noop);
        let frame = widgets::card_frame().inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let url = level.thumbnail_url(&self.settings.api_url);
                if thumbnail(ui, url).clicked() {
                    self.open_detail(&level.id);
                }
                let body_width = (ui.available_width() - 210.0).max(160.0);
                let body = ui.allocate_ui_with_layout(Vec2::new(body_width, THUMB.y), Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.horizontal_wrapped(|ui| {
                        let name = ui.add(egui::Label::new(RichText::new(&level.name).font(font_bold(18.0))).sense(Sense::click()));
                        if name.hovered() {
                            ui.painter().hline(name.rect.x_range(), name.rect.bottom(), Stroke::new(1.0, PRIMARY));
                        }
                        if name.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.open_detail(&level.id);
                        }
                        if let Some(a) = &level.author {
                            widgets::muted(ui, format!("by {}", a.username));
                        }
                    });
                    clamped_text(ui, &plain_description(&level.description).split_whitespace().collect::<Vec<_>>().join(" "), 2);
                    level_chips(ui, level.published, level.difficulty, level.room_amount, &level.tags);
                });
                let height = body.response.rect.height().max(THUMB.y).max(112.0);
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), height), Layout::top_down(Align::Max), |ui| {
                    ui.set_min_height(height);
                    ui.horizontal(|ui| {
                        widgets::muted(ui, "downloads");
                        ui.label(RichText::new(api::format_count(level.downloads)).font(font_bold(16.0)));
                        Icon::Download.show(ui, 18.0, PRIMARY);
                    });
                    ui.horizontal(|ui| {
                        widgets::muted(ui, format!("Updated {}", api::relative_date(&level.last_edit)));
                        Icon::History.show(ui, 16.0, ON_SURFACE_VARIANT);
                    });
                    ui.with_layout(Layout::bottom_up(Align::Max), |ui| self.level_actions(ui, level));
                });
            });
        });
        let rect = frame.response.rect;
        if ui.rect_contains_pointer(rect) {
            ui.painter().set(bg, egui::epaint::RectShape::filled(rect, CornerRadius::same(SHAPE_LG), alpha(ON_SURFACE, STATE_HOVER * 0.6)));
        }
    }

    /// Download / update / play buttons for an online level (right to left).
    fn level_actions(&mut self, ui: &mut egui::Ui, level: &Level) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(dl) = self.downloads.get(&level.id) {
                    let frac = dl.fraction();
                    let label = frac.map_or("Downloading…".to_string(), |f| format!("Downloading {:.0}%", f * 100.0));
                    Button::tonal(label).icon(Icon::Download).progress(frac).ui(ui);
                    return;
                }
                match self.installed_level(&level.id).cloned() {
                    Some(installed) => {
                        self.play_button(ui, &installed);
                        if self.has_update(&installed) && Button::tonal("Update").icon(Icon::Update).ui(ui).clicked() {
                            self.download(level);
                        }
                    }
                    None => {
                        if Button::filled("Download").icon(Icon::Download).ui(ui).clicked() {
                            self.download(level);
                        }
                    }
                }
            });
        });
    }

    /// Split "Play" button: the main part uses the default version, the arrow picks one.
    pub fn play_button(&mut self, ui: &mut egui::Ui, level: &InstalledLevel) {
        let enabled = !self.versions.is_empty() && self.running.is_none();
        let split = ui.push_id(level.key(), |ui| widgets::split_button(ui, "Play", Icon::Play, enabled)).inner;
        let tip = match (self.running.is_some(), self.default_version()) {
            (true, _) => "The game is already running".to_string(),
            (_, Some(v)) => format!("Play with {}", v.name()),
            (_, None) => "Put a game version in the versions folder first".to_string(),
        };
        if split.main.on_hover_text(tip).clicked() {
            if let Some(v) = self.default_version().cloned() {
                self.play(&v, Some(level));
            }
        }
        let versions = self.versions.clone();
        let default = self.default_version().map(|v| v.name());
        egui::Popup::menu(&split.arrow).show(|ui| {
            ui.set_min_width(240.0);
            ui.label(RichText::new("Play with").font(font_bold(12.0)).color(ON_SURFACE_VARIANT));
            for v in &versions {
                let is_default = default.as_deref() == Some(&v.name());
                let (icon, color) = if is_default { (Icon::CheckCircle, PRIMARY) } else { (Icon::Gamepad, ON_SURFACE) };
                let item = widgets::menu_item(ui, icon, &v.name(), color);
                let item = if is_default { item.on_hover_text("Default version") } else { item };
                if item.clicked() && self.running.is_none() {
                    self.play(v, Some(level));
                }
            }
            ui.separator();
            if widgets::menu_item(ui, Icon::Delete, "Delete level…", ERROR).clicked() {
                self.confirm_delete = Some(level.clone());
            }
        });
    }

    fn installed_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, Icon::Refresh, "Rescan the levels folder").clicked() {
                    self.refresh_installed();
                }
                if Button::outlined("Open folder").icon(Icon::Folder).ui(ui).clicked() {
                    open_path(self.paths.levels.clone());
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let text = RichText::new(format!("Every .pak in {}", self.paths.levels.display())).color(ON_SURFACE_VARIANT);
                    ui.add(egui::Label::new(text).truncate());
                });
            });
        });
        ui.add_space(8.0);
        if self.installed_loading && self.installed.is_empty() {
            ui.add(egui::Spinner::new().size(32.0).color(PRIMARY));
            return;
        }
        if self.installed.is_empty() {
            empty_state(ui, Icon::Download, "No level installed", "Download levels from the Browse tab, or put .pak files in the levels folder.");
            return;
        }
        for level in self.installed.clone() {
            self.installed_card(ui, &level);
            ui.add_space(4.0);
        }
    }

    fn installed_card(&mut self, ui: &mut egui::Ui, level: &InstalledLevel) {
        widgets::card_frame().inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let url = level.meta.as_ref().and_then(|m| Some(api::gallery_url(&self.settings.api_url, &m.level_id, m.thumbnail_id.as_ref()?)));
                let thumb = thumbnail(ui, url);
                let online_id = level.meta.as_ref().map(|m| m.level_id.clone());
                if let Some(id) = &online_id {
                    if thumb.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.open_detail(id);
                    }
                }
                let body_width = (ui.available_width() - 250.0).max(160.0);
                let body = ui.allocate_ui_with_layout(Vec2::new(body_width, THUMB.y), Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&level.title).font(font_bold(18.0)));
                        match level.meta.as_ref().and_then(|m| m.author.clone()) {
                            Some(a) => widgets::muted(ui, format!("by {a}")),
                            None => widgets::muted(ui, "Local file"),
                        };
                    });
                    ui.horizontal_wrapped(|ui| {
                        widgets::chip(ui, Some(Icon::Folder), &level.pak_name(), ChipStyle::Plain);
                        if let Some(m) = &level.meta {
                            widgets::chip(ui, Some(Icon::Terrain), api::difficulty_name(m.difficulty), ChipStyle::Plain);
                            if m.room_amount > 0 {
                                widgets::chip(ui, Some(Icon::Rooms), &api::rooms_text(m.room_amount), ChipStyle::Plain);
                            }
                            if self.has_update(level) {
                                widgets::chip(ui, Some(Icon::Update), "Update available", ChipStyle::Success);
                            }
                        }
                    });
                    ui.label(RichText::new(&level.entry).font(font(12.0)).color(ON_SURFACE_VARIANT));
                });
                let height = body.response.rect.height().max(THUMB.y).max(112.0);
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), height), Layout::top_down(Align::Max), |ui| {
                    ui.set_min_height(height);
                    if widgets::icon_button(ui, Icon::Delete, "Delete").clicked() {
                        self.confirm_delete = Some(level.clone());
                    }
                    ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            self.play_button(ui, level);
                            if let (true, Some(meta)) = (self.has_update(level), &level.meta) {
                                if let Some(dl) = self.downloads.get(&meta.level_id) {
                                    Button::tonal("Updating…").progress(dl.fraction()).ui(ui);
                                } else if Button::tonal("Update").icon(Icon::Update).ui(ui).clicked() {
                                    let stub = stub_level(meta);
                                    self.download(&stub);
                                }
                            }
                        });
                    });
                });
            });
        });
    }

    fn level_detail(&mut self, ui: &mut egui::Ui) {
        if Button::text("Back").icon(Icon::ArrowBack).ui(ui).clicked() {
            self.detail = None;
            return;
        }
        let detail = self.detail.as_ref().unwrap();
        if let Some(e) = &detail.error {
            widgets::card(ui, |ui| {
                ui.label(RichText::new(e).color(ERROR));
            });
            return;
        }
        let Some(level) = detail.level.clone() else {
            ui.add(egui::Spinner::new().size(32.0).color(PRIMARY));
            return;
        };

        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width() - 320.0);
                widgets::heading(ui, &level.name);
                if let Some(a) = &level.author {
                    ui.horizontal(|ui| {
                        widgets::avatar(ui, a.has_profile_picture.then(|| api::avatar_url(&self.settings.api_url, &a.id)), &a.username, 24.0);
                        widgets::muted(ui, format!("by {}", a.username));
                        if a.verified_modder {
                            widgets::chip(ui, Some(Icon::Check), "Verified modder", ChipStyle::Highlight);
                        }
                    });
                }
                ui.add_space(4.0);
                level_chips(ui, level.published, level.difficulty, level.room_amount, &level.tags);
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.vertical(|ui| self.level_actions(ui, &level));
            });
        });
        ui.add_space(16.0);

        let gallery = level.gallery_files.clone().unwrap_or_default();
        if !gallery.is_empty() {
            egui::ScrollArea::horizontal().id_salt("gallery").show(ui, |ui| {
                ui.horizontal(|ui| {
                    for g in &gallery {
                        let url = api::gallery_url(&self.settings.api_url, &level.id, &g.id);
                        let r = ui.add(egui::Image::from_uri(url.clone()).fit_to_exact_size(Vec2::new(352.0, 198.0)).corner_radius(CornerRadius::same(SHAPE_MD)).sense(Sense::click()));
                        let r = match g.description.as_deref().filter(|d| !d.is_empty()) {
                            Some(d) => r.on_hover_text(d),
                            None => r,
                        };
                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    }
                });
            });
            ui.add_space(16.0);
        }

        ui.horizontal_top(|ui| {
            let side = 300.0;
            let main_width = ui.available_width() - side - 16.0;
            ui.allocate_ui_with_layout(Vec2::new(main_width, 0.0), Layout::top_down(Align::Min), |ui| {
                widgets::card(ui, |ui| {
                    widgets::title(ui, "Description");
                    ui.add(egui::Label::new(RichText::new(plain_description(&level.description)).color(ON_SURFACE_VARIANT)).wrap());
                });
            });
            ui.allocate_ui_with_layout(Vec2::new(side, 0.0), Layout::top_down(Align::Min), |ui| {
                widgets::card(ui, |ui| {
                    widgets::title(ui, "Details");
                    detail_row(ui, Icon::Download, "Downloads", &api::format_count(level.downloads));
                    detail_row(ui, Icon::Terrain, "Difficulty", api::difficulty_name(level.difficulty));
                    if level.room_amount > 0 {
                        detail_row(ui, Icon::Rooms, "Rooms", &level.room_amount.to_string());
                    }
                    detail_row(ui, Icon::History, "Updated", &api::relative_date(&level.last_edit));
                    if let Some(f) = level.latest_file() {
                        ui.separator();
                        widgets::title(ui, "Latest file");
                        detail_row(ui, Icon::Folder, "File", &format!("{}.pak", f.file_name));
                        detail_row(ui, Icon::Image, "Size", &api::format_bytes(f.file_size));
                        detail_row(ui, Icon::History, "Uploaded", &api::relative_date(&f.upload_date));
                    }
                    ui.add_space(4.0);
                    let page = format!("{}/levels/{}", api::website_url(&self.settings.api_url), level.id);
                    if Button::text("Open on the website").ui(ui).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(page));
                    }
                });
            });
        });
    }
}

fn toggle<T: PartialEq>(values: &mut Vec<T>, value: T) {
    match values.iter().position(|v| *v == value) {
        Some(i) => {
            values.remove(i);
        }
        None => values.push(value),
    }
}

fn detail_row(ui: &mut egui::Ui, icon: Icon, label: &str, value: &str) {
    ui.horizontal(|ui| {
        icon.show(ui, 16.0, PRIMARY);
        widgets::muted(ui, label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).font(font_bold(14.0)));
        });
    });
}

fn level_chips(ui: &mut egui::Ui, published: bool, difficulty: i32, rooms: i32, tags: &[String]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        ui.spacing_mut().interact_size.y = 24.0;
        if !published {
            widgets::chip(ui, None, "Draft", ChipStyle::Highlight);
        }
        widgets::chip(ui, Some(Icon::Terrain), api::difficulty_name(difficulty), ChipStyle::Plain);
        if rooms > 0 {
            widgets::chip(ui, Some(Icon::Rooms), &api::rooms_text(rooms), ChipStyle::Plain);
        }
        for t in tags.iter().filter(|t| !t.is_empty()) {
            widgets::chip(ui, Some(Icon::Tag), t, ChipStyle::Plain);
        }
    });
}

/// 16:9 rounded thumbnail, or an image placeholder.
fn thumbnail(ui: &mut egui::Ui, url: Option<String>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(THUMB, Sense::click());
    let radius = CornerRadius::same(SHAPE_MD);
    ui.painter().rect_filled(rect, radius, SURFACE_CONTAINER_HIGHEST);
    match url {
        Some(url) => {
            egui::Image::from_uri(url).corner_radius(radius).maintain_aspect_ratio(false).fit_to_exact_size(THUMB).paint_at(ui, rect);
        }
        None => Icon::Image.paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(32.0)), OUTLINE),
    }
    response
}

fn search_field(ui: &mut egui::Ui, query: &mut String, width: f32) {
    let width = width.max(200.0);
    let frame = egui::Frame::new()
        .fill(alpha(SURFACE_CONTAINER_HIGHEST, 0.6))
        .corner_radius(CornerRadius::same(SHAPE_FULL))
        .inner_margin(Margin::symmetric(16, 4));
    let inner = frame.show(ui, |ui| {
        ui.set_width(width - 32.0);
        ui.horizontal(|ui| {
            Icon::Search.show(ui, 20.0, ON_SURFACE_VARIANT);
            ui.add(
                egui::TextEdit::singleline(query)
                    .frame(egui::Frame::NONE)
                    .hint_text(RichText::new("Search levels").color(ON_SURFACE_VARIANT))
                    .desired_width(ui.available_width() - 32.0)
                    .align(egui::Align2::LEFT_CENTER)
                    .min_size(Vec2::new(0.0, 32.0))
                    .margin(Margin::symmetric(0, 4))
                    .font(font(14.0)),
            );
            if !query.is_empty() && widgets::icon_button(ui, Icon::Close, "Clear").clicked() {
                query.clear();
            }
        });
    });
    let stroke = if inner.response.contains_pointer() { OUTLINE } else { OUTLINE_VARIANT };
    ui.painter().rect_stroke(inner.response.rect, CornerRadius::same(SHAPE_FULL), Stroke::new(1.0, stroke), StrokeKind::Inside);
}

/// Text clamped to `rows` lines with an ellipsis (`-webkit-line-clamp`).
fn clamped_text(ui: &mut egui::Ui, text: &str, rows: usize) {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat { font_id: font(14.0), color: ON_SURFACE_VARIANT, ..Default::default() },
    );
    job.wrap = egui::text::TextWrapping { max_width: ui.available_width(), max_rows: rows, break_anywhere: false, overflow_character: Some('…') };
    ui.label(job);
}

/// The website renders Discord-flavoured markdown; show it as plain text.
fn plain_description(md: &str) -> String {
    let s = md.replace("**", "").replace("__", "").replace('`', "").replace("||", "");
    s.lines().map(|l| l.trim_start_matches("# ").trim_start_matches("## ").trim_start_matches("### ")).collect::<Vec<_>>().join("\n")
}

fn empty_state(ui: &mut egui::Ui, icon: Icon, title: &str, text: &str) {
    widgets::card(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            icon.show(ui, 40.0, OUTLINE);
            ui.label(RichText::new(title).font(font_bold(16.0)));
            widgets::muted(ui, text);
            ui.add_space(12.0);
        });
    });
}

/// A level known only from its sidecar, enough to download its latest file.
fn stub_level(meta: &crate::library::Meta) -> Level {
    Level {
        id: meta.level_id.clone(),
        name: meta.name.clone(),
        description: String::new(),
        last_edit: String::new(),
        difficulty: meta.difficulty,
        room_amount: meta.room_amount,
        published: true,
        author: None,
        tags: Vec::new(),
        downloads: 0,
        thumbnail_id: meta.thumbnail_id.clone(),
        level_files: None,
        gallery_files: None,
    }
}

pub fn open_path(path: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&path);
    std::thread::spawn(move || {
        let _ = open::that(path);
    });
}
