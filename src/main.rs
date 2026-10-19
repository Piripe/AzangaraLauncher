use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

use azangara::{levels, patch, DisplaySettings, Install, LaunchOptions, Runner};

/// Configure and launch Azangara (Direct3D 1.x builds and the OpenGL port).
#[derive(Parser)]
#[command(name = "azl", version)]
struct Cli {
    /// Folder containing the game folders [default: $AZANGARA_ROOT, ./AzangaraVersions or .]
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List installations found under the root folder.
    List,
    /// Show engine, files and current display settings of an installation.
    Info { game: String },
    /// List the levels that can be opened at launch.
    Levels { game: String },
    /// Show or change display settings (no flags: show).
    Display {
        game: String,
        #[command(flatten)]
        display: DisplayArgs,
    },
    /// Read or edit raw config variables.
    Config {
        game: String,
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Write the dialog-free executable without launching (Direct3D builds).
    Patch { game: String },
    /// Start the game, skipping the config dialog.
    Launch(LaunchArgs),
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print every variable (default).
    Show,
    /// Print the value the game would read.
    Get { key: String },
    /// Set a variable.
    Set { key: String, value: String },
    /// Delete a variable so the game uses its default.
    Unset { key: String },
}

#[derive(Args)]
struct LaunchArgs {
    /// Installation folder, or the name (or unique part of the name) of a folder under the root.
    game: String,
    /// Level to open after the intro: `level_1`, `level_1.txt` or `levels/level_1.txt`.
    #[arg(short, long)]
    level: Option<String>,
    #[command(flatten)]
    display: DisplayArgs,
    /// Extra config variable, repeatable: `--set nolife=1`.
    #[arg(long = "set", value_name = "KEY=VALUE", value_parser = parse_key_value)]
    vars: Vec<(String, String)>,
    /// Show the Direct3D config dialog instead of skipping it.
    #[arg(long)]
    show_dialog: bool,
    /// Program used to run the .exe [default: native on Windows, $WINE or `wine` elsewhere].
    #[arg(long, conflicts_with = "native")]
    runner: Option<OsString>,
    /// Argument passed to the runner before the executable, repeatable.
    #[arg(long = "runner-arg", allow_hyphen_values = true)]
    runner_args: Vec<OsString>,
    /// Run the executable directly, without wine.
    #[arg(long)]
    native: bool,
    /// Return immediately instead of waiting for the game to exit.
    /// (Level auto-start then stays enabled in the config until the next launch.)
    #[arg(long)]
    no_wait: bool,
    /// Print what would be done without writing anything or starting the game.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Default)]
struct DisplayArgs {
    /// Resolution as WIDTHxHEIGHT, e.g. 1920x1080.
    #[arg(short, long, value_parser = parse_resolution, conflicts_with_all = ["width", "height"])]
    resolution: Option<(u32, u32)>,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    /// Fullscreen mode.
    #[arg(short, long, conflicts_with = "windowed")]
    fullscreen: bool,
    /// Windowed mode.
    #[arg(short, long)]
    windowed: bool,
    /// Refresh rate in Hz (fullscreen).
    #[arg(long)]
    refresh: Option<u32>,
    /// Vertical sync: on/off.
    #[arg(long, value_parser = parse_switch)]
    vsync: Option<bool>,
    /// Colour depth: 16 or 32.
    #[arg(long)]
    color_bits: Option<u32>,
    /// Texture depth: 16 or 32.
    #[arg(long)]
    texture_bits: Option<u32>,
    /// Texture filtering: on/off.
    #[arg(long, value_parser = parse_switch)]
    texture_filter: Option<bool>,
    /// Brightness from 0 to 1 (default 0.5).
    #[arg(long)]
    brightness: Option<f32>,
    /// Field of view in degrees (default 60).
    #[arg(long)]
    fov: Option<f32>,
    /// Frame rate cap, 0 = unlimited.
    #[arg(long)]
    max_fps: Option<u32>,
    /// FPS counter: on/off.
    #[arg(long, value_parser = parse_switch)]
    show_fps: Option<bool>,
    /// Direct3D adapter index.
    #[arg(long)]
    adapter: Option<u32>,
}

impl DisplayArgs {
    fn settings(&self) -> DisplaySettings {
        DisplaySettings {
            width: self.resolution.map(|r| r.0).or(self.width),
            height: self.resolution.map(|r| r.1).or(self.height),
            fullscreen: match (self.fullscreen, self.windowed) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
            refresh: self.refresh,
            vsync: self.vsync,
            color_bits: self.color_bits,
            texture_bits: self.texture_bits,
            texture_filter: self.texture_filter,
            brightness: self.brightness,
            fov: self.fov,
            max_fps: self.max_fps,
            show_fps: self.show_fps,
            adapter: self.adapter,
        }
    }
}

fn parse_resolution(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once(['x', 'X']).ok_or("expected WIDTHxHEIGHT")?;
    Ok((w.trim().parse().map_err(|_| "bad width")?, h.trim().parse().map_err(|_| "bad height")?))
}

fn parse_switch(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Ok(true),
        "off" | "0" | "false" | "no" => Ok(false),
        _ => Err("expected on or off".into()),
    }
}

fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let (k, v) = s.split_once('=').ok_or("expected KEY=VALUE")?;
    Ok((k.trim().to_string(), v.to_string()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.clone().unwrap_or_else(default_root);

    match cli.command {
        Cmd::List => {
            let installs = Install::scan(&root)
                .with_context(|| format!("scanning {}", root.display()))?;
            if installs.is_empty() {
                println!("No installation found in {}", root.display());
            }
            for i in installs {
                println!("{:<24} {:<9} {}", i.name(), i.engine.name(), i.dir.display());
            }
        }
        Cmd::Info { game } => {
            let install = find_install(&root, &game)?;
            let cfg = install.load_config()?;
            println!("Folder:     {}", install.dir.display());
            println!("Engine:     {}", install.engine.name());
            println!("Executable: {}", install.exe.display());
            println!("Config:     {}", install.config_path().display());
            print_display(&DisplaySettings::read(&cfg));
            let auto = cfg.get("designer").is_some_and(|v| v.parse::<f32>().is_ok_and(|v| v != 0.0));
            let level = cfg.get("dev_level").unwrap_or_default();
            println!(
                "Level auto-start: {}",
                if auto && !level.is_empty() { level } else { "off".into() }
            );
        }
        Cmd::Levels { game } => {
            let install = find_install(&root, &game)?;
            for l in levels::list(&install) {
                println!(
                    "{:<20} {:<26} {}",
                    l.short_name(),
                    l.title.as_deref().unwrap_or("-"),
                    l.source.as_deref().unwrap_or("(loose file)")
                );
            }
        }
        Cmd::Display { game, display } => {
            let install = find_install(&root, &game)?;
            let mut cfg = install.load_config()?;
            let settings = display.settings();
            if !settings.is_empty() {
                settings.apply(&mut cfg).map_err(anyhow::Error::msg)?;
                install.save_config(&cfg)?;
            }
            print_display(&DisplaySettings::read(&cfg));
        }
        Cmd::Config { game, action } => {
            let install = find_install(&root, &game)?;
            let mut cfg = install.load_config()?;
            match action.unwrap_or(ConfigAction::Show) {
                ConfigAction::Show => {
                    for (k, v) in cfg.vars() {
                        println!("{k} = {v:?}");
                    }
                }
                ConfigAction::Get { key } => match cfg.get(&key) {
                    Some(v) => println!("{v}"),
                    None => bail!("'{key}' is not set (the game uses its default)"),
                },
                ConfigAction::Set { key, value } => {
                    cfg.set(&key, &value).map_err(anyhow::Error::msg)?;
                    install.save_config(&cfg)?;
                }
                ConfigAction::Unset { key } => {
                    cfg.remove(&key);
                    install.save_config(&cfg)?;
                }
            }
        }
        Cmd::Patch { game } => {
            let install = find_install(&root, &game)?;
            let exe = patch::skip_dialog_exe(&install)?;
            println!("{}", exe.display());
        }
        Cmd::Launch(args) => launch(&root, args)?,
    }
    Ok(())
}

fn launch(root: &Path, args: LaunchArgs) -> Result<()> {
    let install = find_install(root, &args.game)?;
    let level = args.level.as_deref().map(|l| levels::resolve(&install, l));
    let runner = match (args.native, args.runner) {
        (true, _) => Runner::Native,
        (false, Some(program)) => Runner::Wrapper { program, args: args.runner_args },
        (false, None) => match Runner::default() {
            Runner::Wrapper { program, .. } => Runner::Wrapper { program, args: args.runner_args },
            native => native,
        },
    };
    let opts = LaunchOptions {
        level,
        display: args.display.settings(),
        vars: args.vars,
        show_dialog: args.show_dialog,
        runner,
        wait: !args.no_wait,
    };
    let plan = azangara::launch::plan(&install, &opts)?;

    println!("{} ({})", install.name(), install.engine.name());
    for (k, v) in &plan.vars {
        println!("  set {k} \"{v}\"");
    }
    println!("  run {:?}", plan.command());
    if args.dry_run {
        return Ok(());
    }
    if let Some(status) = plan.execute()? {
        println!("Game exited: {status}");
    }
    Ok(())
}

fn print_display(d: &DisplaySettings) {
    let opt = |v: Option<String>| v.unwrap_or_else(|| "(default)".into());
    let onoff = |v: Option<bool>| opt(v.map(|b| if b { "on" } else { "off" }.into()));
    println!("Display:");
    println!(
        "  resolution     {}x{}",
        opt(d.width.map(|v| v.to_string())),
        opt(d.height.map(|v| v.to_string()))
    );
    println!(
        "  mode           {}",
        opt(d.fullscreen.map(|f| if f { "fullscreen" } else { "windowed" }.into()))
    );
    println!("  refresh        {}", opt(d.refresh.map(|v| format!("{v} Hz"))));
    println!("  vsync          {}", onoff(d.vsync));
    println!("  color bits     {}", opt(d.color_bits.map(|v| v.to_string())));
    println!("  texture bits   {}", opt(d.texture_bits.map(|v| v.to_string())));
    println!("  texture filter {}", onoff(d.texture_filter));
    println!("  brightness     {}", opt(d.brightness.map(|v| v.to_string())));
    println!("  fov            {}", opt(d.fov.map(|v| v.to_string())));
    println!("  max fps        {}", opt(d.max_fps.map(|v| if v == 0 { "unlimited".into() } else { v.to_string() })));
    println!("  show fps       {}", onoff(d.show_fps));
    if let Some(a) = d.adapter {
        println!("  d3d adapter    {a}");
    }
}

fn default_root() -> PathBuf {
    if let Some(root) = std::env::var_os("AZANGARA_ROOT") {
        return root.into();
    }
    let versions = PathBuf::from("AzangaraVersions");
    if versions.is_dir() {
        versions
    } else {
        PathBuf::from(".")
    }
}

/// Accepts a folder path, or the exact / unique partial name of a folder under `root`.
fn find_install(root: &Path, game: &str) -> Result<Install> {
    let path = Path::new(game);
    if path.is_dir() {
        return Ok(Install::open(path)?);
    }
    let installs = Install::scan(root).with_context(|| format!("scanning {}", root.display()))?;
    let wanted = game.to_lowercase();
    if let Some(i) = installs.iter().find(|i| i.name().to_lowercase() == wanted) {
        return Ok(i.clone());
    }
    let matches: Vec<_> = installs
        .iter()
        .filter(|i| i.name().to_lowercase().contains(&wanted))
        .collect();
    match matches[..] {
        [one] => Ok(one.clone()),
        [] => bail!("no installation matching '{game}' in {} (try `azl list`)", root.display()),
        _ => bail!(
            "'{game}' is ambiguous: {}",
            matches.iter().map(|i| i.name()).collect::<Vec<_>>().join(", ")
        ),
    }
}
