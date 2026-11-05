//! Key bindings shared between game versions.
//!
//! A version's controls are its `keycmd <key> <command>` lines plus the
//! controls menu's `up/down/left/right/jump_ch1/_ch2` variables. The engines
//! name keys differently and compare names case-sensitively:
//! - Direct3D builds use lower-case Windows virtual-key names (`space`,
//!   `control`, `prior`, `a`, `f1`);
//! - the OpenGL port uses SDL scancode names (`Space`, `PageUp`, `A`, `F1`).
//!
//! [`KeySet`] stores bindings with Direct3D names and translates them for each
//! engine. Keys with no equivalent are left out; this includes every SDL name
//! containing a space (`Left Ctrl`, `Keypad 0`), which a `keycmd` line can't
//! hold. Letters map by name, so on non-QWERTY layouts a binding may land on
//! another physical key when it moves between the two engines.

use std::collections::HashSet;

use crate::config::Config;
use crate::install::Engine;

/// Variables written by the in-game controls menu.
pub const CHAR_VARS: [&str; 10] =
    ["up_ch1", "up_ch2", "down_ch1", "down_ch2", "left_ch1", "left_ch2", "right_ch1", "right_ch2", "jump_ch1", "jump_ch2"];

/// Direct3D name ↔ SDL scancode name, besides letters, digits and F-keys.
const NAMES: &[(&str, &str)] = &[
    ("space", "Space"),
    ("escape", "Escape"),
    ("tab", "Tab"),
    ("return", "Return"),
    ("back", "Backspace"),
    ("left", "Left"),
    ("right", "Right"),
    ("up", "Up"),
    ("down", "Down"),
    ("home", "Home"),
    ("end", "End"),
    ("prior", "PageUp"),
    ("next", "PageDown"),
    ("insert", "Insert"),
    ("delete", "Delete"),
    ("capital", "CapsLock"),
    ("scroll", "ScrollLock"),
    ("numlock", "Numlock"),
    ("snapshot", "PrintScreen"),
    ("apps", "Application"),
];

/// Key bindings with Direct3D key names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeySet {
    /// (key, command) pairs.
    pub binds: Vec<(String, String)>,
    /// Controls-menu variables (see [`CHAR_VARS`]).
    pub chars: Vec<(String, String)>,
}

impl KeySet {
    /// The bindings of a version's config.
    pub fn read(cfg: &Config, engine: Engine) -> Self {
        let binds = cfg.keycmds().into_iter().filter_map(|(k, c)| Some((to_common(engine, &k)?, c))).collect();
        let chars = CHAR_VARS.iter().filter_map(|v| Some((v.to_string(), cfg.get(v)?))).collect();
        Self { binds, chars }
    }

    /// Writes the bindings into a version's config. The keys and commands
    /// they bind lose their other bindings; the rest of the config's bindings
    /// (e.g. ones the other engine has no name for) stay.
    pub fn apply(&self, cfg: &mut Config, engine: Engine) -> Result<(), String> {
        let binds: Vec<(String, String)> =
            self.binds.iter().filter_map(|(k, c)| Some((from_common(engine, k)?, c.clone()))).collect();
        let keys: HashSet<&str> = binds.iter().map(|(k, _)| k.as_str()).collect();
        let commands: HashSet<&str> = binds.iter().map(|(_, c)| c.as_str()).collect();
        let mut merged: Vec<(String, String)> = cfg
            .keycmds()
            .into_iter()
            .filter(|(k, c)| !keys.contains(k.as_str()) && !commands.contains(c.as_str()))
            .collect();
        merged.extend(binds.iter().cloned());
        cfg.set_keycmds(&merged);
        for (var, value) in &self.chars {
            cfg.set(var, value)?;
        }
        Ok(())
    }
}

/// Engine key name → Direct3D name.
pub fn to_common(engine: Engine, key: &str) -> Option<String> {
    match engine {
        Engine::Direct3D => Some(key.to_ascii_lowercase()),
        Engine::OpenGL => {
            if is_letter_or_digit(key) || is_f_key(key) {
                Some(key.to_ascii_lowercase())
            } else {
                NAMES.iter().find(|(_, sdl)| *sdl == key).map(|(d3d, _)| d3d.to_string())
            }
        }
    }
}

/// Direct3D name → engine key name.
pub fn from_common(engine: Engine, key: &str) -> Option<String> {
    match engine {
        Engine::Direct3D => Some(key.to_string()),
        Engine::OpenGL => {
            if is_letter_or_digit(key) || is_f_key(key) {
                Some(key.to_ascii_uppercase())
            } else {
                NAMES.iter().find(|(d3d, _)| *d3d == key).map(|(_, sdl)| sdl.to_string())
            }
        }
    }
}

fn is_letter_or_digit(key: &str) -> bool {
    key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric()
}

fn is_f_key(key: &str) -> bool {
    key.len() >= 2
        && key[..1].eq_ignore_ascii_case("f")
        && key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_key_names() {
        assert_eq!(to_common(Engine::OpenGL, "Space").as_deref(), Some("space"));
        assert_eq!(to_common(Engine::OpenGL, "W").as_deref(), Some("w"));
        assert_eq!(to_common(Engine::OpenGL, "F12").as_deref(), Some("f12"));
        assert_eq!(to_common(Engine::OpenGL, "Left Ctrl"), None);
        assert_eq!(from_common(Engine::OpenGL, "prior").as_deref(), Some("PageUp"));
        assert_eq!(from_common(Engine::OpenGL, "control"), None);
        assert_eq!(from_common(Engine::Direct3D, "control").as_deref(), Some("control"));
    }

    #[test]
    fn syncs_opengl_bindings_into_direct3d() {
        let gl = Config::parse(b"keycmd A move_horz\r\nkeycmd D +move_horz\r\nkeycmd Space jump\r\nup_ch1 \"z\"\r\n");
        let d3d_text = b"keycmd tab m_show IDM_LEVEL_STAT\r\nkeycmd control jump\r\nkeycmd space jump\r\nkeycmd left move_horz\r\nkeycmd right +move_horz\r\nkeycmd p pause\r\nup_ch1 \"\"\r\n";
        let mut d3d = Config::parse(d3d_text);
        let keys = KeySet::read(&gl, Engine::OpenGL);
        keys.apply(&mut d3d, Engine::Direct3D).unwrap();
        let binds = d3d.keycmds();
        let has = |k: &str, c: &str| binds.iter().any(|(bk, bc)| bk == k && bc == c);
        assert!(has("a", "move_horz") && has("d", "+move_horz") && has("space", "jump"));
        // Commands moved to other keys lose their old keys; unrelated bindings stay.
        assert!(!has("left", "move_horz") && !has("control", "jump"));
        assert!(has("tab", "m_show IDM_LEVEL_STAT") && has("p", "pause"));
        assert_eq!(d3d.get("up_ch1").as_deref(), Some("z"));

        // And back: the Direct3D-only keys are skipped for OpenGL.
        let mut gl2 = gl.clone();
        KeySet::read(&d3d, Engine::Direct3D).apply(&mut gl2, Engine::OpenGL).unwrap();
        assert!(gl2.keycmds().iter().all(|(k, _)| k != "control" && k != "Left Ctrl"));
        assert!(gl2.keycmds().contains(&("Tab".to_string(), "m_show IDM_LEVEL_STAT".to_string())));
    }
}
