//! Running the game with extra `.pak` files without copying them into its folder.
//!
//! Every build loads `*.pak` from its *working directory* (`EnumDirFiles("",
//! "pak")`) and resolves loose data files relative to it too. An overlay is a
//! temporary directory the game is started in, made of symlinks:
//!
//! - every entry of the game folder;
//! - each extra (level) pak, and its whole content extracted as loose files, so
//!   the level loads the same way whatever the build does with paks;
//! - with `merge_textures` (the OpenGL port), the `textures/` of the game's own
//!   paks: that build loads textures only as loose files (`GLTexture::Load` →
//!   DevIL `ilLoadImage(path)`), never from paks, although its `data.pak` holds
//!   them all.
//!
//! Where these overlap, level files win over the game paks' textures, which win
//! over the game folder's own files. Folders that nothing overrides stay single
//! symlinks; the others are rebuilt file by file. Extracted files are cached in
//! `$TMPDIR/azl-pak-cache/` while the pak is unchanged.
//!
//! Writes the game makes to existing files (config, log, saves folder, ...) go
//! through the symlinks as usual; top-level files it creates are moved into the
//! game folder when the overlay is closed, where a normal run would have put them.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::pak::Pak;

const TEXTURES: &str = "textures";

#[derive(Debug)]
pub struct Overlay {
    dir: PathBuf,
    game_dir: PathBuf,
    linked: HashSet<PathBuf>,
}

/// Which files of a pak to extract.
#[derive(Debug, Clone, Copy, Hash)]
enum Extract {
    All,
    Textures,
}

impl Overlay {
    pub fn create(game_dir: &Path, extra_paks: &[PathBuf], merge_textures: bool) -> io::Result<Self> {
        static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("azl-overlay-{}-{n}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        let mut overlay = Overlay { dir, game_dir: game_dir.to_owned(), linked: HashSet::new() };

        // Lower-case relative path → file, lowest priority first.
        let mut extracted = BTreeMap::new();
        if merge_textures {
            let mut paks: Vec<PathBuf> = fs::read_dir(game_dir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pak")))
                .collect();
            paks.sort();
            for pak in &paks {
                if let Ok(cache) = extracted_files(pak, Extract::Textures) {
                    collect_files(&cache, Path::new(""), &mut extracted);
                }
            }
        }
        for pak in extra_paks {
            // Not every extra pak has to be readable (or a pak at all).
            if let Ok(cache) = extracted_files(pak, Extract::All) {
                collect_files(&cache, Path::new(""), &mut extracted);
            }
        }
        let merged_tops: HashSet<String> = extracted
            .keys()
            .filter_map(|p: &PathBuf| p.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()))
            .collect();

        // Game folder: whole entries, or their files where something overrides them.
        let mut files = BTreeMap::new();
        let mut taken = HashSet::new();
        for entry in fs::read_dir(game_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            taken.insert(lower.clone());
            if !merged_tops.contains(&lower) {
                overlay.link(&entry.path(), &name)?;
            } else if entry.path().is_dir() {
                collect_files(&entry.path(), Path::new(&lower), &mut files);
            }
        }
        files.extend(extracted);
        for (rel, source) in files {
            // Lower-case names: Wine looks files up case-insensitively anyway.
            let link = overlay.dir.join(&rel);
            fs::create_dir_all(link.parent().unwrap())?;
            symlink(&source, &link)?;
        }

        for pak in extra_paks {
            let pak = pak.canonicalize()?;
            let name = pak.file_name().unwrap().to_string_lossy().into_owned();
            let mut unique = name.clone();
            let mut n = 1;
            while !taken.insert(unique.to_lowercase()) {
                unique = format!("azl{n}_{name}");
                n += 1;
            }
            overlay.link(&pak, &unique)?;
        }
        Ok(overlay)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Moves files the game created into the game folder and deletes the overlay.
    pub fn close(self) -> io::Result<()> {
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if self.linked.contains(&path) || path.is_symlink() || !path.is_file() {
                continue;
            }
            let target = self.game_dir.join(path.file_name().unwrap());
            if !target.exists() {
                fs::copy(&path, &target)?;
            }
        }
        // Removes the symlinks themselves, never what they point to.
        fs::remove_dir_all(&self.dir)
    }

    fn link(&mut self, target: &Path, name: &str) -> io::Result<()> {
        let link = self.dir.join(name);
        symlink(target, &link)?;
        self.linked.insert(link);
        Ok(())
    }
}

fn collect_files(dir: &Path, rel: &Path, out: &mut BTreeMap<PathBuf, PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name == ".complete" && rel.as_os_str().is_empty() {
            continue;
        }
        let rel = rel.join(name);
        if path.is_dir() {
            collect_files(&path, &rel, out);
        } else if path.is_file() {
            out.insert(rel, path);
        }
    }
}

