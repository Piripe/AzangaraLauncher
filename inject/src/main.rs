//! `azl-inject.exe <command line>`
//!
//! Starts `<command line>` suspended, applies the patches described by the
//! `AZL_PATCH` environment variable to the new process's memory, resumes it and
//! exits with its exit code. Nothing is written to disk.
//!
//! `AZL_PATCH` is a `;`-separated list of `RVA:EXPECTED:REPLACEMENT`, all hex,
//! e.g. `7fa0:6aff68:b001c3`. RVAs are relative to the main image's base, read
//! from the child's PEB. Each patch is applied only if the bytes currently in
//! memory equal EXPECTED. With `AZL_INJECT_DRY_RUN=1` the patches are verified
//! and the child is terminated without ever running.
//!
//! The helper is 32-bit and uses no CRT, so it runs under any wine prefix and
//! only targets 32-bit programs (the Direct3D Azangara builds).

#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr::{null, null_mut};

type Handle = *mut c_void;

#[repr(C)]
struct StartupInfoW {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    x_size: u32,
    y_size: u32,
    x_count_chars: u32,
    y_count_chars: u32,
    fill_attribute: u32,
    flags: u32,
    show_window: u16,
    reserved2_len: u16,
    reserved2: *mut u8,
    std_input: Handle,
    std_output: Handle,
    std_error: Handle,
}

#[repr(C)]
struct ProcessInformation {
    process: Handle,
    thread: Handle,
    process_id: u32,
    thread_id: u32,
}

#[repr(C)]
struct ProcessBasicInformation {
    exit_status: i32,
    peb: usize,
    affinity_mask: usize,
    base_priority: i32,
    process_id: usize,
    parent_process_id: usize,
}

const CREATE_SUSPENDED: u32 = 0x4;
const PAGE_EXECUTE_READWRITE: u32 = 0x40;
const INFINITE: u32 = 0xffff_ffff;
const STD_ERROR_HANDLE: u32 = -12i32 as u32;
/// `PEB::ImageBaseAddress` on 32-bit Windows.
const PEB_IMAGE_BASE: usize = 0x8;

#[link(name = "kernel32", kind = "raw-dylib", import_name_type = "undecorated")]
unsafe extern "system" {
    fn GetCommandLineW() -> *const u16;
    fn GetEnvironmentVariableW(name: *const u16, buf: *mut u16, len: u32) -> u32;
    fn CreateProcessW(
        app: *const u16,
        cmd: *mut u16,
        proc_attr: *const c_void,
        thread_attr: *const c_void,
        inherit: i32,
        flags: u32,
        env: *const c_void,
        cwd: *const u16,
        si: *const StartupInfoW,
        pi: *mut ProcessInformation,
    ) -> i32;
    fn ReadProcessMemory(p: Handle, addr: usize, buf: *mut u8, len: usize, read: *mut usize) -> i32;
    fn WriteProcessMemory(p: Handle, addr: usize, buf: *const u8, len: usize, written: *mut usize) -> i32;
    fn VirtualProtectEx(p: Handle, addr: usize, len: usize, prot: u32, old: *mut u32) -> i32;
    fn FlushInstructionCache(p: Handle, addr: usize, len: usize) -> i32;
    fn ResumeThread(t: Handle) -> u32;
    fn TerminateProcess(p: Handle, code: u32) -> i32;
    fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
    fn GetExitCodeProcess(p: Handle, code: *mut u32) -> i32;
    fn GetLastError() -> u32;
    fn GetStdHandle(which: u32) -> Handle;
    fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, overlapped: *mut c_void) -> i32;
    fn ExitProcess(code: u32) -> !;
}

#[link(name = "ntdll", kind = "raw-dylib", import_name_type = "undecorated")]
unsafe extern "system" {
    fn NtQueryInformationProcess(p: Handle, class: u32, info: *mut c_void, len: u32, ret: *mut u32) -> i32;
}

const MAX_PATCHES: usize = 8;
const MAX_PATCH_LEN: usize = 32;

#[derive(Clone, Copy)]
struct Patch {
    rva: usize,
    len: usize,
    expected: [u8; MAX_PATCH_LEN],
    replacement: [u8; MAX_PATCH_LEN],
}

const EMPTY_PATCH: Patch = Patch { rva: 0, len: 0, expected: [0; MAX_PATCH_LEN], replacement: [0; MAX_PATCH_LEN] };

static mut CMD_LINE: [u16; 32768] = [0; 32768];
static mut PATCH_SPEC: [u16; 1024] = [0; 1024];

#[unsafe(no_mangle)]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe { ExitProcess(run()) }
}

