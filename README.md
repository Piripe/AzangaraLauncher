# Azangara Launcher

A desktop launcher (`azangara-launcher`), a CLI (`azl`) and a Rust library (`azangara`) that
configure and start every Azangara build: the Direct3D 8 versions (`game.exe`, e.g. 1.35,
1.73, 1.77.2) and the OpenGL port (`azangara.exe`). Game files are never modified. The only
file it writes in a game folder is the game's own `Azangara.cfg`.

## Launcher (GUI)

```sh
cargo run --release --bin azangara-launcher
```

It is styled like the AzangaraMods website and has a navigation rail with three pages:

- **Levels**
  - **Browse** searches AzangaraMods. It has the website's filter sidebar (difficulty, size,
    tags with their counts, **Clear filters**) and its sort orders.
  - **Installed** lists every `.pak` in `levels/`, whether downloaded or dropped in by hand.
  - Opening a level shows its gallery, description and details.
  - **Download** gets the latest file. Downloaded levels show **Update** when the author
    uploads a new file.
  - **Play** opens the level with the default version. Its arrow lets you pick another version
    or delete the level.
- **Versions** lists the game folders in `versions/`.
  - The default version (used for levels) has a check mark next to its **Play** button.
  - **Play** starts the game normally, still skipping the setup screen.
  - The ⋮ menu opens the version's own display settings (a pop-up), makes it the default,
    opens its folder, or deletes it after confirmation. A version that is a symlink only loses
    the link.
- **Settings** (the gear at the bottom right of the navigation)
  - the game's display settings, used by every version without its own;
  - **Controls**: an option to keep the same key bindings in every version. Bindings changed
    in game are copied to the other versions when they're launched. Keys are translated
    between the Direct3D names (`space`, `prior`) and the OpenGL SDL names (`Space`, `PageUp`).
    Keys with no equivalent are left alone, and letters map by name (QWERTY);
  - the AzangaraMods API URL (`https://api.azangara-mods.piripe.fr` by default);
  - Wine (Linux and macOS): the Wine build (the system Wine by default, plus any Lutris or
    Bottles runners found), the prefix (`compat` by default), extra arguments and environment
    variables, and buttons to set up the prefix or open `winecfg`;
  - the folders it uses.
- **Log in** at the bottom signs in to AzangaraMods, so you can download your own drafts.
  The session token is stored in `launcher.json`.

Folders live in the launcher folder:

| Path | Contents |
| --- | --- |
| `versions/` | one folder per game version |
| `levels/` | level `.pak` files; downloads also get a `.json` file with their origin |
| `compat/` | default Wine prefix, created on the first launch |
| `launcher.json` | settings |

The launcher folder is `$AZL_HOME` if set. Otherwise it is the folder of the executable when
that folder contains `versions/` (a portable install), or else the current directory. On the
first run, the display settings are read from the default version's config.

It runs on Linux and Windows, and should work on macOS (not tested). On Windows the game
runs natively, so the Wine settings are hidden.

## CLI

```sh
cargo build --release      # binary: target/release/azl

azl list                                   # installs under ./versions (or --root / $AZANGARA_ROOT)
azl info 1.77.2                            # engine, files, display settings
azl levels opengl --pak levels/lebel.pak   # stock levels + the ones in an extra pak
azl display 1.35 -r 1920x1080 --fullscreen --vsync on
azl config 1.73 set nolife 1
azl verify 1.35                            # check the dialog skip works for this exe (game never runs)
azl launch 1.77.2                          # main menu, no config dialog
azl launch opengl --level level_2_1 -r 1280x720 --windowed
azl launch 1.77.2 --level levels/lebel.pak # pak outside the game folder, opens its only level
azl launch 1.35 --pak levels/lebel.pak --level lebel --dry-run
```

You can name a game by its folder path or by a unique part of its folder name.
On Linux the game runs through `wine` (override it with `$WINE`, `--runner` or `--runner-arg`).
On Windows it runs natively.

## Building

`build.rs` cross-builds the small `inject/` helper (`azl-inject.exe`) and embeds it in both programs.
The helper is `no_std` and links with the toolchain's bundled `rust-lld`, so MinGW is not needed.
It needs only:

