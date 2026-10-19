//! Starting the game.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result};

use crate::display::DisplaySettings;
use crate::inject;
use crate::install::{Engine, Install};
use crate::overlay::Overlay;
use crate::patch::{self, MemoryPatch};

/// How the Windows executable is run.
#[derive(Debug, Clone)]
pub enum Runner {
    /// Run the executable directly (Windows).
    Native,
    /// Run through a compatibility layer such as `wine`, with extra arguments
    /// placed before the executable path.
    Wrapper { program: OsString, args: Vec<OsString> },
}

impl Default for Runner {
    /// Native on Windows, otherwise `$WINE` or `wine`.
    fn default() -> Self {
        if cfg!(windows) {
            Runner::Native
        } else {
            let program = std::env::var_os("WINE").unwrap_or_else(|| "wine".into());
            Runner::Wrapper { program, args: Vec::new() }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    /// Level to open right after the intro, as an engine path (`levels/x.txt`).
    pub level: Option<String>,
    pub display: DisplaySettings,
    /// Extra raw config variables to set before launching.
    pub vars: Vec<(String, String)>,
    /// Additional `.pak` archives made visible to the game (see [`crate::overlay`]).
    pub extra_paks: Vec<PathBuf>,
    /// Show the Direct3D configuration dialog instead of skipping it.
    pub show_dialog: bool,
    pub runner: Runner,
    /// Wait for the game to exit, then switch level auto-start back off so a
    /// plain start of the game goes to the main menu again, and remove the
    /// temporary overlay directory.
    pub wait: bool,
}

/// Everything a launch will do, computed without touching the disk.
#[derive(Debug)]
pub struct LaunchPlan {
    pub install: Install,
    /// Config variables written before starting, in order.
    pub vars: Vec<(String, String)>,
    /// In-memory patches applied through `azl-inject` (empty: started directly).
    pub patches: Vec<MemoryPatch>,
    pub extra_paks: Vec<PathBuf>,
    pub runner: Runner,
    pub wait: bool,
    restore_designer: bool,
}

pub fn plan(install: &Install, opts: &LaunchOptions) -> Result<LaunchPlan> {
    opts.display.validate().map_err(anyhow::Error::msg)?;
    for pak in &opts.extra_paks {
        crate::pak::Pak::open(pak).with_context(|| format!("reading {}", pak.display()))?;
    }

    let mut vars: Vec<(String, String)> =
        opts.display.to_vars().into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    vars.extend(opts.vars.iter().cloned());
    match &opts.level {
        Some(level) => {
            vars.push(("designer".into(), "1".into()));
            vars.push(("dev_level".into(), level.clone()));
        }
        None => vars.push(("designer".into(), "0".into())),
    }

    let mut patches = Vec::new();
    if install.engine == Engine::Direct3D && !opts.show_dialog {
        let bytes = std::fs::read(&install.exe)?;
        let patch = patch::dialog_patch(&bytes)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("cannot skip the config dialog of {}", install.exe.display()))?;
        patches.push(patch);
    }

    Ok(LaunchPlan {
        install: install.clone(),
        vars,
        patches,
        extra_paks: opts.extra_paks.clone(),
        runner: opts.runner.clone(),
        wait: opts.wait,
        restore_designer: opts.level.is_some(),
    })
}

impl LaunchPlan {
    /// The command that starts the game, with `cwd` as working directory
    /// (the game folder, or an overlay of it).
    pub fn command(&self, cwd: PathBuf) -> Result<Command> {
        let exe = &self.install.exe;
        let (program, game_arg): (OsString, OsString) = if self.patches.is_empty() {
            (exe.clone().into(), OsString::new())
        } else {
            // The helper gets the game as a Windows path: relative to the
            // working directory under wine (which holds the exe or a link to
            // it), absolute when native.
            let game = match self.runner {
                Runner::Native => exe.clone().into_os_string(),
                Runner::Wrapper { .. } => format!(".\\{}", exe.file_name().unwrap().to_string_lossy()).into(),
            };
            (inject::helper_path()?.into(), game)
        };

        let mut cmd = match &self.runner {
            Runner::Native => Command::new(&program),
            Runner::Wrapper { program: wrapper, args } => {
                let mut cmd = Command::new(wrapper);
                cmd.args(args).arg(&program);
                if std::env::var_os("WINEDEBUG").is_none() {
                    cmd.env("WINEDEBUG", "-all");
                }
                cmd
            }
        };
        if !self.patches.is_empty() {
            cmd.arg(game_arg).env(inject::PATCH_ENV, inject::patch_env(&self.patches));
        }
        cmd.current_dir(cwd);
        Ok(cmd)
    }

    /// Writes the config, then starts the game. Returns the exit status when waiting.
    pub fn execute(&self) -> Result<Option<ExitStatus>> {
        let install = &self.install;
        let mut cfg = install.load_config().context("reading config")?;
        for (k, v) in &self.vars {
            cfg.set(k, v).map_err(anyhow::Error::msg)?;
        }
        install.save_config(&cfg).context("writing config")?;

        let overlay = if self.extra_paks.is_empty() {
            None
        } else {
            Some(Overlay::create(&install.dir, &self.extra_paks).context("creating overlay directory")?)
        };
        let cwd = overlay.as_ref().map_or_else(|| install.dir.clone(), |o| o.dir().to_owned());

        let spawned = self.command(cwd)?.spawn().context("starting the game");
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                if let Some(o) = overlay {
                    let _ = o.close();
                }
                return Err(e);
            }
        };
        if !self.wait {
            // The overlay must outlive us; it is left in the temp directory.
            std::mem::forget(overlay);
            return Ok(None);
        }
        let status = child.wait()?;
        if let Some(o) = overlay {
            o.close().context("removing overlay directory")?;
        }
        if self.restore_designer {
            // The game rewrites its config on exit, so reload before editing.
            let mut cfg = install.load_config()?;
            cfg.set("designer", "0").map_err(anyhow::Error::msg)?;
            install.save_config(&cfg)?;
        }
        Ok(Some(status))
    }
}
