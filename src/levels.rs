//! Discovering the levels an installation can start.
//!
//! A level is a `levels/<name>.txt` script, either loose on disk or inside a
//! `.pak`. Its `$header` block carries a `$start_title` shown when it begins.

use std::collections::BTreeMap;
use std::fs;

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
}

/// Lists the levels available to `install`; loose files override pak entries.
pub fn list(install: &Install) -> Vec<Level> {
    let mut levels = BTreeMap::new();
    for pak_path in install.paks() {
        let Ok(pak) = Pak::open(&pak_path) else { continue };
        let source = pak_path.file_name().map(|n| n.to_string_lossy().into_owned());
        for entry in pak.entries().iter().filter(|e| is_level_path(&e.path)) {
            let title = pak.read(entry).ok().and_then(|d| start_title(&d));
            let path = entry.path.to_ascii_lowercase();
            levels.insert(path.clone(), Level { path, title, source: source.clone() });
        }
    }
    if let Ok(rd) = fs::read_dir(install.dir.join("levels")) {
        for p in rd.filter_map(|e| e.ok().map(|e| e.path())) {
            let path = format!("levels/{}", p.file_name().unwrap().to_string_lossy()).to_ascii_lowercase();
            if p.is_file() && is_level_path(&path) {
                let title = fs::read(&p).ok().and_then(|d| start_title(&d));
                levels.insert(path.clone(), Level { path, title, source: None });
            }
        }
    }
    levels.into_values().collect()
}

/// Resolves user input (`level_1`, `level_1.txt` or `levels/level_1.txt`) to an
/// engine path. Unknown names are accepted as given so custom files still work.
pub fn resolve(install: &Install, wanted: &str) -> String {
    let wanted = wanted.replace('\\', "/");
    let lower = wanted.to_ascii_lowercase();
    list(install)
        .into_iter()
        .find(|l| l.path == lower || l.short_name() == lower || l.path == format!("levels/{lower}"))
        .map(|l| l.path)
        .unwrap_or(wanted)
}

fn is_level_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.strip_prefix("levels/")
        .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".txt"))
}

/// First uncommented `$start_title "..."` in a level script.
fn start_title(script: &[u8]) -> Option<String> {
    let text = latin1(script);
    let mut in_block_comment = false;
    for line in text.lines() {
        let line = line.trim();
        if in_block_comment {
            in_block_comment = !line.contains("*/");
            continue;
        }
        if line.starts_with("/*") {
            in_block_comment = !line.contains("*/");
            continue;
        }
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
    fn level_paths() {
        assert!(is_level_path("levels/level_1.txt"));
        assert!(!is_level_path("levels/ctrls_0/003_ctrl.txt"));
        assert!(!is_level_path("scripts/x.txt"));
    }
}
