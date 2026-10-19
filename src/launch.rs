//! Starting the game.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result};

use crate::display::DisplaySettings;
use crate::install::{Engine, Install};
use crate::patch;

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
    /// Show the Direct3D configuration dialog instead of skipping it.
    pub show_dialog: bool,
    pub runner: Runner,
    /// Wait for the game to exit, then switch level auto-start back off so a
    /// plain start of the game goes to the main menu again.
    pub wait: bool,
}

/// Everything a launch will do, computed without touching the disk.
#[derive(Debug)]
pub struct LaunchPlan {
    pub install: Install,
    /// Config variables written before starting, in order.
    pub vars: Vec<(String, String)>,
    /// Executable that will be started.
    pub exe: PathBuf,
    pub runner: Runner,
    pub wait: bool,
    restore_designer: bool,
    patch_exe: bool,
}

pub fn plan(install: &Install, opts: &LaunchOptions) -> Result<LaunchPlan> {
    opts.display.validate().map_err(anyhow::Error::msg)?;

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

    let patch_exe = install.engine == Engine::Direct3D && !opts.show_dialog;
    let exe = if patch_exe {
        // Fail early if this build is not recognised.
        let bytes = std::fs::read(&install.exe)?;
        patch::find_dialog_function(&bytes)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("cannot skip the config dialog of {}", install.exe.display()))?;
        install.dir.join(patch::PATCHED_EXE_NAME)
    } else {
        install.exe.clone()
    };

    Ok(LaunchPlan {
        install: install.clone(),
        vars,
        exe,
        runner: opts.runner.clone(),
        wait: opts.wait,
        restore_designer: opts.level.is_some(),
        patch_exe,
    })
}

impl LaunchPlan {
    pub fn command(&self) -> Command {
        let mut cmd = match &self.runner {
            Runner::Native => Command::new(&self.exe),
            Runner::Wrapper { program, args } => {
                let mut cmd = Command::new(program);
                cmd.args(args).arg(&self.exe);
                if std::env::var_os("WINEDEBUG").is_none() {
                    cmd.env("WINEDEBUG", "-all");
                }
                cmd
            }
        };
        cmd.current_dir(&self.install.dir);
        cmd
    }

    /// Writes the config (and patched executable), then starts the game.
    /// Returns the exit status when waiting.
    pub fn execute(&self) -> Result<Option<ExitStatus>> {
        let install = &self.install;
        let mut cfg = install.load_config().context("reading config")?;
        for (k, v) in &self.vars {
            cfg.set(k, v).map_err(anyhow::Error::msg)?;
        }
        install.save_config(&cfg).context("writing config")?;
        if self.patch_exe {
            patch::skip_dialog_exe(install).context("writing dialog-free executable")?;
        }

        let mut child = self
            .command()
            .spawn()
            .with_context(|| format!("starting {}", self.exe.display()))?;
        if !self.wait {
            return Ok(None);
        }
        let status = child.wait()?;
        if self.restore_designer {
            // The game rewrites its config on exit, so reload before editing.
            let mut cfg = install.load_config()?;
            cfg.set("designer", "0").map_err(anyhow::Error::msg)?;
            install.save_config(&cfg)?;
        }
        Ok(Some(status))
    }
}
