#![cfg(windows)]
//! Diagnosis for the Windows CI flake where `NamedTempFile::new()` fails with
//! "Access is denied" in RUNNER_TEMP. It churns temp files the way tempfile
//! makes them and, for each failure, records the NT status behind the error,
//! what the path holds, and whether the same name and a fresh one work after.
//! It always fails, so nextest prints its report.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x100;
const UNDER_NEXTEST_TIMEOUT: Duration = Duration::from_secs(45);

#[link(name = "ntdll", kind = "raw-dylib")]
extern "system" {
    fn RtlGetLastNtStatus() -> i32;
}

fn last_nt_status() -> String {
    let status = unsafe { RtlGetLastNtStatus() } as u32;
    let name = match status {
        0xC000_0022 => "ACCESS_DENIED",
        0xC000_0056 => "DELETE_PENDING",
        0xC000_0043 => "SHARING_VIOLATION",
        0xC000_0035 => "OBJECT_NAME_COLLISION",
        0xC000_0121 => "CANNOT_DELETE",
        0xC000_00BA => "FILE_IS_A_DIRECTORY",
        _ => "",
    };
    format!("{status:#010x} {name}")
}

fn create_like_tempfile(path: &Path) -> io::Result<fs::File> {
    OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .custom_flags(FILE_ATTRIBUTE_TEMPORARY)
        .open(path)
}

fn what_is_at(path: &Path) -> String {
    match fs::symlink_metadata(path) {
        Ok(found) => format!(
            "present (directory {}, {} bytes, attributes {:#x})",
            found.is_dir(),
            found.len(),
            found.file_attributes()
        ),
        Err(error) => {
            let status = last_nt_status();
            format!(
                "stat {:?} os {:?} nt {status}",
                error.kind(),
                error.raw_os_error()
            )
        }
    }
}

fn report(path: &Path, error: &io::Error, status: String, took: Duration) -> String {
    let now = what_is_at(path);
    let mut same_name = Vec::new();
    for wait_ms in [0u64, 10, 100, 1000] {
        std::thread::sleep(Duration::from_millis(wait_ms));
        match create_like_tempfile(path) {
            Ok(file) => {
                drop(file);
                let _ = fs::remove_file(path);
                same_name.push(format!("+{wait_ms} ms created"));
                break;
            }
            Err(retry) => {
                let retry_status = last_nt_status();
                same_name.push(format!(
                    "+{wait_ms} ms {:?} os {:?} nt {retry_status}",
                    retry.kind(),
                    retry.raw_os_error()
                ));
            }
        }
    }
    let fresh = match tempfile::NamedTempFile::new() {
        Ok(_) => "created".to_string(),
        Err(fresh_error) => fresh_error.to_string(),
    };
    format!(
        "{path:?}: {:?} os {:?} nt {status} after {took:?}\n  path now: {now}\n  same name: {}\n  fresh name: {fresh}",
        error.kind(),
        error.raw_os_error(),
        same_name.join("; ")
    )
}

#[test]
fn tempfile_churn_reports_every_access_denied() {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let failures = Mutex::new(Vec::new());
    let made = AtomicUsize::new(0);
    let start = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while start.elapsed() < UNDER_NEXTEST_TIMEOUT {
                    let began = Instant::now();
                    let file = tempfile::Builder::new().make(|path| {
                        create_like_tempfile(path).map_err(|error| {
                            let status = last_nt_status();
                            let took = began.elapsed();
                            failures
                                .lock()
                                .unwrap()
                                .push(report(path, &error, status, took));
                            error
                        })
                    });
                    if let Ok(mut file) = file {
                        file.write_all(b"diagnosis").unwrap();
                        fs::metadata(file.path()).unwrap();
                        made.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    panic!(
        "{} temp files made in {:?} on {threads} threads in {:?}; {} failed\n{}",
        made.load(Ordering::Relaxed),
        start.elapsed(),
        std::env::temp_dir(),
        failures.len(),
        failures.join("\n")
    );
}
