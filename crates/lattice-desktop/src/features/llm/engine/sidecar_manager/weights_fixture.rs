//! Model files for tests that price weights by file length.
//!
//! `set_len` leaves a hole on macOS and Linux, but NTFS reserves every byte
//! unless the file is marked sparse first. The tests that price 16 GiB models
//! side by side filled the Windows CI runner's disk.

use std::path::Path;

/// Creates `path` with a length of `bytes` that takes no disk space.
pub(crate) fn sparse_file(path: &Path, bytes: u64) {
    let file = std::fs::File::create(path).unwrap();
    #[cfg(windows)]
    mark_sparse(&file);
    file.set_len(bytes).unwrap();
}

#[cfg(windows)]
fn mark_sparse(file: &std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    use windows::Win32::System::IO::DeviceIoControl;

    let handle = HANDLE(file.as_raw_handle() as isize);
    // Without an OVERLAPPED the call writes the byte count even when there
    // is no output, so it needs somewhere to put it.
    let mut returned = 0u32;
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_SPARSE,
            None,
            0,
            None,
            0,
            Some(&mut returned),
            None,
        )
    }
    .expect("mark the fixture file sparse");
}
