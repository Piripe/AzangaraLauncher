//! Reader for the game's `.pak` archives.
//!
//! Layout (little endian):
//! - `"PACK"`, 2 bytes version (`01 01`), `u32` directory size in bytes;
//! - directory: entries of 136 bytes = 128-byte NUL-padded path, `u32` absolute
//!   offset, `u32` size;
//! - file data, each byte nibble-swapped (`0x42` is stored for `0x24`).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"PACK";
const HEADER_LEN: usize = 10;
const ENTRY_LEN: usize = 136;
const NAME_LEN: usize = 128;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug)]
pub struct Pak {
    path: PathBuf,
    entries: Vec<Entry>,
}

impl Pak {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut header = [0u8; HEADER_LEN];
        file.read_exact(&mut header)?;
        if &header[..4] != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a PACK archive"));
        }
        let dir_len = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
        if !dir_len.is_multiple_of(ENTRY_LEN) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad PACK directory size"));
        }
        let mut dir = vec![0u8; dir_len];
        file.read_exact(&mut dir)?;

        let entries = dir
            .chunks_exact(ENTRY_LEN)
            .map(|e| {
                let name = &e[..NAME_LEN];
                let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(NAME_LEN)];
                Entry {
                    path: crate::config::latin1(name).replace('\\', "/"),
                    offset: u32::from_le_bytes(e[128..132].try_into().unwrap()),
                    size: u32::from_le_bytes(e[132..136].try_into().unwrap()),
                }
            })
            .collect();
        Ok(Self { path: path.to_owned(), entries })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(path))
    }

    /// Reads and de-obfuscates one file.
    pub fn read(&self, entry: &Entry) -> io::Result<Vec<u8>> {
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(entry.offset.into()))?;
        let mut data = vec![0u8; entry.size as usize];
        file.read_exact(&mut data)?;
        for b in &mut data {
            *b = b.rotate_left(4);
        }
        Ok(data)
    }
}
