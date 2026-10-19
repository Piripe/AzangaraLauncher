//! The `azl-inject.exe` helper (see `inject/`), which starts a program
//! suspended and patches its memory before any of its code runs.
//!
//! It is embedded in the launcher at build time and extracted on demand into
//! the system temp directory, never into a game folder.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::patch::MemoryPatch;

static HELPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/azl-inject.exe"));

/// Environment variable read by the helper.
pub const PATCH_ENV: &str = "AZL_PATCH";

/// Path to a usable helper: `$AZL_INJECT` if set, else the embedded copy.
pub fn helper_path() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("AZL_INJECT") {
        return Ok(path.into());
    }
    if HELPER.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "this azl was built without azl-inject.exe (see build.rs); set AZL_INJECT to a pre-built helper",
        ));
    }
    let dir = std::env::temp_dir().join(format!("azl-inject-{:016x}", fnv1a(HELPER)));
    let path = dir.join("azl-inject.exe");
    if fs::read(&path).ok().as_deref() != Some(HELPER) {
        fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!("azl-inject.{}.tmp", std::process::id()));
        fs::write(&tmp, HELPER)?;
        fs::rename(&tmp, &path)?;
    }
    Ok(path)
}

/// Value of [`PATCH_ENV`] for a set of patches.
pub fn patch_env(patches: &[MemoryPatch]) -> String {
    patches.iter().map(MemoryPatch::spec).collect::<Vec<_>>().join(";")
}

fn fnv1a(data: &[u8]) -> u64 {
    data.iter()
        .fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}
