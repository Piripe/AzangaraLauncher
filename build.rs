//! Cross-builds `inject/` (the `azl-inject.exe` helper) for i686-pc-windows-gnu
//! and embeds it into the launcher.
//!
//! The helper is `no_std` and links with the toolchain's bundled `rust-lld`, so
//! no MinGW installation is needed, only:
//! - `rustup target add i686-pc-windows-gnu`
//! - an LLVM `dlltool` (`llvm-dlltool`, or set `AZL_DLLTOOL`) to generate the
//!   kernel32/ntdll import libraries.
//!
//! If either is missing the launcher still builds; skipping the Direct3D config
//! dialog then needs a pre-built helper given through `AZL_INJECT`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const TARGET: &str = "i686-pc-windows-gnu";
/// Libraries the windows-gnu target always links; the no_std helper uses none.
const STUB_LIBS: &[&str] = &["gcc_eh", "pthread", "msvcrt", "mingwex", "mingw32", "gcc", "user32", "kernel32"];

fn main() {
    println!("cargo:rerun-if-changed=inject/src");
    println!("cargo:rerun-if-changed=inject/Cargo.toml");
    println!("cargo:rerun-if-env-changed=AZL_DLLTOOL");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let embedded = out.join("azl-inject.exe");
    match build_helper(&out) {
        Ok(exe) => fs::copy(exe, &embedded).map(drop).unwrap(),
        Err(e) => {
            println!("cargo:warning=azl-inject.exe not built ({e}); Direct3D dialog skipping will need AZL_INJECT");
            fs::write(&embedded, b"").unwrap();
        }
    }
}

fn build_helper(out: &Path) -> Result<PathBuf, String> {
    let dlltool = find_dlltool().ok_or("no llvm-dlltool found, set AZL_DLLTOOL")?;

    let stubs = out.join("inject-stubs");
    fs::create_dir_all(&stubs).map_err(|e| e.to_string())?;
    for lib in STUB_LIBS {
        fs::write(stubs.join(format!("lib{lib}.a")), b"!<arch>\n").map_err(|e| e.to_string())?;
    }

    let flags = [
        format!("-Cdlltool={}", dlltool.display()),
        "-Clinker=rust-lld".into(),
        "-Clinker-flavor=ld.lld".into(),
        format!("-L{}", stubs.display()),
        "-Clink-arg=--entry=_mainCRTStartup".into(),
        "-Clink-arg=--subsystem=console".into(),
    ];
    let target_dir = out.join("inject-target");
    let manifest = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("inject/Cargo.toml");
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--release", "--target", TARGET, "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target_dir)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"))
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("cargo build of the helper failed ({status}); is the {TARGET} target installed?"));
    }
    Ok(target_dir.join(TARGET).join("release/azl-inject.exe"))
}

fn find_dlltool() -> Option<PathBuf> {
    if let Some(p) = env::var_os("AZL_DLLTOOL") {
        return Some(p.into());
    }
    let path = env::var_os("PATH")?;
    let names = ["llvm-dlltool", "i686-w64-mingw32-dlltool"];
    env::split_paths(&path).find_map(|dir| {
        names.iter().map(|n| dir.join(n)).find(|p| p.is_file()).or_else(|| {
            // Versioned LLVM installs, e.g. llvm-dlltool-18.
            fs::read_dir(&dir).ok()?.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("llvm-dlltool-"))
            })
        })
    })
}
