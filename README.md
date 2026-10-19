# Azangara Launcher

A Rust library (`azangara`) and CLI (`azl`) that configure and start every Azangara build:
the Direct3D 8 versions (`game.exe`, e.g. 1.35, 1.73, 1.77.2) and the OpenGL port
(`azangara.exe`). Game files are never modified. The only file it writes is the game's own
`Azangara.cfg`.

It can:
- change display settings (resolution, fullscreen/windowed, refresh, vsync, depth, FOV, etc.);
- start the game without the Direct3D configuration dialog (patched in memory only);
- open a level straight after the intro, including a level from a `.pak` stored outside the game folder;
- list the levels inside `.pak` archives;
- read and edit any variable in `Azangara.cfg`.

```sh
cargo build --release      # binary: target/release/azl

azl list                                   # installs under ./AzangaraVersions (or --root / $AZANGARA_ROOT)
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

`build.rs` cross-builds the small `inject/` helper (`azl-inject.exe`) and embeds it in `azl`.
The helper is `no_std` and links with the toolchain's bundled `rust-lld`, so MinGW is not needed.
It needs only:

- `rustup target add i686-pc-windows-gnu`
- `llvm-dlltool` on `PATH` (or set `$AZL_DLLTOOL`)

If either is missing, `azl` still builds with a warning. Skipping the dialog then needs a
pre-built helper passed in `$AZL_INJECT`. When `azl` runs, it extracts the helper into the
system temp directory (`azl-inject-<hash>/`).

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
  launcher creates `$TMPDIR/azl-overlay-<pid>/`, holding symlinks to everything in the game
  folder plus the extra paks, and starts the game with that as its working directory. Writes
  to existing files (config, log, `saves/`) go through the links as usual. Top-level files
  the game creates are moved to the game folder afterwards, and the overlay is deleted.
  Levels in extra paks are detected by content (`$header` … `$rooms`), so any path works,
  e.g. `assets/lebel/lebel.txt`.
- **`.pak` archives:** `"PACK"`, 2-byte version, `u32` directory size, then 136-byte entries
  (128-byte path, `u32` offset, `u32` size). File data is nibble-swapped.
