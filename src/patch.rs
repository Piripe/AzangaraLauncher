//! Skipping the Direct3D builds' configuration dialog.
//!
//! `nsAzangaraGame::InitDialog` ends with a tail call to a function that runs
//! `DialogBoxParamA(..., MAKEINTRESOURCE(0x66), ...)` and returns `result == 1`
//! (the dialog's OK/Play buttons only call `EndDialog(hwnd, 1)`; every setting
//! it shows is a plain config variable). Making that function start with
//! `mov al, 1; ret` is therefore equivalent to clicking Play.
//!
//! The change is applied only in memory: [`crate::inject`] starts the game
//! suspended, writes the bytes into the new process and resumes it. Files on
//! disk are never modified.

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

/// Bytes to replace in a loaded image, relative to its base address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryPatch {
    pub rva: u32,
    pub expected: Vec<u8>,
    pub replacement: Vec<u8>,
}

impl MemoryPatch {
    /// `RVA:EXPECTED:REPLACEMENT` in hex, the format `azl-inject` reads from `AZL_PATCH`.
    pub fn spec(&self) -> String {
        let hex = |b: &[u8]| b.iter().map(|b| format!("{b:02x}")).collect::<String>();
        format!("{:x}:{}:{}", self.rva, hex(&self.expected), hex(&self.replacement))
    }
}

/// The in-memory patch that skips the config dialog of a Direct3D `game.exe`.
pub fn dialog_patch(exe: &[u8]) -> Result<MemoryPatch, String> {
    let offset = find_dialog_function(exe)?;
    let rva = file_offset_to_rva(exe, offset).ok_or("dialog function is outside every section")?;
    Ok(MemoryPatch {
        rva,
        expected: exe[offset..offset + RETURN_TRUE.len()].to_vec(),
        replacement: RETURN_TRUE.to_vec(),
    })
}

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

/// Maps a file offset to an RVA using the PE section table.
fn file_offset_to_rva(exe: &[u8], offset: usize) -> Option<u32> {
    let u16_at = |o: usize| Some(u16::from_le_bytes(exe.get(o..o + 2)?.try_into().ok()?));
    let u32_at = |o: usize| Some(u32::from_le_bytes(exe.get(o..o + 4)?.try_into().ok()?));
    let pe = u32_at(0x3c)? as usize;
    if exe.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let sections = u16_at(pe + 6)? as usize;
    let table = pe + 24 + u16_at(pe + 20)? as usize;
    (0..sections).find_map(|i| {
        let s = table + i * 40;
        let (va, raw_size, raw_ptr) = (u32_at(s + 12)?, u32_at(s + 16)?, u32_at(s + 20)?);
        let rel = (offset as u32).checked_sub(raw_ptr)?;
        (rel < raw_size).then_some(va + rel)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal PE: one section with raw data at 0x200 mapped at RVA 0x1000.
    fn fake_pe(code: &[u8]) -> Vec<u8> {
        let mut exe = vec![0u8; 0x200];
        exe[..2].copy_from_slice(b"MZ");
        exe[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        exe[0x40..0x44].copy_from_slice(b"PE\0\0");
        exe[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
        exe[0x54..0x56].copy_from_slice(&0xe0u16.to_le_bytes());
        let s = 0x40 + 24 + 0xe0;
        exe[s + 12..s + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        exe[s + 16..s + 20].copy_from_slice(&(code.len() as u32).to_le_bytes());
        exe[s + 20..s + 24].copy_from_slice(&0x200u32.to_le_bytes());
        exe.extend_from_slice(code);
        exe
    }

    fn dialog_code() -> Vec<u8> {
        let mut code = vec![0xcc; 4];
        code.extend_from_slice(&[0x6a, 0xff, 0x68, 1, 2, 3, 4, 0x90, 0x90]);
        code.extend_from_slice(&[0x6a, 0x66, 0x6a, 0, 0x6a, 0, 0xc7, 0x44, 0x24, 0x40, 0, 0, 0, 0, 0xff, 0x15]);
        code.extend_from_slice(&[0; 8]);
        code
    }

    #[test]
    fn finds_function_start() {
        assert_eq!(find_dialog_function(&dialog_code()), Ok(4));
    }

    #[test]
    fn builds_memory_patch() {
        let patch = dialog_patch(&fake_pe(&dialog_code())).unwrap();
        assert_eq!(patch.rva, 0x1004);
        assert_eq!(patch.spec(), "1004:6aff68:b001c3");
    }

    #[test]
    fn rejects_missing_site() {
        assert!(find_dialog_function(&[0xcc; 64]).is_err());
    }
}
