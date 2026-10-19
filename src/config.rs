//! Reading and editing `Azangara.cfg`.
//!
//! The file is a list of lines: `keycmd <key> <command>` bindings followed by
//! `name "value"` variables. It is handled as raw bytes because some values
//! (e.g. `curr_name`) contain non-UTF-8 data that must survive a rewrite.
//!
//! The engine resolves a variable with `strstr(file, name)`, takes the text up to
//! the next newline and accepts it only if its first token equals `name`. A key
//! therefore only takes effect if its first occurrence anywhere in the file is
//! the start of its own line; [`Config::set`] guarantees this.

use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Config {
    lines: Vec<Vec<u8>>,
    crlf: bool,
}

impl Config {
    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(Self::parse(&fs::read(path)?))
    }

    pub fn parse(bytes: &[u8]) -> Self {
        let crlf = bytes.windows(2).any(|w| w == b"\r\n") || bytes.is_empty();
        let lines = bytes
            .split(|&b| b == b'\n')
            .map(|l| l.strip_suffix(b"\r").unwrap_or(l).to_vec())
            .filter(|l| !l.is_empty())
            .collect();
        Self { lines, crlf }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let eol: &[u8] = if self.crlf { b"\r\n" } else { b"\n" };
        let mut out = Vec::new();
        for line in &self.lines {
            out.extend_from_slice(line);
            out.extend_from_slice(eol);
        }
        out
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        fs::write(path, self.to_bytes())
    }

    /// The value the engine would read for `key`, mirroring its lookup quirks.
    pub fn get(&self, key: &str) -> Option<String> {
        let text = self.to_bytes();
        let start = find(&text, key.as_bytes())?;
        let rest = &text[start..];
        let line = &rest[..rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len())];
        match parse_var(line) {
            Some((k, v)) if k == key.as_bytes() => Some(latin1(&v)),
            _ => None,
        }
    }

    /// Sets `key` to `value`, keeping it visible to the engine's lookup.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        validate_key(key)?;
        if value.contains(['"', '\r', '\n']) || value.chars().any(|c| c as u32 > 0xff) {
            return Err(format!(
                "value for '{key}' may not contain quotes, newlines or non-Latin-1 characters"
            ));
        }
        let mut line = format!("{key} \"").into_bytes();
        line.extend(value.chars().map(|c| c as u8));
        line.push(b'"');

        match self.var_index(key) {
            Some(i) => self.lines[i] = line,
            None => self.lines.push(line),
        }
        if self.get(key).as_deref() != Some(value) {
            // An earlier line contains `key` as a substring and would shadow it:
            // move ours to the top so it is the first match.
            let i = self.var_index(key).expect("line was just written");
            let line = self.lines.remove(i);
            self.lines.insert(0, line);
        }
        Ok(())
    }

    /// Removes every `key "..."` line.
    pub fn remove(&mut self, key: &str) {
        self.lines
            .retain(|l| !matches!(parse_var(l), Some((k, _)) if k == key.as_bytes()));
    }

    /// All `name "value"` variables in file order (key bindings excluded).
    pub fn vars(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.lines.iter().filter_map(|l| {
            let (k, v) = parse_var(l)?;
            (k != b"keycmd").then(|| (latin1(&k), latin1(&v)))
        })
    }

    /// `keycmd` key bindings as raw lines.
    pub fn key_bindings(&self) -> impl Iterator<Item = String> + '_ {
        self.lines
            .iter()
            .filter(|l| l.starts_with(b"keycmd "))
            .map(|l| latin1(l))
    }

    fn var_index(&self, key: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| matches!(parse_var(l), Some((k, _)) if k == key.as_bytes()))
    }
}

fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key == "keycmd" || !key.bytes().all(|b| b.is_ascii_graphic() && b != b'"') {
        return Err(format!("invalid variable name '{key}'"));
    }
    Ok(())
}

/// Splits a line into its first token and the (unquoted) second token.
fn parse_var(line: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let line = trim(line);
    let split = line.iter().position(|b| b.is_ascii_whitespace())?;
    let (key, rest) = (&line[..split], trim(&line[split..]));
    let value = match rest.strip_prefix(b"\"") {
        Some(q) => &q[..q.iter().position(|&b| b == b'"').unwrap_or(q.len())],
        None => &rest[..rest.iter().position(|b| b.is_ascii_whitespace()).unwrap_or(rest.len())],
    };
    Some((key.to_vec(), value.to_vec()))
}

fn trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(s.len());
    let end = s.iter().rposition(|b| !b.is_ascii_whitespace()).map_or(start, |e| e + 1);
    &s[start..end]
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The game uses a single-byte (Windows-1251/1252) encoding; map bytes 1:1.
pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"keycmd w cam_depth\r\nr_width \"640\"\r\ncurr_name \"\xe0\xe1\"\r\ndev_level \"levels/level_1.txt\"\r\n";

    #[test]
    fn round_trip_is_lossless() {
        assert_eq!(Config::parse(SAMPLE).to_bytes(), SAMPLE);
    }

    #[test]
    fn get_and_set_existing() {
        let mut cfg = Config::parse(SAMPLE);
        assert_eq!(cfg.get("r_width").as_deref(), Some("640"));
        cfg.set("r_width", "1920").unwrap();
        assert_eq!(cfg.get("r_width").as_deref(), Some("1920"));
        assert!(cfg.to_bytes().ends_with(b"dev_level \"levels/level_1.txt\"\r\n"));
    }

    #[test]
    fn shadowed_key_is_moved_to_top() {
        // `cam_depth` first appears inside the key binding line.
        let mut cfg = Config::parse(SAMPLE);
        assert_eq!(cfg.get("cam_depth"), None);
        cfg.set("cam_depth", "1").unwrap();
        assert_eq!(cfg.get("cam_depth").as_deref(), Some("1"));
        assert!(cfg.to_bytes().starts_with(b"cam_depth \"1\"\r\n"));
    }

    #[test]
    fn new_key_is_appended() {
        let mut cfg = Config::parse(SAMPLE);
        cfg.set("designer", "1").unwrap();
        assert_eq!(cfg.get("designer").as_deref(), Some("1"));
        assert!(cfg.to_bytes().ends_with(b"designer \"1\"\r\n"));
    }
}
