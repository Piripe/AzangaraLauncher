//! Skipping the Direct3D builds' configuration dialog.
//!
//! `nsAzangaraGame::InitDialog` ends with a tail call to a function that runs
//! `DialogBoxParamA(..., MAKEINTRESOURCE(0x66), ...)` and returns `result == 1`
//! (the dialog's OK/Play buttons only call `EndDialog(hwnd, 1)`; every setting
//! it shows is a plain config variable). Replacing that function's first bytes
//! with `mov al, 1; ret` is therefore equivalent to clicking Play.
//!
//! The original executable is never modified: a patched copy is written next to
//! it (the game resolves data relative to its working directory).

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::install::{Engine, Install};

pub const PATCHED_EXE_NAME: &str = "game_nodialog.exe";

/// `push 0x66; push 0; push 0; mov dword [esp+??], 0; call [import]` (dialog resource 0x66)
const DIALOG_CALL: &[Option<u8>] = &[
    Some(0x6a), Some(0x66), Some(0x6a), Some(0x00), Some(0x6a), Some(0x00),
    Some(0xc7), Some(0x44), Some(0x24), None, Some(0x00), Some(0x00), Some(0x00), Some(0x00),
    Some(0xff), Some(0x15),
];
/// SEH prologue every candidate function starts with: `push -1; push imm32`.
const PROLOGUE: &[u8] = &[0x6a, 0xff, 0x68];
/// `mov al, 1; ret`
const RETURN_TRUE: &[u8] = &[0xb0, 0x01, 0xc3];
const MAX_PROLOGUE_DISTANCE: usize = 0x80;

/// File offset of the config-dialog function in a Direct3D `game.exe`.
pub fn find_dialog_function(exe: &[u8]) -> Result<usize, String> {
    let hits: Vec<usize> = (0..exe.len().saturating_sub(DIALOG_CALL.len()))
        .filter(|&i| {
            DIALOG_CALL
                .iter()
                .zip(&exe[i..])
                .all(|(p, b)| p.is_none_or(|p| p == *b))
        })
        .collect();
    let [site] = hits[..] else {
        return Err(format!("expected one config-dialog call site, found {}", hits.len()));
    };

    // Functions are separated by int3 padding; walk back to the start.
    let start = exe[..site]
        .iter()
        .rposition(|&b| b == 0xcc)
        .map_or(0, |p| p + 1);
    if site - start > MAX_PROLOGUE_DISTANCE || !exe[start..].starts_with(PROLOGUE) {
        return Err("config-dialog function has an unexpected prologue".into());
    }
    Ok(start)
}

/// Returns the executable to start: for Direct3D builds a dialog-free copy
/// (created or refreshed as needed), for OpenGL the original.
pub fn skip_dialog_exe(install: &Install) -> io::Result<PathBuf> {
    if install.engine == Engine::OpenGL {
        return Ok(install.exe.clone());
    }
    let mut exe = fs::read(&install.exe)?;
    let start = find_dialog_function(&exe)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    exe[start..start + RETURN_TRUE.len()].copy_from_slice(RETURN_TRUE);

    let target = install.dir.join(PATCHED_EXE_NAME);
    if fs::read(&target).ok().as_deref() != Some(&exe[..]) {
        fs::write(&target, &exe)?;
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_function_start() {
        let mut exe = vec![0xcc; 4];
        exe.extend_from_slice(&[0x6a, 0xff, 0x68, 1, 2, 3, 4, 0x90, 0x90]);
        exe.extend_from_slice(&[0x6a, 0x66, 0x6a, 0, 0x6a, 0, 0xc7, 0x44, 0x24, 0x40, 0, 0, 0, 0, 0xff, 0x15]);
        exe.extend_from_slice(&[0; 8]);
        assert_eq!(find_dialog_function(&exe), Ok(4));
    }

    #[test]
    fn rejects_missing_site() {
        assert!(find_dialog_function(&[0xcc; 64]).is_err());
    }
}
