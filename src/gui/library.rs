//! Levels installed in the launcher's `levels/` folder.
//!
//! Every `.pak` there is playable. Levels downloaded from AzangaraMods also have
//! a `<name>.json` sidecar with where they came from, which gives them a proper
//! name, a thumbnail and update checks. Other paks (dropped in by hand) are
//! scanned for level scripts.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use serde::{Deserialize, Serialize};

use super::api::{self, Client};
use azangara::levels;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub level_id: String,
    pub file_id: String,
    pub name: String,
    pub author: Option<String>,
    pub entry_point: String,
    pub upload_date: String,
    pub thumbnail_id: Option<String>,
    pub difficulty: i32,
    pub room_amount: i32,
}

#[derive(Debug, Clone)]
pub struct InstalledLevel {
    pub pak: PathBuf,
    /// Level script inside the pak, as given to the game.
    pub entry: String,
    pub title: String,
    pub meta: Option<Meta>,
}

impl InstalledLevel {
    pub fn key(&self) -> String {
        format!("{}|{}", self.pak.display(), self.entry)
    }

    pub fn pak_name(&self) -> String {
        self.pak.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// Scans `dir` (blocking: it reads every pak's level scripts).
pub fn scan(dir: &Path) -> Vec<InstalledLevel> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut paks: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pak")))
        .collect();
    paks.sort();

    let mut out = Vec::new();
    for pak in paks {
        let found = levels::pak_levels(&pak);
        let meta: Option<Meta> =
            fs::read(sidecar(&pak)).ok().and_then(|b| serde_json::from_slice(&b).ok());
        match meta {
            Some(meta) => {
                let entry = found
                    .iter()
                    .find(|l| l.path.eq_ignore_ascii_case(&meta.entry_point))
                    .or(found.first())
                    .map_or_else(|| meta.entry_point.to_ascii_lowercase(), |l| l.path.clone());
                out.push(InstalledLevel { pak, entry, title: meta.name.clone(), meta: Some(meta) });
            }
            None => {
                let stem = pak.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                for l in found {
                    let title = l.title.clone().map(|t| title_case(&t)).unwrap_or_else(|| stem.clone());
                    out.push(InstalledLevel { pak: pak.clone(), entry: l.path, title, meta: None });
                }
            }
        }
    }
    out
}

/// Downloads the latest file of `level` into `dir`, replacing an older
/// download of the same level. Returns the pak path.
pub fn download(
    client: &Client,
    level: &api::Level,
    file: &api::LevelFile,
    dir: &Path,
    progress: &AtomicU64,
    total: &AtomicU64,
) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = target_path(dir, level);
    let zip_part = target.with_extension("pak.zip.part");
    let part = target.with_extension("pak.part");
    let cleanup = || {
        let _ = fs::remove_file(&zip_part);
        let _ = fs::remove_file(&part);
    };
    let mut out = fs::File::create(&zip_part).map_err(|e| format!("Can't create {}: {e}", zip_part.display()))?;
    let result = client.download(&level.id, &file.id, &mut out, progress, total);
    drop(out);
    if let Err(e) = result {
        cleanup();
        return Err(e.message);
    }
    if let Err(e) = extract_pak(&zip_part, &part) {
        cleanup();
        return Err(e);
    }
    let _ = fs::remove_file(&zip_part);

    // Prefer the entry point the website detected; fall back to what's in the pak.
    let found = levels::pak_levels(&part);
    let entry_point = file
        .entry_point
        .clone()
        .filter(|e| found.iter().any(|l| l.path.eq_ignore_ascii_case(e)))
        .or_else(|| found.first().map(|l| l.path.clone()))
        .ok_or_else(|| {
            cleanup();
            "The downloaded file doesn't contain any level".to_string()
        })?;

    fs::rename(&part, &target).map_err(|e| e.to_string())?;
    let meta = Meta {
        level_id: level.id.clone(),
        file_id: file.id.clone(),
        name: level.name.clone(),
        author: level.author.as_ref().map(|a| a.username.clone()),
        entry_point,
        upload_date: file.upload_date.clone(),
        thumbnail_id: level.thumbnail_id.clone().or_else(|| level.gallery_files.as_ref()?.first().map(|g| g.id.clone())),
        difficulty: level.difficulty,
        room_amount: level.room_amount,
    };
    fs::write(sidecar(&target), serde_json::to_vec_pretty(&meta).unwrap()).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Extracts the level `.pak` from a downloaded `.pak.zip` (the website stores
/// each upload as a zip holding a single `level-data.pak`).
fn extract_pak(zip_path: &Path, out: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("The downloaded file isn't a valid zip ({e})"))?;
    let index = (0..zip.len())
        .find(|&i| zip.name_for_index(i).is_some_and(|n| n.is_ok_and(|n| n.to_ascii_lowercase().ends_with(".pak"))))
        .or((!zip.is_empty()).then_some(0))
        .ok_or("The downloaded zip is empty")?;
    let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
    let mut dest = fs::File::create(out).map_err(|e| format!("Can't create {}: {e}", out.display()))?;
    std::io::copy(&mut entry, &mut dest).map_err(|e| format!("Can't extract the level ({e})"))?;
    Ok(())
}

/// Deletes a pak and its sidecar.
pub fn delete(pak: &Path) -> std::io::Result<()> {
    fs::remove_file(pak)?;
    match fs::remove_file(sidecar(pak)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn sidecar(pak: &Path) -> PathBuf {
    pak.with_extension("json")
}

/// `<name>.pak`, reusing the existing download of the same level, and adding the
/// level id when another pak already has that name.
fn target_path(dir: &Path, level: &api::Level) -> PathBuf {
    let mut name: String = level
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    name = name.trim_matches('_').to_string();
    if name.is_empty() {
        name = level.id.clone();
    }
    for existing in scan_meta(dir) {
        if existing.1.level_id == level.id {
            return existing.0;
        }
    }
    let plain = dir.join(format!("{name}.pak"));
    if !plain.exists() {
        return plain;
    }
    dir.join(format!("{name}_{}.pak", level.id))
}

fn scan_meta(dir: &Path) -> Vec<(PathBuf, Meta)> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    rd.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let meta: Meta = serde_json::from_slice(&fs::read(&p).ok()?).ok()?;
            Some((p.with_extension("pak"), meta))
        })
        .collect()
}

/// "BROWN ROOM" → "Brown Room" (stock level titles are upper case).
fn title_case(s: &str) -> String {
    if s.chars().any(|c| c.is_lowercase()) {
        return s.to_string();
    }
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