/// Extracts (part of) a pak into `$TMPDIR/azl-pak-cache/<key>/`, reused while
/// the pak is unchanged (same path, size and modification time).
fn extracted_files(pak: &Path, what: Extract) -> io::Result<PathBuf> {
    let pak = pak.canonicalize()?;
    let meta = fs::metadata(&pak)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (3u8, what, &pak, meta.len(), meta.modified().ok()).hash(&mut hasher);
    let dir = std::env::temp_dir().join("azl-pak-cache").join(format!("{:016x}", hasher.finish()));
    let done = dir.join(".complete");
    if done.exists() {
        return Ok(dir);
    }
    let _ = fs::remove_dir_all(&dir);
    let archive = Pak::open(&pak)?;
    for entry in archive.entries() {
        let rel = Path::new(&entry.path);
        let wanted = match what {
            Extract::All => true,
            Extract::Textures => rel.components().next().is_some_and(|c| c.as_os_str().eq_ignore_ascii_case(TEXTURES)),
        };
        let safe = rel.components().all(|c| matches!(c, Component::Normal(_)));
        if !wanted || !safe || entry.path.ends_with('/') {
            continue;
        }
        let target = dir.join(rel);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(target, archive.read(entry)?)?;
    }
    fs::create_dir_all(&dir)?;
    fs::write(done, b"")?;
    Ok(dir)
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    if target.is_dir() {
        symlink_dir(target, link)
    } else {
        // Symlinks need Developer Mode; hard links work on the same volume.
        symlink_file(target, link).or_else(|_| fs::hard_link(target, link))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_game_and_extra_paks_then_cleans_up() {
        let root = std::env::temp_dir().join(format!("azl-overlay-test-{}", std::process::id()));
        let game = root.join("game");
        let mods = root.join("mods");
        fs::create_dir_all(game.join("saves")).unwrap();
        fs::create_dir_all(&mods).unwrap();
        fs::write(game.join("data.pak"), b"orig").unwrap();
        fs::write(mods.join("data.pak"), b"mod").unwrap();
        fs::write(mods.join("lebel.pak"), b"lebel").unwrap();

        let overlay = Overlay::create(&game, &[mods.join("lebel.pak"), mods.join("data.pak")], false).unwrap();
        let dir = overlay.dir().to_owned();
        assert_eq!(fs::read(dir.join("data.pak")).unwrap(), b"orig");
        assert_eq!(fs::read(dir.join("lebel.pak")).unwrap(), b"lebel");
        assert_eq!(fs::read(dir.join("azl1_data.pak")).unwrap(), b"mod");
        assert!(dir.join("saves").is_dir());

        fs::write(dir.join("Azangara.log"), b"log").unwrap();
        overlay.close().unwrap();
        assert!(!dir.exists());
        assert_eq!(fs::read(game.join("Azangara.log")).unwrap(), b"log");
        assert_eq!(fs::read(mods.join("lebel.pak")).unwrap(), b"lebel");
        assert!(!game.join("lebel.pak").exists());
        fs::remove_dir_all(root).unwrap();
    }

    /// A `.pak` with the given files (nibble-swapped data, 136-byte entries).
    fn write_pak(path: &Path, files: &[(&str, &[u8])]) {
        let dir_len = files.len() * 136;
        let mut out = b"PACK\x01\x01".to_vec();
        out.extend((dir_len as u32).to_le_bytes());
        let mut offset = 10 + dir_len;
        for (name, data) in files {
            let mut entry = [0u8; 136];
            entry[..name.len()].copy_from_slice(name.as_bytes());
            entry[128..132].copy_from_slice(&(offset as u32).to_le_bytes());
            entry[132..136].copy_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend(entry);
            offset += data.len();
        }
        for (name, data) in files {
            let swap = name.contains(".txt");
            out.extend(data.iter().map(|b| if swap { b.rotate_left(4) } else { *b }));
        }
        fs::write(path, out).unwrap();
    }

    #[test]
    fn merges_pak_and_loose_textures() {
        let root = std::env::temp_dir().join(format!("azl-overlay-tex-test-{}", std::process::id()));
        let game = root.join("game");
        fs::create_dir_all(game.join("Textures/menu")).unwrap();
        fs::write(game.join("Textures/menu/a.tga"), b"loose").unwrap();
        fs::write(game.join("Textures/only_loose.tga"), b"loose only").unwrap();
        write_pak(&game.join("data.pak"), &[("textures/menu/a.tga", b"pak a"), ("textures/b.jpg", b"pak b"), ("scripts/x.txt", b"x")]);
        fs::create_dir_all(game.join("levels")).unwrap();
        fs::write(game.join("levels/level_1.txt"), b"stock").unwrap();
        let level = root.join("level.pak");
        write_pak(&level, &[
            ("textures/custom/c.png", b"level c"),
            ("assets/indigo/indigo.txt", b"$header"),
            ("assets/indigo/wall.tga", b"tga"),
            ("levels/extra.txt", b"extra"),
        ]);

        let overlay = Overlay::create(&game, &[level], true).unwrap();
        let t = overlay.dir().join("textures");
        assert_eq!(fs::read(t.join("menu/a.tga")).unwrap(), b"pak a");
        assert_eq!(fs::read(t.join("only_loose.tga")).unwrap(), b"loose only");
        assert_eq!(fs::read(t.join("b.jpg")).unwrap(), b"pak b");
        assert_eq!(fs::read(t.join("custom/c.png")).unwrap(), b"level c");
        assert!(!t.join("x.txt").exists() && !t.is_symlink());
        let has_uppercase = fs::read_dir(overlay.dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name() == "Textures");
        assert!(!has_uppercase);
        // On case-sensitive filesystems (Linux), verify the uppercase directory does not exist.
        // On Windows and macOS, path lookups are case-insensitive so "Textures" resolves to "textures".
        #[cfg(not(any(windows, target_os = "macos")))]
        assert!(!overlay.dir().join("Textures").exists());
        // The whole level pak is extracted, scripts decoded.
        let d = overlay.dir();
        assert_eq!(fs::read(d.join("assets/indigo/indigo.txt")).unwrap(), b"$header");
        assert_eq!(fs::read(d.join("assets/indigo/wall.tga")).unwrap(), b"tga");
        // A game folder the level also uses is merged file by file.
        assert_eq!(fs::read(d.join("levels/level_1.txt")).unwrap(), b"stock");
        assert_eq!(fs::read(d.join("levels/extra.txt")).unwrap(), b"extra");
        assert!(d.join("level.pak").exists() && d.join("data.pak").is_symlink());
        overlay.close().unwrap();
        assert_eq!(fs::read(game.join("Textures/menu/a.tga")).unwrap(), b"loose");
        fs::remove_dir_all(root).unwrap();
    }
}
