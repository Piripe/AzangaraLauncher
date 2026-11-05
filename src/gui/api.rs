//! Blocking client for the AzangaraMods REST API (call it from worker threads).
//!
//! Mirrors the website's `fetchApi`: the session token goes as-is in the
//! `Authorization` header, IDs are strings and errors come back as
//! `{ error, errorCode }`.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use ureq::Agent;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub has_profile_picture: bool,
    #[serde(default)]
    pub verified_modder: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub last_edit: String,
    #[serde(default)]
    pub difficulty: i32,
    #[serde(default)]
    pub room_amount: i32,
    #[serde(default)]
    pub published: bool,
    pub author: Option<User>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub downloads: i64,
    pub thumbnail_id: Option<String>,
    pub level_files: Option<Vec<LevelFile>>,
    pub gallery_files: Option<Vec<GalleryFile>>,
}

impl Level {
    /// The most recently uploaded file, which is what players get.
    pub fn latest_file(&self) -> Option<&LevelFile> {
        self.level_files.as_ref()?.iter().max_by(|a, b| a.upload_date.cmp(&b.upload_date))
    }

    pub fn thumbnail_url(&self, api: &str) -> Option<String> {
        let thumb = self.thumbnail_id.as_ref().or_else(|| self.gallery_files.as_ref()?.first().map(|g| &g.id))?;
        Some(gallery_url(api, &self.id, thumb))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelFile {
    pub id: String,
    pub upload_date: String,
    pub entry_point: Option<String>,
    pub file_name: String,
    #[serde(default)]
    pub file_size: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFile {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<Level>,
    pub total_hits: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Relevance,
    Downloads,
    Newest,
    Updated,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Relevance, Sort::Downloads, Sort::Newest, Sort::Updated];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Relevance => "Relevance",
            Sort::Downloads => "Most downloaded",
            Sort::Newest => "Newest",
            Sort::Updated => "Recently updated",
        }
    }

    fn query(self) -> &'static str {
        match self {
            Sort::Relevance => "Relevance",
            Sort::Downloads => "Downloads",
            Sort::Newest => "Newest",
            Sort::Updated => "Updated",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    pub sort: Sort,
    pub difficulties: Vec<i32>,
    /// Ids from [`LEVEL_SIZES`].
    pub sizes: Vec<&'static str>,
    pub tags: Vec<String>,
    pub offset: usize,
    pub limit: usize,
}

pub const DIFFICULTIES: [(i32, &str); 6] =
    [(10, "Very Easy"), (20, "Easy"), (30, "Normal"), (40, "Hard"), (50, "Very Hard"), (60, "Expert")];

/// Room count buckets, as the website's `levelSizes` (id sent as `?size=`, label, rooms).
pub const LEVEL_SIZES: [(&str, &str, &str); 5] = [
    ("verySmall", "Very Small", "1–7"),
    ("small", "Small", "8–15"),
    ("medium", "Medium", "16–23"),
    ("large", "Large", "24–31"),
    ("veryLarge", "Very Large", "32+"),
];

pub fn difficulty_name(d: i32) -> &'static str {
    DIFFICULTIES.iter().find(|(v, _)| *v == d).map_or("Unknown", |(_, n)| n)
}

/// Same buckets as the website's `getRoomsText`.
pub fn rooms_text(rooms: i32) -> String {
    let size = match rooms {
        ..8 => "Very Small",
        8..16 => "Small",
        16..24 => "Medium",
        24..32 => "Large",
        _ => "Very Large",
    };
    format!("{rooms} room{} · {size}", if rooms == 1 { "" } else { "s" })
}

pub fn gallery_url(api: &str, level_id: &str, file_id: &str) -> String {
    format!("{}/download/level/{level_id}/gallery/{file_id}", api.trim_end_matches('/'))
}

pub fn avatar_url(api: &str, user_id: &str) -> String {
    format!("{}/download/user/{user_id}/avatar", api.trim_end_matches('/'))
}

