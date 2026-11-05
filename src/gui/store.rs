//! Launcher folders and persisted settings.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use azangara::DisplaySettings;

pub const DEFAULT_API_URL: &str = "https://api.azangara-mods.piripe.fr";
const SETTINGS_FILE: &str = "launcher.json";

/// The launcher folder and its fixed sub-folders.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    /// Game builds, one sub-folder each.
    pub versions: PathBuf,
    /// Level `.pak` files (downloaded ones carry a `.json` sidecar).
    pub levels: PathBuf,
    /// Default Wine prefix.
    pub compat: PathBuf,
}

impl Paths {
    /// `$AZL_HOME`, else the folder of the executable when it holds a
    /// `versions` folder (portable install), else the working directory.
    pub fn locate() -> Self {
        let root = std::env::var_os("AZL_HOME").map(PathBuf::from).unwrap_or_else(|| {
            let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_owned));
            match exe_dir {
                Some(dir) if dir.join("versions").is_dir() => dir,
                _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            }
        });
        let root = root.canonicalize().unwrap_or(root);
        Self {
            versions: root.join("versions"),
            levels: root.join("levels"),
            compat: root.join("compat"),
            root,
        }
    }

    pub fn ensure(&self) {
        for dir in [&self.versions, &self.levels] {
            let _ = fs::create_dir_all(dir);
        }
    }

    /// Resolves a path from the settings: relative paths are relative to the launcher folder.
    pub fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() { p.to_owned() } else { self.root.join(p) }
    }

    fn settings_file(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub api_url: String,
    /// AzangaraMods session token.
    pub token: Option<String>,
    /// Folder name (in `versions/`) used to play levels.
    pub default_version: Option<String>,
    /// Display settings applied to every version without its own.
    pub display: Display,
    /// Per-version display settings, by folder name.
    pub version_display: BTreeMap<String, Display>,
    pub wine: WineSettings,
    /// Keep the same key bindings in every version.
    pub sync_keys: bool,
    /// The shared key bindings (Direct3D key names), from the last version played.
    pub keybinds: Option<StoredKeys>,
}

/// [`azangara::keys::KeySet`] as saved in the settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredKeys {
    pub binds: Vec<(String, String)>,
    pub chars: Vec<(String, String)>,
}

impl From<azangara::keys::KeySet> for StoredKeys {
    fn from(k: azangara::keys::KeySet) -> Self {
        Self { binds: k.binds, chars: k.chars }
    }
}

impl From<&StoredKeys> for azangara::keys::KeySet {
    fn from(k: &StoredKeys) -> Self {
        Self { binds: k.binds.clone(), chars: k.chars.clone() }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_url: DEFAULT_API_URL.into(),
            token: None,
            default_version: None,
            display: Display::default(),
            version_display: BTreeMap::new(),
            wine: WineSettings::default(),
            sync_keys: false,
            keybinds: None,
        }
    }
}

impl Settings {
    /// The saved settings, or `None` on first run.
    pub fn load(paths: &Paths) -> Option<Self> {
        fs::read(paths.settings_file()).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).expect("settings serialize");
        let tmp = paths.root.join(format!("{SETTINGS_FILE}.tmp"));
        fs::write(&tmp, json)?;
        fs::rename(tmp, paths.settings_file())
    }

    pub fn display_for(&self, version: &str) -> &Display {
        self.version_display.get(version).unwrap_or(&self.display)
    }
}

/// Display settings as edited in the launcher (all values explicit).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    /// Hz, 0 = adapter default.
    pub refresh: u32,
    pub vsync: bool,
    pub color_bits: u32,
    pub texture_bits: u32,
    pub texture_filter: bool,
    pub brightness: f32,
    pub fov: f32,
    /// 0 = unlimited.
    pub max_fps: u32,
    pub show_fps: bool,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fullscreen: false,
            refresh: 0,
            vsync: true,
            color_bits: 32,
            texture_bits: 32,
            texture_filter: true,
            brightness: 0.5,
            fov: 60.0,
            max_fps: 0,
            show_fps: false,
        }
    }
}

impl Display {
    /// Display settings read from a game config; missing values keep the defaults.
    pub fn from_settings(d: &DisplaySettings) -> Self {
        let def = Self::default();
        Self {
            width: d.width.unwrap_or(def.width),
            height: d.height.unwrap_or(def.height),
            fullscreen: d.fullscreen.unwrap_or(def.fullscreen),
            refresh: d.refresh.unwrap_or(def.refresh),
            vsync: d.vsync.unwrap_or(def.vsync),
            color_bits: d.color_bits.filter(|b| *b == 16 || *b == 32).unwrap_or(def.color_bits),
            texture_bits: d.texture_bits.filter(|b| *b == 16 || *b == 32).unwrap_or(def.texture_bits),
            texture_filter: d.texture_filter.unwrap_or(def.texture_filter),
            brightness: d.brightness.unwrap_or(def.brightness).clamp(0.0, 1.0),
            fov: d.fov.unwrap_or(def.fov),
            max_fps: d.max_fps.unwrap_or(def.max_fps),
            show_fps: d.show_fps.unwrap_or(def.show_fps),
        }
    }

    pub fn to_settings(&self) -> DisplaySettings {
        DisplaySettings {
            width: Some(self.width),
            height: Some(self.height),
            fullscreen: Some(self.fullscreen),
            refresh: Some(self.refresh),
            vsync: Some(self.vsync),
            color_bits: Some(self.color_bits),
            texture_bits: Some(self.texture_bits),
            texture_filter: Some(self.texture_filter),
            brightness: Some(self.brightness),
            fov: Some(self.fov),
            max_fps: Some(self.max_fps),
            show_fps: Some(self.show_fps),
            adapter: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WineSettings {
    /// Wine executable: a path, or a command looked up in `PATH`.
    pub wine: String,
    /// Prefix folder, relative to the launcher folder unless absolute.
    pub prefix: String,
    /// Arguments placed between `wine` and the game executable.
    pub args: String,
    /// `KEY=VALUE` lines added to the environment.
    pub env: String,
}

impl Default for WineSettings {
    fn default() -> Self {
        Self { wine: "wine".into(), prefix: "compat".into(), args: String::new(), env: String::new() }
    }
}