unsafe fn run() -> u32 {
    let mut patches = [EMPTY_PATCH; MAX_PATCHES];
    let Some(count) = (unsafe { read_patches(&mut patches) }) else {
        return fail("AZL_PATCH is missing or malformed", 120);
    };
    let cmd = unsafe { child_command_line() };
    if unsafe { *cmd } == 0 {
        return fail("usage: azl-inject.exe <program> [args...]", 120);
    }

    let si = StartupInfoW {
        cb: size_of::<StartupInfoW>() as u32,
        ..unsafe { core::mem::zeroed() }
    };
    let mut pi: ProcessInformation = unsafe { core::mem::zeroed() };
    let ok = unsafe {
        CreateProcessW(null(), cmd, null(), null(), 0, CREATE_SUSPENDED, null(), null(), &si, &mut pi)
    };
    if ok == 0 {
        return fail_code("cannot start the program, error", unsafe { GetLastError() }, 121);
    }

    let result = unsafe { apply(pi.process, &patches[..count]) };
    if let Err((msg, code)) = result {
        unsafe { TerminateProcess(pi.process, code) };
        return fail(msg, code);
    }
    if dry_run() {
        unsafe { TerminateProcess(pi.process, 0) };
        eprint(b"azl-inject: patches verified (dry run)\r\n");
        return 0;
    }

    unsafe {
        ResumeThread(pi.thread);
        WaitForSingleObject(pi.process, INFINITE);
        let mut code = 0;
        GetExitCodeProcess(pi.process, &mut code);
        code
    }
}

unsafe fn apply(process: Handle, patches: &[Patch]) -> Result<(), (&'static str, u32)> {
    let mut pbi: ProcessBasicInformation = unsafe { core::mem::zeroed() };
    let status = unsafe {
        NtQueryInformationProcess(process, 0, (&raw mut pbi).cast(), size_of::<ProcessBasicInformation>() as u32, null_mut())
    };
    let mut base = 0usize;
    if status != 0
        || !unsafe { read(process, pbi.peb + PEB_IMAGE_BASE, (&raw mut base).cast(), size_of::<usize>()) }
        || base == 0
    {
        return Err(("cannot read the image base of the program", 122));
    }

    // Verify everything before writing anything.
    for p in patches {
        let mut current = [0u8; MAX_PATCH_LEN];
        if !unsafe { read(process, base + p.rva, current.as_mut_ptr(), p.len) } {
            return Err(("cannot read the program's memory", 122));
        }
        if current[..p.len] != p.expected[..p.len] {
            return Err(("unexpected bytes in memory: unsupported game version", 123));
        }
    }
    for p in patches {
        let addr = base + p.rva;
        let mut old = 0;
        let mut written = 0;
        let ok = unsafe {
            VirtualProtectEx(process, addr, p.len, PAGE_EXECUTE_READWRITE, &mut old) != 0
                && WriteProcessMemory(process, addr, p.replacement.as_ptr(), p.len, &mut written) != 0
                && written == p.len
        };
        if !ok {
            return Err(("cannot write the program's memory", 124));
        }
        unsafe {
            VirtualProtectEx(process, addr, p.len, old, &mut old);
            FlushInstructionCache(process, addr, p.len);
        }
    }
    Ok(())
}

unsafe fn read(process: Handle, addr: usize, buf: *mut u8, len: usize) -> bool {
    let mut done = 0;
    (unsafe { ReadProcessMemory(process, addr, buf, len, &mut done) }) != 0 && done == len
}

/// Copies our command line minus argv[0] into a writable buffer.
unsafe fn child_command_line() -> *mut u16 {
    let mut p = unsafe { GetCommandLineW() };
    unsafe {
        if *p == b'"' as u16 {
            p = p.add(1);
            while *p != 0 && *p != b'"' as u16 {
                p = p.add(1);
            }
            if *p != 0 {
                p = p.add(1);
            }
        } else {
            while *p != 0 && *p != b' ' as u16 && *p != b'\t' as u16 {
                p = p.add(1);
            }
        }
        while *p == b' ' as u16 || *p == b'\t' as u16 {
            p = p.add(1);
        }
        let buf = &raw mut CMD_LINE;
        let buf = &mut *buf;
        let mut i = 0;
        while *p != 0 && i < buf.len() - 1 {
            buf[i] = *p;
            p = p.add(1);
            i += 1;
        }
        buf[i] = 0;
        buf.as_mut_ptr()
    }
}

