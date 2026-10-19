//! Running the game with extra `.pak` files without copying them into its folder.
//!
//! Every build loads `*.pak` from its *working directory* (`EnumDirFiles("",
//! "pak")`) and resolves loose data files relative to it too. An overlay is a
//! temporary directory holding a symlink to every entry of the game folder plus
//! a symlink to each extra pak; the game is started with it as working
//! directory. Writes the game makes to existing files (config, log, saves
//! folder, ...) go through the symlinks as usual; top-level files it creates are
//! moved into the game folder when the overlay is closed, exactly where a normal
//! run would have put them.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Overlay {
    dir: PathBuf,
    game_dir: PathBuf,
    linked: HashSet<PathBuf>,
}

impl Overlay {
    pub fn create(game_dir: &Path, extra_paks: &[PathBuf]) -> io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("azl-overlay-{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        let mut overlay = Overlay { dir, game_dir: game_dir.to_owned(), linked: HashSet::new() };

        let mut taken = HashSet::new();
        for entry in fs::read_dir(game_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            taken.insert(name.to_string_lossy().to_lowercase());
            overlay.link(&entry.path(), &name.to_string_lossy())?;
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
            if self.linked.contains(&path) || !path.is_file() {
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

        let overlay = Overlay::create(&game, &[mods.join("lebel.pak"), mods.join("data.pak")]).unwrap();
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
}
