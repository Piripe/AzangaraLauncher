//! Toolkit for configuring and launching Azangara.
//!
//! Supports both engine families:
//! - the Direct3D 8 builds (`game.exe`, 32-bit, e.g. 1.35 / 1.73 / 1.77.2), which
//!   always show a configuration dialog before starting;
//! - the OpenGL/SDL2 port (`azangara.exe`, 64-bit), which has no dialog.
//!
//! How the launcher drives the game (found by reverse engineering, identical in
//! every build):
//! - Settings live in `Azangara.cfg` as `name "value"` lines. The engine looks a
//!   variable up with `strstr` on the whole file, so only the first textual match
//!   counts; [`config::Config`] keeps that invariant when editing.
//! - When the start-up intro finishes the game opens the main menu, *unless*
//!   `designer` is non-zero and `dev_level` names a level file, in which case it
//!   starts that level directly. That is how a level is opened at launch.
//! - The Direct3D config dialog is shown unconditionally by one function
//!   returning "user pressed Play". [`patch`] locates it and [`inject`] starts
//!   the game suspended and makes it return true in memory; no file changes.
//! - `*.pak` archives are loaded from the working directory, so [`overlay`]
//!   adds extra paks by running the game from a temporary directory of
//!   symlinks instead of copying anything into the game folder.

pub mod config;
pub mod display;
pub mod inject;
pub mod install;
pub mod keys;
pub mod launch;
pub mod levels;
pub mod overlay;
pub mod pak;
pub mod patch;

pub use config::Config;
pub use display::DisplaySettings;
pub use install::{Engine, Install};
pub use launch::{LaunchOptions, RunningGame, Runner};