unsafe fn read_patches(out: &mut [Patch; MAX_PATCHES]) -> Option<usize> {
    let spec = unsafe { &mut *(&raw mut PATCH_SPEC) };
    let name = wide::<10>(b"AZL_PATCH");
    let len = unsafe { GetEnvironmentVariableW(name.as_ptr(), spec.as_mut_ptr(), spec.len() as u32) } as usize;
    if len == 0 || len >= spec.len() {
        return None;
    }
    let mut count = 0;
    for entry in spec[..len].split(|&c| c == b';' as u16).filter(|e| !e.is_empty()) {
        let mut fields = entry.split(|&c| c == b':' as u16);
        let (Some(rva), Some(exp), Some(rep), None) = (fields.next(), fields.next(), fields.next(), fields.next()) else {
            return None;
        };
        let patch = out.get_mut(count)?;
        patch.rva = parse_hex_number(rva)?;
        let n = parse_hex_bytes(exp, &mut patch.expected)?;
        if n == 0 || parse_hex_bytes(rep, &mut patch.replacement)? != n {
            return None;
        }
        patch.len = n;
        count += 1;
    }
    (count > 0).then_some(count)
}

fn parse_hex_number(s: &[u16]) -> Option<usize> {
    if s.is_empty() || s.len() > 8 {
        return None;
    }
    s.iter().try_fold(0usize, |acc, &c| Some(acc << 4 | hex_digit(c)? as usize))
}

fn parse_hex_bytes(s: &[u16], out: &mut [u8; MAX_PATCH_LEN]) -> Option<usize> {
    if s.len() % 2 != 0 || s.len() / 2 > MAX_PATCH_LEN {
        return None;
    }
    for (i, pair) in s.chunks_exact(2).enumerate() {
        out[i] = hex_digit(pair[0])? << 4 | hex_digit(pair[1])?;
    }
    Some(s.len() / 2)
}

fn hex_digit(c: u16) -> Option<u8> {
    match c as u8 {
        b'0'..=b'9' if c < 0x80 => Some(c as u8 - b'0'),
        b'a'..=b'f' if c < 0x80 => Some(c as u8 - b'a' + 10),
        b'A'..=b'F' if c < 0x80 => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

fn dry_run() -> bool {
    let name = wide::<19>(b"AZL_INJECT_DRY_RUN");
    let mut buf = [0u16; 4];
    let n = unsafe { GetEnvironmentVariableW(name.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    n > 0 && n < 4 && buf[0] != b'0' as u16
}

/// NUL-terminated UTF-16 copy of an ASCII string; `N` is its length + 1.
fn wide<const N: usize>(s: &[u8]) -> [u16; N] {
    let mut out = [0u16; N];
    for (o, &c) in out.iter_mut().zip(s) {
        *o = c as u16;
    }
    out
}

fn eprint(msg: &[u8]) {
    unsafe {
        let mut written = 0;
        WriteFile(GetStdHandle(STD_ERROR_HANDLE), msg.as_ptr(), msg.len() as u32, &mut written, null_mut());
    }
}

fn fail(msg: &str, code: u32) -> u32 {
    eprint(b"azl-inject: ");
    eprint(msg.as_bytes());
    eprint(b"\r\n");
    code
}

fn fail_code(msg: &str, err: u32, code: u32) -> u32 {
    eprint(b"azl-inject: ");
    eprint(msg.as_bytes());
    let mut digits = [0u8; 10];
    let mut i = digits.len();
    let mut v = err;
    loop {
        i -= 1;
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    eprint(b" ");
    eprint(&digits[i..]);
    eprint(b"\r\n");
    code
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { ExitProcess(101) }
}

// There is no C runtime: provide the memory routines the compiler emits calls
// to. Volatile accesses keep LLVM from turning the loops back into calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe { dst.add(i).write_volatile(src.add(i).read_volatile()) };
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dst as usize) < (src as usize) {
        return unsafe { memcpy(dst, src, n) };
    }
    for i in (0..n).rev() {
        unsafe { dst.add(i).write_volatile(src.add(i).read_volatile()) };
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe { dst.add(i).write_volatile(c as u8) };
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let (x, y) = unsafe { (a.add(i).read_volatile(), b.add(i).read_volatile()) };
        if x != y {
            return x as i32 - y as i32;
        }
    }
    0
}

// `rsbegin.o` of the windows-gnu target registers unwind tables through libgcc;
// with `panic = "abort"` and no libgcc these are no-ops.
#[unsafe(no_mangle)]
pub extern "C" fn __register_frame_info(_: *const u8, _: *mut u8) {}

#[unsafe(no_mangle)]
pub extern "C" fn __deregister_frame_info(_: *const u8) -> *mut u8 {
    null_mut()
}