- `rustup target add i686-pc-windows-gnu`
- `llvm-dlltool` on `PATH` (or set `$AZL_DLLTOOL`)

If either is missing, the programs still build with a warning. Skipping the dialog then needs a
pre-built helper passed in `$AZL_INJECT`. When `azl` runs, it extracts the helper into the
system temp directory (`azl-inject-<hash>/`).

`cargo build --no-default-features` builds only the CLI and the library, without the GUI
dependencies.

## How it works

The details below were worked out from the binaries. The OpenGL build has debug symbols,
and the Direct3D builds contain the same engine code.

- **Config:** `Azangara.cfg` holds `name "value"` lines. The engine finds a variable with
  `strstr` on the whole file and uses only the first match. If that match is not at the start
  of the variable's own line, the game silently falls back to the default value. `Config::set`
  edits lines in place and keeps every key visible to that lookup. Line endings and
  non-UTF-8 bytes are preserved.
- **Level at launch:** when the start-up intro event ends, the game checks `designer != 0`
  and a non-empty `dev_level`. If both are set it calls `StartLevel(dev_level)` instead of
  opening `IDM_MAIN_MENU`. `designer` has no other use, and there are no useful command-line
  arguments. The launcher writes both variables. After the game exits, it sets `designer`
  back to `0` (except with `--no-wait`), so a normal start still goes to the menu.
- **Skipping the dialog (Direct3D builds):** `InitDialog` always ends by calling a function
  that runs dialog `0x66` and returns whether Play was pressed. Play only calls
  `EndDialog(1)`, and every setting in the dialog is a config variable. The launcher finds
  that function by byte pattern and converts its offset to an RVA. `azl-inject.exe` then runs
  `CreateProcess(CREATE_SUSPENDED)` and reads the image base from the child's PEB. It checks
  that the original bytes are there and writes `mov al,1; ret` with `WriteProcessMemory`.
  Finally it resumes the game, waits for it and returns its exit code. Nothing on disk
  changes, and no DLL is involved. The OpenGL build has no dialog.
- **Extra `.pak` files:** every build loads `*.pak` from its *working directory*
  (`EnumDirFiles("", "pak")`), so there's no need to copy a pak into the game folder. The
  launcher creates `$TMPDIR/azl-overlay-<pid>-<n>/` and starts the game with that as its
  working directory. The overlay holds symlinks to everything in the game folder, plus each
  level pak. Each level pak is also extracted in full as loose files, so its assets load the
  same way whatever the build does with paks; for example, Indigo Palace keeps its textures
  under `assets/indigo/`. Level files win over the game's files. Folders that both use are
  merged file by file. Writes to existing files (config, log, `saves/`) go through the links
  as usual. Top-level files the game creates are moved to the game folder afterwards, and the
  overlay is deleted. Levels in extra paks are detected by content (`$header` … `$rooms`), so
  any path works, e.g. `assets/lebel/lebel.txt`.
- **OpenGL textures:** the OpenGL port loads textures only as loose files
  (`GLTexture::Load` → DevIL `ilLoadImage(path)`), never from paks, so it renders white
  without them. For this build the overlay also gets the `textures/` of the game's own paks.
  Those win over the game's `textures/` folder, which only fills in textures no pak has.
  Extracted files are cached in `$TMPDIR/azl-pak-cache/` until the pak changes.
- **Downloads:** the launcher downloads the website's `.pak.zip` (`?ext=zip`) and extracts the
  `.pak` itself.
- **`.pak` archives:** `"PACK"`, 2-byte version, `u32` directory size, then 136-byte entries
  (128-byte path, `u32` offset, `u32` size). Scripts (paths containing `.txt`) are
  nibble-swapped; all other data is stored as-is.

## Credits

- The background silhouettes and the icon come from the AzangaraMods website.
- [Inter](https://rsms.me/inter/) is under the SIL Open Font License (`assets/fonts/Inter-LICENSE.txt`).
- The icons are [Material Icons](https://fonts.google.com/icons) (Apache 2.0), the same set as the website.
