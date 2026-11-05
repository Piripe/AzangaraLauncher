//! Application state, background work and the window layout (navigation rail
//! on the left, the selected page on the right).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Vec2};

use azangara::{launch, Install, LaunchOptions};

use super::api::{self, ApiResult, Client};
use super::icons::Icon;
use super::library::{self, InstalledLevel};
use super::store::{Display, Paths, Settings};
use super::theme::*;
use super::widgets::{self, Button};
use super::wine::{self, WineBuild};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Levels,
    Versions,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelsTab {
    Browse,
    Installed,
}

pub enum Msg {
    Search { generation: u64, result: ApiResult<api::SearchResult> },
    Detail { id: String, result: ApiResult<Box<api::Level>> },
    Login(ApiResult<(String, api::User)>),
    Me(ApiResult<api::User>),
    Installed(Vec<InstalledLevel>),
    /// Latest file id of each downloaded level.
    Updates(BTreeMap<String, String>),
    Downloaded { level_id: String, result: Result<PathBuf, String> },
    GameExited(Result<std::process::ExitStatus, String>),
    Ping(ApiResult<()>),
    Tags(ApiResult<Vec<api::TagCount>>),
    Notify { text: String, error: bool },
}

pub struct Download {
    pub progress: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
}

impl Download {
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| self.progress.load(Ordering::Relaxed) as f32 / total as f32)
    }
}

pub struct Running {
    pub title: String,
    pub version: String,
    pub pid: u32,
    pub started: Instant,
}

struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

#[derive(Default)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    pub error: Option<String>,
    pub busy: bool,
}

pub struct Browse {
    pub query: String,
    pub sort: api::Sort,
    pub difficulties: Vec<i32>,
    pub sizes: Vec<&'static str>,
    pub tags: Vec<String>,
    /// Tags of published levels, most used first.
    pub all_tags: Vec<api::TagCount>,
    pub show_all_tags: bool,
    pub page: usize,
    pub result: Option<api::SearchResult>,
    pub error: Option<String>,
    pub loading: bool,
    pub generation: u64,
    /// Search text last sent, to search as the user types (debounced).
    pub sent_query: String,
    pub typed_at: Option<Instant>,
    /// API the current results come from.
    pub api_url: String,
}

pub const PAGE_SIZE: usize = 20;

pub struct Detail {
    pub id: String,
    pub level: Option<api::Level>,
    pub error: Option<String>,
}

