//! Discovering the levels an installation can start.
//!
//! The stock levels are `levels/<name>.txt` scripts, loose on disk or inside
//! the game's `.pak` files. Extra paks (custom levels) may store theirs anywhere,
//! e.g. `assets/lebel/lebel.txt`, so their scripts are recognised by content: a
//! `$header` block followed by `$rooms`. The header's `$start_title` is the
//! name shown when the level begins.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::latin1;
use crate::install::Install;
use crate::pak::Pak;

#[derive(Debug, Clone)]
pub struct Level {
    /// Path as the engine expects it, e.g. `levels/level_1.txt`.
    pub path: String,
    pub title: Option<String>,
    /// Pak file name, or `None` for a loose file.
    pub source: Option<String>,
}

impl Level {
    /// Short name accepted on the command line, e.g. `level_1`.
    pub fn short_name(&self) -> &str {
        let name = self.path.strip_prefix("levels/").unwrap_or(&self.path);
        name.strip_suffix(".txt").unwrap_or(name)
    }

    /// File name without extension, e.g. `lebel` for `assets/lebel/lebel.txt`.
    pub fn stem(&self) -> &str {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        name.strip_suffix(".txt").unwrap_or(name)
    }
}

/// Lists the levels available to `install` plus those in `extra_paks`.
/// Later sources override earlier ones: game paks, loose files, extra paks.
pub fn list(install: &Install, extra_paks: &[PathBuf]) -> Vec<Level> {
    let mut levels = BTreeMap::new();
    for pak_path in install.paks() {
        add_pak_levels(&mut levels, &pak_path, false);
    }
    if let Ok(rd) = fs::read_dir(install.dir.join("levels")) {
        for p in rd.filter_map(|e| e.ok().map(|e| e.path())) {
            let path = format!("levels/{}", p.file_name().unwrap().to_string_lossy()).to_ascii_lowercase();
            if p.is_file() && is_stock_level_path(&path) {
                let title = fs::read(&p).ok().and_then(|d| start_title(&d));
                levels.insert(path.clone(), Level { path, title, source: None });
            }
        }
    }
    for pak_path in extra_paks {
        add_pak_levels(&mut levels, pak_path, true);
    }
    levels.into_values().collect()
}

/// Levels contained in a single pak (any path, detected by content).
pub fn pak_levels(pak_path: &Path) -> Vec<Level> {
    let mut levels = BTreeMap::new();
    add_pak_levels(&mut levels, pak_path, true);
    levels.into_values().collect()
}

fn add_pak_levels(levels: &mut BTreeMap<String, Level>, pak_path: &Path, any_path: bool) {
    let Ok(pak) = Pak::open(pak_path) else { return };
    let source = pak_path.file_name().map(|n| n.to_string_lossy().into_owned());
    for entry in pak.entries() {
        let path = entry.path.to_ascii_lowercase();
        let candidate = if any_path { path.ends_with(".txt") } else { is_stock_level_path(&path) };
        if !candidate {
            continue;
        }
        let Ok(data) = pak.read(entry) else { continue };
        if any_path && !is_level_script(&data) {
            continue;
        }
        let title = start_title(&data);
        levels.insert(path.clone(), Level { path, title, source: source.clone() });
    }
}

/// Resolves user input (`level_1`, `level_1.txt`, `levels/level_1.txt`, or the
/// file stem of a custom level such as `lebel`) to an engine path. Unknown
/// names are accepted as given so custom loose files still work.
pub fn resolve(install: &Install, extra_paks: &[PathBuf], wanted: &str) -> String {
    let wanted = wanted.replace('\\', "/");
    let lower = wanted.to_ascii_lowercase();
    let levels = list(install, extra_paks);
    let exact = levels.iter().find(|l| {
        l.path == lower || l.short_name() == lower || l.path == format!("levels/{lower}")
    });
    if let Some(l) = exact {
        return l.path.clone();
    }
    let by_stem: Vec<_> = levels.iter().filter(|l| l.stem() == lower.trim_end_matches(".txt")).collect();
    match by_stem[..] {
        [one] => one.path.clone(),
        _ => wanted,
    }
}

fn is_stock_level_path(path: &str) -> bool {
    path.strip_prefix("levels/")
        .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".txt"))
}

/// Uncommented lines of a script, trimmed.
fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut in_block_comment = false;
    text.lines().filter_map(move |line| {
        let line = line.trim();
        if in_block_comment {
            in_block_comment = !line.contains("*/");
            return None;
        }
        if line.starts_with("/*") {
            in_block_comment = !line.contains("*/");
            return None;
        }
        (!line.is_empty() && !line.starts_with("//")).then_some(line)
    })
}

/// A level script starts with `$header` and has a `$rooms` block.
fn is_level_script(script: &[u8]) -> bool {
    let text = latin1(script);
    let mut lines = code_lines(&text);
    lines.next().is_some_and(|l| l.starts_with("$header")) && lines.any(|l| l.starts_with("$rooms"))
}

/// First uncommented `$start_title "..."` in a level script's header.
fn start_title(script: &[u8]) -> Option<String> {
    let text = latin1(script);
    for line in code_lines(&text) {
        if let Some(rest) = line.strip_prefix("$start_title") {
            let title = rest.trim().trim_matches('"');
            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
            return (!title.is_empty()).then_some(title);
        }
        if line.starts_with("$rooms") {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_uncommented_title() {
        let script = b"$header\n{\n//\t$start_title \"OLD\"\n/*\n$start_title \"X\"\n*/\n\t$start_title \"GREEN  PALACE\"\n}\n";
        assert_eq!(start_title(script).as_deref(), Some("GREEN PALACE"));
    }

    #[test]
    fn stock_level_paths() {
        assert!(is_stock_level_path("levels/level_1.txt"));
        assert!(!is_stock_level_path("levels/ctrls_0/003_ctrl.txt"));
        assert!(!is_stock_level_path("scripts/x.txt"));
    }

    #[test]
    fn detects_level_scripts_by_content() {
        assert!(is_level_script(b"// x\n$header\n{\n}\n$rooms\n{\n}\n"));
        assert!(!is_level_script(b"$back_wall 1\n$geometry\n{\n}\n"));
        assert!(!is_level_script(b"$header\n{\n}\n"));
    }
}
