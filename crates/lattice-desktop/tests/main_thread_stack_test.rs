//! Checks the link arguments that size the main thread, and that the app
//! binary carries the result. The module is compiled a second time here
//! because Cargo never runs tests inside `build.rs`.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

#[allow(dead_code)]
#[path = "../build_support/main_thread_stack.rs"]
mod main_thread_stack;

#[cfg(any(windows, target_os = "macos"))]
use main_thread_stack::MAIN_THREAD_STACK_BYTES;

#[cfg(any(windows, target_os = "macos"))]
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    let raw: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap();
    u32::from_le_bytes(raw)
}

#[cfg(any(windows, target_os = "macos"))]
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let raw: [u8; 8] = bytes[offset..offset + 8].try_into().unwrap();
    u64::from_le_bytes(raw)
}

#[cfg(windows)]
#[test]
fn the_app_reserves_its_main_thread_the_stack_its_commands_need() {
    let bytes = std::fs::read(env!("CARGO_BIN_EXE_lattice-desktop")).unwrap();
    let pe = u32_at(&bytes, 0x3C) as usize;
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "not a PE file");
    let optional = pe + 24;
    let magic = u16::from_le_bytes([bytes[optional], bytes[optional + 1]]);
    assert_eq!(magic, 0x20B, "not a PE32+ optional header");
    let reserve = u64_at(&bytes, optional + 72);
    assert!(
        reserve >= MAIN_THREAD_STACK_BYTES,
        "SizeOfStackReserve is {reserve} bytes, the app needs at least {MAIN_THREAD_STACK_BYTES}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_app_reserves_its_main_thread_the_stack_its_commands_need() {
    const LC_MAIN: u32 = 0x8000_0028;
    let bytes = std::fs::read(env!("CARGO_BIN_EXE_lattice-desktop")).unwrap();
    let magic = u32_at(&bytes, 0);
    assert_eq!(magic, 0xFEED_FACF, "not a thin 64-bit little-endian Mach-O");
    let ncmds = u32_at(&bytes, 16);
    let mut offset = 32usize;
    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let size = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_MAIN {
            let stacksize = u64_at(&bytes, offset + 16);
            assert!(
                stacksize >= MAIN_THREAD_STACK_BYTES,
                "LC_MAIN stacksize is {stacksize} bytes, the app needs at least {MAIN_THREAD_STACK_BYTES}"
            );
            return;
        }
        offset += size;
    }
    panic!("no LC_MAIN load command in the app binary");
}