pub struct App {
    pub ctx: egui::Context,
    pub paths: Paths,
    pub settings: Settings,
    saved_settings: Settings,
    pub page: Page,
    pub levels_tab: LevelsTab,
    pub versions: Vec<Install>,
    pub installed: Vec<InstalledLevel>,
    pub installed_loading: bool,
    /// Latest file id per downloaded level id, once checked.
    pub latest_files: BTreeMap<String, String>,
    pub wine_builds: Vec<WineBuild>,
    pub user: Option<api::User>,
    pub login: Option<LoginForm>,
    pub browse: Browse,
    pub detail: Option<Detail>,
    pub downloads: HashMap<String, Download>,
    pub running: Option<Running>,
    pub confirm_delete: Option<InstalledLevel>,
    /// Version whose display settings pop-up is open.
    pub display_popup: Option<String>,
    pub confirm_delete_version: Option<Install>,
    pub ping: Option<Result<(), String>>,
    toasts: Vec<Toast>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        install(&cc.egui_ctx);
        let paths = Paths::locate();
        paths.ensure();
        let saved = Settings::load(&paths);
        let first_run = saved.is_none();
        let settings = saved.unwrap_or_default();
        let (tx, rx) = channel();
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            versions: Vec::new(),
            installed: Vec::new(),
            installed_loading: false,
            latest_files: BTreeMap::new(),
            wine_builds: if wine::NEEDED { wine::detect() } else { Vec::new() },
            user: None,
            login: None,
            browse: Browse {
                query: String::new(),
                sort: api::Sort::Relevance,
                difficulties: Vec::new(),
                sizes: Vec::new(),
                tags: Vec::new(),
                all_tags: Vec::new(),
                show_all_tags: false,
                page: 0,
                result: None,
                error: None,
                loading: false,
                generation: 0,
                sent_query: String::new(),
                typed_at: None,
                api_url: String::new(),
            },
            detail: None,
            downloads: HashMap::new(),
            running: None,
            confirm_delete: None,
            display_popup: None,
            confirm_delete_version: None,
            ping: None,
            toasts: Vec::new(),
            page: Page::Levels,
            levels_tab: LevelsTab::Browse,
            saved_settings: settings.clone(),
            settings,
            paths,
            tx,
            rx,
        };
        app.refresh_versions();
        if first_run {
            // Start from what the default version already uses rather than
            // overwriting it with generic values on the first launch.
            if let Some(cfg) = app.default_version().and_then(|v| v.load_config().ok()) {
                app.settings.display = Display::from_settings(&azangara::DisplaySettings::read(&cfg));
            }
        }
        app.refresh_installed();
        app.search();
        app.load_tags();
        if app.settings.token.is_some() {
            let client = app.client();
            app.spawn(move || Msg::Me(client.me()));
        }
        app
    }

    pub fn client(&self) -> Client {
        Client::new(&self.settings.api_url, self.settings.token.clone())
    }

    /// Runs `job` on a worker thread and delivers its message to [`Self::handle`].
    pub fn spawn(&self, job: impl FnOnce() -> Msg + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
    }

    pub fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast { text: text.into(), error, until: Instant::now() + Duration::from_secs(if error { 8 } else { 4 }) });
    }

    pub fn refresh_versions(&mut self) {
        self.versions = Install::scan(&self.paths.versions)
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i.dir != self.paths.versions)
            .collect();
        let valid = self.settings.default_version.as_ref().is_some_and(|d| self.versions.iter().any(|v| &v.name() == d));
        if !valid {
            self.settings.default_version = self.versions.first().map(Install::name);
        }
    }

    pub fn refresh_installed(&mut self) {
        self.installed_loading = true;
        let dir = self.paths.levels.clone();
        self.spawn(move || Msg::Installed(library::scan(&dir)));
    }

    pub fn search(&mut self) {
        self.browse.generation += 1;
        self.browse.loading = true;
        self.browse.sent_query = self.browse.query.clone();
        self.browse.typed_at = None;
        self.browse.api_url = self.settings.api_url.clone();
        let query = api::SearchQuery {
            text: self.browse.query.clone(),
            sort: self.browse.sort,
            difficulties: self.browse.difficulties.clone(),
            sizes: self.browse.sizes.clone(),
            tags: self.browse.tags.clone(),
            offset: self.browse.page * PAGE_SIZE,
            limit: PAGE_SIZE,
        };
        let generation = self.browse.generation;
        let client = self.client();
        self.spawn(move || Msg::Search { generation, result: client.search(&query) });
    }

    pub fn load_tags(&mut self) {
        let client = self.client();
        self.spawn(move || Msg::Tags(client.tags()));
    }

    pub fn filter_count(&self) -> usize {
        self.browse.difficulties.len() + self.browse.sizes.len() + self.browse.tags.len()
    }

    /// Turns key binding sync on, starting from the default version's bindings.
    pub fn enable_key_sync(&mut self) {
        self.settings.sync_keys = true;
        if let Some(v) = self.default_version() {
            if let Ok(cfg) = v.load_config() {
                self.settings.keybinds = Some(azangara::keys::KeySet::read(&cfg, v.engine).into());
            }
        }
    }

    pub fn open_detail(&mut self, id: &str) {
        self.detail = Some(Detail { id: id.to_string(), level: None, error: None });
        let client = self.client();
        let id = id.to_string();
        self.spawn(move || Msg::Detail { id: id.clone(), result: client.level(&id).map(Box::new) });
    }

    /// The installed copy of an AzangaraMods level.
    pub fn installed_level(&self, level_id: &str) -> Option<&InstalledLevel> {
        self.installed.iter().find(|l| l.meta.as_ref().is_some_and(|m| m.level_id == level_id))
    }

    pub fn has_update(&self, level: &InstalledLevel) -> bool {
        let Some(meta) = &level.meta else { return false };
        self.latest_files.get(&meta.level_id).is_some_and(|f| f != &meta.file_id)
    }

    /// Downloads the latest file of a level (fetching its file list first if needed).
    pub fn download(&mut self, level: &api::Level) {
        if self.downloads.contains_key(&level.id) {
            return;
        }
        let progress = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        self.downloads.insert(level.id.clone(), Download { progress: progress.clone(), total: total.clone() });
        let client = self.client();
        let dir = self.paths.levels.clone();
        let level = level.clone();
        self.spawn(move || {
            let result = (|| {
                let full = if level.level_files.is_some() { level.clone() } else { client.level(&level.id).map_err(|e| e.message)? };
                let file = full.latest_file().ok_or("This level has no file to download")?.clone();
                let mut full = full;
                full.thumbnail_id = full.thumbnail_id.or(level.thumbnail_id.clone());
                library::download(&client, &full, &file, &dir, &progress, &total)
            })();
            Msg::Downloaded { level_id: level.id.clone(), result }
        });
    }

    pub fn check_updates(&mut self) {
        let ids: Vec<String> = self.installed.iter().filter_map(|l| l.meta.as_ref().map(|m| m.level_id.clone())).collect();
        if ids.is_empty() {
            return;
        }
        let client = self.client();
        self.spawn(move || {
            let latest = ids
                .into_iter()
                .filter_map(|id| Some((id.clone(), client.level(&id).ok()?.latest_file()?.id.clone())))
                .collect();
            Msg::Updates(latest)
        });
    }

    pub fn default_version(&self) -> Option<&Install> {
        let name = self.settings.default_version.as_ref()?;
        self.versions.iter().find(|v| &v.name() == name).or(self.versions.first())
    }

    /// Starts `version`, opening `level` after the intro if given.
    pub fn play(&mut self, version: &Install, level: Option<&InstalledLevel>) {
        if self.running.is_some() {
            self.notify("The game is already running", true);
            return;
        }
        if let (true, Some(keys)) = (self.settings.sync_keys, &self.settings.keybinds) {
            let keys = azangara::keys::KeySet::from(keys);
            let synced = version.load_config().map_err(|e| e.to_string()).and_then(|mut cfg| {
                keys.apply(&mut cfg, version.engine)?;
                version.save_config(&cfg).map_err(|e| e.to_string())
            });
            if let Err(e) = synced {
                self.notify(format!("Can't sync the key bindings: {e}"), true);
            }
        }
        let (runner, env) = wine::runner_and_env(&self.settings.wine, &self.paths);
        let opts = LaunchOptions {
            level: level.map(|l| l.entry.clone()),
            display: self.settings.display_for(&version.name()).to_settings(),
            extra_paks: level.map(|l| l.pak.clone()).into_iter().collect(),
            runner,
            env,
            wait: true,
            ..Default::default()
        };
        let started = launch::plan(version, &opts).and_then(|plan| plan.spawn());
        match started {
            Ok(game) => {
                self.running = Some(Running {
                    title: level.map_or_else(|| version.name(), |l| l.title.clone()),
                    version: version.name(),
                    pid: game.id(),
                    started: Instant::now(),
                });
                self.spawn(move || Msg::GameExited(game.wait().map_err(|e| format!("{e:#}"))));
            }
            Err(e) => self.notify(format!("Can't start the game: {e:#}"), true),
        }
    }

    pub fn stop_game(&mut self) {
        let Some(running) = &self.running else { return };
        if let Err(e) = wine::kill(&self.settings.wine, &self.paths, running.pid) {
            self.notify(format!("Can't stop the game: {e}"), true);
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Search { generation, result } => {
                if generation != self.browse.generation {
                    return;
                }
                self.browse.loading = false;
                match result {
                    Ok(r) => {
                        self.browse.result = Some(r);
                        self.browse.error = None;
                    }
                    Err(e) => self.browse.error = Some(e.message),
                }
            }
            Msg::Detail { id, result } => {
                if let Some(d) = self.detail.as_mut().filter(|d| d.id == id) {
                    match result {
                        Ok(level) => {
                            if let Some(f) = level.latest_file() {
                                self.latest_files.insert(level.id.clone(), f.id.clone());
                            }
                            d.level = Some(*level);
                        }
                        Err(e) => d.error = Some(e.message),
                    }
                }
            }
            Msg::Login(result) => match result {
                Ok((token, user)) => {
                    self.settings.token = Some(token);
                    self.notify(format!("Logged in as {}", user.username), false);
                    self.user = Some(user);
                    self.login = None;
                    self.search();
                }
                Err(e) => {
                    if let Some(form) = &mut self.login {
                        form.busy = false;
                        form.error = Some(e.message);
                    }
                }
            },
            Msg::Me(result) => match result {
                Ok(user) => self.user = Some(user),
                Err(e) if e.unauthorized => {
                    self.settings.token = None;
                    self.notify("Your session has expired, please log in again", true);
                }
                Err(_) => {}
            },
            Msg::Installed(levels) => {
                self.installed = levels;
                self.installed_loading = false;
                self.check_updates();
            }
            Msg::Updates(latest) => self.latest_files.extend(latest),
            Msg::Downloaded { level_id, result } => {
                self.downloads.remove(&level_id);
                match result {
                    Ok(path) => {
                        self.notify(format!("Downloaded {}", path.file_name().unwrap_or_default().to_string_lossy()), false);
                        self.refresh_installed();
                    }
                    Err(e) => self.notify(format!("Download failed: {e}"), true),
                }
            }
            Msg::GameExited(result) => {
                let running = self.running.take();
                // Bindings changed in game become the shared ones.
                if let (true, Some(r)) = (self.settings.sync_keys, &running) {
                    if let Some(v) = self.versions.iter().find(|v| v.name() == r.version) {
                        if let Ok(cfg) = v.load_config() {
                            self.settings.keybinds = Some(azangara::keys::KeySet::read(&cfg, v.engine).into());
                        }
                    }
                }
                match result {
                    Ok(status) if !status.success() && running.as_ref().is_some_and(|r| r.started.elapsed() < Duration::from_secs(5)) => {
                        self.notify(format!("The game closed right away ({status})"), true)
                    }
                    Ok(_) => {}
                    Err(e) => self.notify(format!("Error after the game closed: {e}"), true),
                }
            }
            Msg::Ping(result) => self.ping = Some(result.map_err(|e| e.message)),
            Msg::Tags(result) => {
                if let Ok(tags) = result {
                    self.browse.all_tags = tags;
                }
            }
            Msg::Notify { text, error } => self.notify(text, error),
        }
    }

    fn save_settings_if_changed(&mut self) {
        if self.settings != self.saved_settings {
            match self.settings.save(&self.paths) {
                Ok(()) => self.saved_settings = self.settings.clone(),
                Err(e) => {
                    self.saved_settings = self.settings.clone();
                    self.notify(format!("Can't save the settings: {e}"), true);
                }
            }
        }
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.add(egui::Image::from_bytes("bytes://icon.png", include_bytes!("../../assets/icon.png")).fit_to_exact_size(Vec2::splat(32.0)).corner_radius(CornerRadius::same(SHAPE_SM)));
            ui.add_space(4.0);
            ui.label(RichText::new("Azangara").font(font_bold(18.0)));
            ui.label(RichText::new("Mods").font(font_bold(18.0)).color(PRIMARY));
        });
        ui.add_space(20.0);

        let installed = self.installed.len().to_string();
        let items = [
            (Page::Levels, Icon::Map, "Levels", None),
            (Page::Versions, Icon::Gamepad, "Versions", None),
        ];
        for (page, icon, label, badge) in items {
            let badge: Option<&str> = if page == Page::Levels && !self.installed.is_empty() { Some(&installed) } else { badge };
            if widgets::nav_item(ui, icon, label, self.page == page, badge).clicked() {
                self.page = page;
                if page == Page::Levels {
                    self.detail = None;
                }
            }
            ui.add_space(2.0);
        }

    }

    /// Bottom of the navigation rail: the running game and the account.
    fn nav_footer(&mut self, ui: &mut egui::Ui) {
        if let Some(running) = &self.running {
            let (title, version, elapsed) = (running.title.clone(), running.version.clone(), running.started.elapsed());
            let mut stop = false;
            Frame::new()
                .fill(PRIMARY_CONTAINER)
                .corner_radius(CornerRadius::same(SHAPE_MD))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        Icon::Play.show(ui, 18.0, ON_PRIMARY_CONTAINER);
                        ui.label(RichText::new("Playing").font(font_bold(12.0)).color(ON_PRIMARY_CONTAINER));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let secs = elapsed.as_secs();
                            ui.label(RichText::new(format!("{}:{:02}", secs / 60, secs % 60)).font(font(12.0)).color(ON_PRIMARY_CONTAINER));
                        });
                    });
                    ui.label(RichText::new(title).font(font_bold(14.0)).color(ON_PRIMARY_CONTAINER));
                    ui.label(RichText::new(version).font(font(12.0)).color(ON_PRIMARY_CONTAINER));
                    let tip = if wine::NEEDED { "Ends every program of the Wine prefix" } else { "Ends the game" };
                    stop = Button::outlined("Stop").icon(Icon::Stop).small().ui(ui).on_hover_text(tip).clicked();
                });
            if stop {
                self.stop_game();
            }
            ui.ctx().request_repaint_after(Duration::from_secs(1));
            ui.add_space(8.0);
        }
        self.account_box(ui);
        ui.add_space(8.0);
    }

    /// Account on the left, settings button at the bottom right of the rail.
    fn account_box(&mut self, ui: &mut egui::Ui) {
        let user = self.user.clone();
        let mut logout = false;
        ui.horizontal(|ui| {
            match &user {
                Some(user) => {
                    let url = user.has_profile_picture.then(|| api::avatar_url(&self.settings.api_url, &user.id));
                    widgets::avatar(ui, url, &user.username, 36.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new(&user.username).font(font_bold(14.0)));
                        ui.label(RichText::new(if user.verified_modder { "Verified modder" } else { "Logged in" }).font(font(12.0)).color(ON_SURFACE_VARIANT));
                    });
                }
                None => {
                    if Button::tonal("Log in").icon(Icon::Person).ui(ui).clicked() {
                        self.login = Some(LoginForm::default());
                    }
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let on_settings = self.page == Page::Settings;
                let color = if on_settings { PRIMARY } else { ON_SURFACE_VARIANT };
                if on_settings {
                    let rect = ui.available_rect_before_wrap();
                    let center = egui::pos2(rect.right() - 20.0, rect.center().y);
                    ui.painter().circle_filled(center, 20.0, SECONDARY_CONTAINER);
                }
                if widgets::icon_button_colored(ui, Icon::Settings, "Settings", color).clicked() {
                    self.page = Page::Settings;
                }
                if user.is_some() {
                    logout = widgets::icon_button(ui, Icon::Logout, "Log out").clicked();
                }
            });
        });
        if logout {
            let client = self.client();
            self.spawn(move || {
                client.logout();
                Msg::Notify { text: "Logged out".into(), error: false }
            });
            self.settings.token = None;
            self.user = None;
        }
    }

    fn login_modal(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.login else { return };
        let mut submit = false;
        let mut close = false;
        let website = api::website_url(&self.settings.api_url);
        let modal = egui::Modal::new(egui::Id::unique("login"))
            .frame(Frame::new().fill(SURFACE_CONTAINER_HIGH).corner_radius(CornerRadius::same(28)).inner_margin(Margin::same(24)))
            .backdrop_color(alpha(egui::Color32::BLACK, 0.5))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new("Log in").font(font_bold(22.0)));
                widgets::muted(ui, "Use your AzangaraMods account to download drafts and your own levels.");
                ui.add_space(8.0);
                ui.label(RichText::new("Email").font(font(12.0)).color(ON_SURFACE_VARIANT));
                let email = widgets::text_field(ui, &mut form.email, "you@example.com", f32::INFINITY);
                if form.email.is_empty() && form.password.is_empty() && !form.busy {
                    email.request_focus();
                }
                ui.label(RichText::new("Password").font(font(12.0)).color(ON_SURFACE_VARIANT));
                let pw = ui.add(
                    egui::TextEdit::singleline(&mut form.password)
                        .password(true)
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(12, 10))
                        .font(font(14.0)),
                );
                if pw.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                if let Some(e) = &form.error {
                    ui.label(RichText::new(e).color(ERROR));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if Button::text("Create an account").ui(ui).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(website.clone()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ready = !form.busy && !form.email.trim().is_empty() && !form.password.is_empty();
                        if Button::filled(if form.busy { "Logging in…" } else { "Log in" }).enabled(ready).ui(ui).clicked() {
                            submit = true;
                        }
                        if Button::text("Cancel").ui(ui).clicked() {
                            close = true;
                        }
                    });
                });
            });
        if modal.should_close() {
            close = true;
        }
        if submit && !form.busy && !form.email.trim().is_empty() {
            form.busy = true;
            form.error = None;
            let (email, password) = (form.email.trim().to_string(), form.password.clone());
            let client = Client::new(&self.settings.api_url, None);
            self.spawn(move || Msg::Login(client.login(&email, &password)));
        }
        if close {
            self.login = None;
        }
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if let Some(next) = self.toasts.iter().map(|t| t.until).min() {
            ctx.request_repaint_after(next - now);
        }
        let mut dismiss = None;
        for (i, toast) in self.toasts.iter().enumerate() {
            egui::Area::new(egui::Id::unique(("toast", i)))
                .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -24.0 - i as f32 * 64.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    Frame::new()
                        .fill(INVERSE_SURFACE)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(16, 12))
                        .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: alpha(egui::Color32::BLACK, 0.4) })
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.horizontal(|ui| {
                                if toast.error {
                                    Icon::Warning.show(ui, 18.0, egui::Color32::from_rgb(0xba, 0x1a, 0x1a));
                                }
                                ui.add(egui::Label::new(RichText::new(&toast.text).color(INVERSE_ON_SURFACE)).wrap());
                                if widgets::icon_button_colored(ui, Icon::Close, "Dismiss", INVERSE_ON_SURFACE).clicked() {
                                    dismiss = Some(i);
                                }
                            });
                        });
                });
        }
        if let Some(i) = dismiss {
            self.toasts.remove(i);
        }
    }

    fn delete_modal(&mut self, ctx: &egui::Context) {
        let Some(level) = self.confirm_delete.clone() else { return };
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("delete"))
            .frame(Frame::new().fill(SURFACE_CONTAINER_HIGH).corner_radius(CornerRadius::same(28)).inner_margin(Margin::same(24)))
            .backdrop_color(alpha(egui::Color32::BLACK, 0.5))
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new("Delete this level?").font(font_bold(22.0)));
                widgets::muted(ui, format!("{} will be removed from the levels folder.", level.pak_name()));
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Button::danger("Delete").icon(Icon::Delete).ui(ui).clicked() {
                        match library::delete(&level.pak) {
                            Ok(()) => self.refresh_installed(),
                            Err(e) => self.notify(format!("Can't delete the level: {e}"), true),
                        }
                        close = true;
                    }
                    if Button::text("Cancel").ui(ui).clicked() {
                        close = true;
                    }
                });
            });
        if close || modal.should_close() {
            self.confirm_delete = None;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        if !self.downloads.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        paint_background(ui, ui.max_rect());

        egui::Panel::left("nav")
            .exact_size(248.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(alpha(SURFACE, 0.8))
                    .inner_margin(Margin::symmetric(12, 12))
                    .stroke(Stroke::NONE),
            )
            .show(ui, |ui| {
                egui::Panel::bottom("nav_footer")
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(Frame::NONE)
                    .show(ui, |ui| self.nav_footer(ui));
                self.nav(ui);
            });
        let nav_edge = ui.max_rect().left() + 248.0;
        ui.painter().vline(nav_edge, ui.max_rect().y_range(), Stroke::new(1.0, alpha(OUTLINE_VARIANT, 0.5)));

        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(32, 28)).show(ui, |ui| {
                    let width = ui.available_width().min(1100.0);
                    ui.set_max_width(width);
                    match self.page {
                        Page::Levels => self.levels_page(ui),
                        Page::Versions => self.versions_page(ui),
                        Page::Settings => self.settings_page(ui),
                    }
                });
            });
        });

        self.login_modal(&ctx);
        self.delete_modal(&ctx);
        self.version_display_modal(&ctx);
        self.delete_version_modal(&ctx);
        self.toasts(&ctx);
        self.save_settings_if_changed();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::from_rgb(0x16, 0x13, 0x16).to_normalized_gamma_f32()
    }
}