/// The website that goes with an API URL (`api.example.com` → `example.com`).
pub fn website_url(api: &str) -> String {
    let api = api.trim_end_matches('/');
    match api.split_once("://") {
        Some((scheme, rest)) => format!("{scheme}://{}", rest.strip_prefix("api.").unwrap_or(rest)),
        None => api.to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
    user: User,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    email: &'a str,
    password: &'a str,
}

/// An API error; `unauthorized` means the token is no longer valid.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub message: String,
    pub unauthorized: bool,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

fn err(message: impl Into<String>) -> ApiError {
    ApiError { message: message.into(), unauthorized: false }
}

#[derive(Clone)]
pub struct Client {
    base: String,
    token: Option<String>,
    agent: Agent,
}

impl Client {
    pub fn new(base: &str, token: Option<String>) -> Self {
        let agent = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .user_agent(concat!("AzangaraLauncher/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { base: base.trim_end_matches('/').to_string(), token, agent }
    }

    fn get(&self, path: &str) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        let req = self.agent.get(format!("{}{path}", self.base));
        match &self.token {
            Some(t) => req.header("Authorization", t),
            None => req,
        }
    }

    fn handle<T: DeserializeOwned>(res: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> ApiResult<T> {
        let mut res = res.map_err(|e| err(format!("The server didn't answer ({e})")))?;
        let status = res.status();
        if status.is_success() {
            return res.body_mut().read_json().map_err(|e| err(format!("Unexpected answer from the server ({e})")));
        }
        let message = res
            .body_mut()
            .read_json::<ErrorResponse>()
            .map(|e| e.error)
            .unwrap_or_else(|_| format!("The server answered {status}"));
        Err(ApiError { message, unauthorized: status.as_u16() == 401 })
    }

    pub fn login(&self, email: &str, password: &str) -> ApiResult<(String, User)> {
        let res = self.agent.post(format!("{}/login", self.base)).send_json(LoginRequest { email, password });
        let login: LoginResponse = Self::handle(res)?;
        Ok((login.token, login.user))
    }

    pub fn me(&self) -> ApiResult<User> {
        Self::handle(self.get("/users/@me").call())
    }

    /// Revokes the token; failures are ignored since the launcher forgets it anyway.
    pub fn logout(&self) {
        let _ = self.get("/logout").call();
    }

    pub fn ping(&self) -> ApiResult<()> {
        Self::handle::<serde_json::Value>(self.get("/settings").call()).map(drop)
    }

    pub fn search(&self, q: &SearchQuery) -> ApiResult<SearchResult> {
        let mut req = self
            .get("/levels")
            .query("sort", q.sort.query())
            .query("offset", q.offset.to_string())
            .query("limit", q.limit.to_string());
        if !q.text.trim().is_empty() {
            req = req.query("q", q.text.trim());
        }
        for d in &q.difficulties {
            req = req.query("difficulty", d.to_string());
        }
        for size in &q.sizes {
            req = req.query("size", *size);
        }
        // The API reads at most 10 tags.
        for tag in q.tags.iter().take(10) {
            req = req.query("tag", tag);
        }
        Self::handle(req.call())
    }

    /// Tags of published levels with their use counts.
    pub fn tags(&self) -> ApiResult<Vec<TagCount>> {
        Self::handle(self.get("/levels/tags").call())
    }

    pub fn level(&self, id: &str) -> ApiResult<Level> {
        Self::handle(self.get(&format!("/levels/{id}")).call())
    }

    /// Streams a level file (the `.pak.zip` the website stores) into `out`,
    /// updating `progress`/`total` in bytes.
    pub fn download(
        &self,
        level_id: &str,
        file_id: &str,
        out: &mut impl Write,
        progress: &AtomicU64,
        total: &AtomicU64,
    ) -> ApiResult<()> {
        let res = self
            .get(&format!("/download/level/{level_id}/files/{file_id}"))
            .query("ext", "zip")
            .call();
        let res = res.map_err(|e| err(format!("The server didn't answer ({e})")))?;
        if !res.status().is_success() {
            return Self::handle::<serde_json::Value>(Ok(res)).map(drop);
        }
        if let Some(len) = res.headers().get("content-length").and_then(|v| v.to_str().ok()?.parse().ok()) {
            total.store(len, Ordering::Relaxed);
        }
        let mut reader = res.into_body().into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf).map_err(|e| err(format!("Download interrupted ({e})")))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| err(format!("Can't write the level file ({e})")))?;
            progress.fetch_add(n as u64, Ordering::Relaxed);
        }
        Ok(())
    }
}

/// "3 days ago", like the website's `formatRelativeDate` (input: RFC 3339 UTC).
pub fn relative_date(date: &str) -> String {
    let Some(then) = parse_timestamp(date) else { return String::new() };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let secs = now - then;
    const UNITS: [(&str, i64); 6] =
        [("year", 365 * 86400), ("month", 30 * 86400), ("week", 7 * 86400), ("day", 86400), ("hour", 3600), ("minute", 60)];
    for (unit, size) in UNITS {
        if secs.abs() >= size {
            let n = (secs as f64 / size as f64).round() as i64;
            return match n {
                1 if unit == "day" => "yesterday".into(),
                1 => format!("last {unit}"),
                n => format!("{n} {unit}s ago"),
            };
        }
    }
    "just now".into()
}

/// Seconds since the epoch of `YYYY-MM-DDTHH:MM:SS...`.
fn parse_timestamp(s: &str) -> Option<i64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Days from civil (Howard Hinnant's algorithm).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((mo + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

pub fn format_count(n: i64) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 1_000_000 {
        trim_decimal(n as f64 / 1000.0, "K")
    } else {
        trim_decimal(n as f64 / 1_000_000.0, "M")
    }
}

fn trim_decimal(v: f64, suffix: &str) -> String {
    let s = format!("{v:.1}");
    format!("{}{suffix}", s.strip_suffix(".0").unwrap_or(&s))
}

pub fn format_bytes(bytes: i64) -> String {
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KiB", b / 1024.0)
    } else {
        format!("{:.1} MiB", b / 1_048_576.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(parse_timestamp("1970-01-02T00:00:00Z"), Some(86400));
        assert_eq!(parse_timestamp("2026-05-25T12:14:03.206656Z"), Some(1_779_711_243));
    }

    #[test]
    fn website_from_api() {
        assert_eq!(website_url("https://api.azangara-mods.piripe.fr/"), "https://azangara-mods.piripe.fr");
    }

    #[test]
    fn counts() {
        assert_eq!(format_count(15), "15");
        assert_eq!(format_count(1500), "1.5K");
        assert_eq!(format_count(2000), "2K");
    }
}
