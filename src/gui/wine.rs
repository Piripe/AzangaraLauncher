//! Running the game through Wine on Linux/macOS (native on Windows).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use azangara::Runner;

use super::store::{Paths, WineSettings};

/// Wine is needed everywhere but on Windows.
pub const NEEDED: bool = !cfg!(windows);

#[derive(Debug, Clone, PartialEq)]
pub struct WineBuild {
    pub label: String,
    /// What goes in [`WineSettings::wine`].
    pub command: String,
}

/// Wine builds found on this machine; the first one is the system default.
pub fn detect() -> Vec<WineBuild> {
    let mut builds = Vec::new();
    if let Some(version) = version_of("wine") {
        builds.push(WineBuild { label: format!("System Wine ({version})"), command: "wine".into() });
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let runner_dirs = [
        home.join(".local/share/lutris/runners/wine"),
        home.join(".var/app/net.lutris.Lutris/data/lutris/runners/wine"),
        home.join(".local/share/bottles/runners"),
        home.join(".var/app/com.usebottles.bottles/data/bottles/runners"),
    ];
    for dir in runner_dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut found: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path().join("bin/wine"))).filter(|p| p.is_file()).collect();
        found.sort();
        for wine in found {
            let name = wine.parent().and_then(Path::parent).and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
            builds.push(WineBuild { label: name.unwrap_or_default(), command: wine.to_string_lossy().into_owned() });
        }
    }
    #[cfg(target_os = "macos")]
    for app in ["Wine Staging", "Wine Stable", "Wine Devel"] {
        let wine = PathBuf::from(format!("/Applications/{app}.app/Contents/Resources/wine/bin/wine"));
        if wine.is_file() {
            builds.push(WineBuild { label: app.into(), command: wine.to_string_lossy().into_owned() });
        }
    }
    builds
}

fn version_of(wine: &str) -> Option<String> {
    let out = Command::new(wine).arg("--version").output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !v.is_empty()).then_some(v)
}

pub fn prefix(settings: &WineSettings, paths: &Paths) -> PathBuf {
    let prefix = settings.prefix.trim();
    if prefix.is_empty() { paths.compat.clone() } else { paths.resolve(prefix) }
}

/// Runner and environment for launching the game.
pub fn runner_and_env(settings: &WineSettings, paths: &Paths) -> (Runner, Vec<(OsString, OsString)>) {
    if !NEEDED {
        return (Runner::Native, Vec::new());
    }
    let program: OsString = if settings.wine.trim().is_empty() { "wine".into() } else { settings.wine.trim().into() };
    let args = split_args(&settings.args).into_iter().map(OsString::from).collect();
    (Runner::Wrapper { program, args }, env(settings, paths))
}

fn env(settings: &WineSettings, paths: &Paths) -> Vec<(OsString, OsString)> {
    let prefix = prefix(settings, paths);
    let _ = std::fs::create_dir_all(&prefix);
    let mut env: Vec<(OsString, OsString)> = vec![
        ("WINEPREFIX".into(), prefix.into()),
        // A fresh prefix would otherwise offer to install Mono and Gecko.
        ("WINEDLLOVERRIDES".into(), "mscoree,mshtml=".into()),
    ];
    for line in settings.env.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if !k.is_empty() {
                env.retain(|(e, _)| e != k);
                env.push((k.into(), v.trim().into()));
            }
        }
    }
    env
}

fn tool(settings: &WineSettings, name: &str) -> PathBuf {
    let wine = Path::new(settings.wine.trim());
    match wine.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(name),
        _ => PathBuf::from(name),
    }
}

fn wine_command(settings: &WineSettings, paths: &Paths) -> Command {
    let program = if settings.wine.trim().is_empty() { "wine" } else { settings.wine.trim() };
    let mut cmd = Command::new(program);
    cmd.envs(env(settings, paths)).current_dir(&paths.root);
    cmd
}

/// Creates or updates the prefix (`wineboot -u`), blocking.
pub fn init_prefix(settings: &WineSettings, paths: &Paths) -> Result<(), String> {
    let status = wine_command(settings, paths).args(["wineboot", "-u"]).status().map_err(|e| e.to_string())?;
    status.success().then_some(()).ok_or_else(|| format!("wineboot failed ({status})"))
}

/// Opens `winecfg` for the prefix, blocking until it is closed.
pub fn winecfg(settings: &WineSettings, paths: &Paths) -> Result<(), String> {
    wine_command(settings, paths).arg("winecfg").status().map(drop).map_err(|e| e.to_string())
}

/// Stops the game. Under Wine this ends every program of the prefix.
pub fn kill(settings: &WineSettings, paths: &Paths, pid: u32) -> Result<(), String> {
    let status = if NEEDED {
        Command::new(tool(settings, "wineserver")).arg("-k").envs(env(settings, paths)).status()
    } else {
        // The helper waits on the game: end the whole process tree.
        Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).status()
    };
    status.map(drop).map_err(|e| e.to_string())
}

/// Splits like a shell: whitespace separates, quotes group, `\` escapes.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut quote = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"') | None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                has = true;
            }
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(split_args(r#"a "b c" 'd e' f\ g"#), ["a", "b c", "d e", "f g"]);
        assert!(split_args("  ").is_empty());
        assert_eq!(split_args(r#""""#), [""]);
    }
}
