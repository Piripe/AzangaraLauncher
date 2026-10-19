//! Locating and identifying game installations.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::patch::PATCHED_EXE_NAME;

pub const CONFIG_NAME: &str = "Azangara.cfg";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Original 32-bit Direct3D 8 builds (`game.exe`), with a config dialog.
    Direct3D,
    /// 64-bit OpenGL/SDL2 port (`azangara.exe`), no config dialog.
    OpenGL,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Engine::Direct3D => "Direct3D",
            Engine::OpenGL => "OpenGL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Install {
    pub dir: PathBuf,
    pub exe: PathBuf,
    pub engine: Engine,
}

impl Install {
    /// Identifies the game in `dir` by inspecting its executables.
    pub fn open(dir: &Path) -> io::Result<Self> {
        let dir = dir.canonicalize()?;
        let mut found = None;
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = file_name_lower(&path);
            if !name.ends_with(".exe") || name == PATCHED_EXE_NAME.to_ascii_lowercase() {
                continue;
            }
            let Some(machine) = pe_machine(&path) else { continue };
            let engine = match (name.as_str(), machine) {
                ("game.exe", 0x14c) => Engine::Direct3D,
                ("azangara.exe", 0x8664) => Engine::OpenGL,
                _ => continue,
            };
            found = Some(Install { dir: dir.clone(), exe: path, engine });
            break;
        }
        found.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no Azangara executable in {}", dir.display()),
            )
        })
    }

    /// Finds every installation directly below `root` (and `root` itself).
    pub fn scan(root: &Path) -> io::Result<Vec<Self>> {
        let mut out: Vec<Self> = Install::open(root).into_iter().collect();
        let mut dirs: Vec<PathBuf> = fs::read_dir(root)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        out.extend(dirs.iter().filter_map(|d| Install::open(d).ok()));
        Ok(out)
    }

    pub fn name(&self) -> String {
        self.dir.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    /// Path of the config file, matching an existing file case-insensitively.
    pub fn config_path(&self) -> PathBuf {
        fs::read_dir(&self.dir)
            .ok()
            .and_then(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.path()))
                    .find(|p| file_name_lower(p) == CONFIG_NAME.to_ascii_lowercase())
            })
            .unwrap_or_else(|| self.dir.join(CONFIG_NAME))
    }

    /// Loads the config, or an empty one if the game has never written it.
    pub fn load_config(&self) -> io::Result<Config> {
        match Config::load(&self.config_path()) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            r => r,
        }
    }

    pub fn save_config(&self, cfg: &Config) -> io::Result<()> {
        cfg.save(&self.config_path())
    }

    /// All `.pak` archives of the installation, sorted by name.
    pub fn paks(&self) -> Vec<PathBuf> {
        let mut paks: Vec<PathBuf> = fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| file_name_lower(p).ends_with(".pak"))
                    .collect()
            })
            .unwrap_or_default();
        paks.sort();
        paks
    }
}

fn file_name_lower(path: &Path) -> String {
    path.file_name().map_or_else(String::new, |n| n.to_string_lossy().to_ascii_lowercase())
}

/// Returns the COFF machine field of a PE file.
fn pe_machine(path: &Path) -> Option<u16> {
    let mut data = Vec::new();
    fs::File::open(path).ok()?.take(4096).read_to_end(&mut data).ok()?;
    if data.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32::from_le_bytes(data.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if data.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(data.get(pe + 4..pe + 6)?.try_into().ok()?))
}
