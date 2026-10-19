# Azangara Launcher

A Rust library (`azangara`) and CLI (`azl`) that configure and start every Azangara build:
the Direct3D 8 versions (`game.exe`, e.g. 1.35, 1.73, 1.77.2) and the OpenGL port
(`azangara.exe`).

It can:
- change display settings (resolution, fullscreen/windowed, refresh, vsync, depth, FOV, etc.);
- start the game without the Direct3D configuration dialog;
- open a level straight after the intro;
- list the levels inside the `.pak` archives;
- read and edit any variable in `Azangara.cfg`.

```sh
cargo build --release      # binary: target/release/azl

azl list                                   # installs under ./AzangaraVersions (or --root / $AZANGARA_ROOT)
azl info 1.77.2                            # engine, files, display settings
azl levels opengl                          # level_1  GREEN PALACE  data.pak ...
azl display 1.35 -r 1920x1080 --fullscreen --vsync on
azl config 1.73 set nolife 1
azl launch 1.77.2                          # main menu, no config dialog
azl launch opengl --level level_2_1 -r 1280x720 --windowed
azl launch 1.35 -l level_1 --dry-run      # show what would happen
```

You can name a game by its folder path or by a unique part of its folder name.
On Linux the game runs through `wine` (override it with `$WINE`, `--runner` or `--runner-arg`).
On Windows it runs natively.

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
  `EndDialog(1)`, and every setting in the dialog is a config variable. The launcher writes
  `game_nodialog.exe` next to the original, with that function's first bytes replaced by
  `mov al,1; ret` (3 bytes). It finds the function by byte pattern, not a fixed address.
  The original `game.exe` is never modified. The OpenGL build has no dialog.
- **`.pak` archives:** `"PACK"`, 2-byte version, `u32` directory size, then 136-byte entries
  (128-byte path, `u32` offset, `u32` size). File data is nibble-swapped.
