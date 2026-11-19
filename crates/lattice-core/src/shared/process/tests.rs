#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use super::BoundCommand;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// Set on a child copy of this test binary to pick which role test does anything.
const ROLE: &str = "LATTICE_PROCESS_TEST_ROLE";
/// Where a parent role writes the pids it started.
const PIDS: &str = "LATTICE_PROCESS_TEST_PIDS";
const SLEEPER: &str = "role_sleeper";
const PARENT_PAIR: &str = "role_parent_pair";
const PARENT_THREAD: &str = "role_parent_thread";
const ECHO: &str = "role_echo";
const EXIT: &str = "role_exit";
const ECHO_LINE: &str = "bound-child-says-hello";
const EXIT_CODE: i32 = 7;
const LONG: Duration = Duration::from_secs(300);

fn in_role(role: &str) -> bool {
    std::env::var(ROLE).is_ok_and(|value| value == role)
}

fn role_args(role: &str) -> Vec<String> {
    vec![
        "--exact".to_string(),
        format!("shared::process::tests::{role}"),
        "--nocapture".to_string(),
    ]
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Kills what a test started, also when the test fails.
struct Cleanup {
    parent: Option<std::process::Child>,
    pids: Vec<u32>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(parent) = self.parent.as_mut() {
            let _ = parent.kill();
            let _ = parent.wait();
        }
        for pid in &self.pids {
            kill_pid(*pid);
        }
    }
}

fn start_parent(role: &str, pids_file: &Path) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(role_args(role))
        .env(ROLE, role)
        .env(PIDS, pids_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn read_pids(file: &Path, count: usize) -> Option<Vec<u32>> {
    let text = std::fs::read_to_string(file).ok()?;
    let pids: Vec<u32> = text
        .split_whitespace()
        .filter_map(|word| word.parse().ok())
        .collect();
    (pids.len() == count).then_some(pids)
}

fn wait_for_pids(file: &Path, count: usize) -> Vec<u32> {
    let mut found = None;
    assert!(
        wait_until(|| {
            found = read_pids(file, count);
            found.is_some()
        }),
        "the parent role did not report {count} pids"
    );
    found.unwrap()
}

fn report_pids(pids: &[u32]) {
    let file = std::env::var_os(PIDS).unwrap();
    let file = Path::new(&file);
    let text: Vec<String> = pids.iter().map(u32::to_string).collect();
    let staging = file.with_extension("staging");
    std::fs::write(&staging, text.join(" ")).unwrap();
    std::fs::rename(&staging, file).unwrap();
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

async fn bound_sleeper() -> tokio::process::Child {
    BoundCommand::new(std::env::current_exe().unwrap())
        .args(role_args(SLEEPER))
        .env(ROLE, SLEEPER)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .await
        .unwrap()
}

#[test]
fn role_sleeper() {
    if in_role(SLEEPER) {
        std::thread::sleep(LONG);
    }
}

/// Starts a bound and an unbound sleeper, reports both pids, then waits to be killed.
#[test]
fn role_parent_pair() {
    if !in_role(PARENT_PAIR) {
        return;
    }
    let runtime = runtime();
    let (bound, control) = runtime.block_on(async {
        let bound = bound_sleeper().await;
        let control = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(role_args(SLEEPER))
            .env(ROLE, SLEEPER)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        (bound, control)
    });
    report_pids(&[bound.id().unwrap(), control.id().unwrap()]);
    std::thread::sleep(LONG);
}

/// Starts a bound sleeper from a thread that then ends, reports its pid, then waits to be killed.
#[test]
fn role_parent_thread() {
    if !in_role(PARENT_THREAD) {
        return;
    }
    let runtime = runtime();
    let handle = runtime.handle().clone();
    let bound = std::thread::spawn(move || handle.block_on(bound_sleeper()))
        .join()
        .unwrap();
    report_pids(&[bound.id().unwrap()]);
    std::thread::sleep(LONG);
}

#[test]
fn role_echo() {
    if in_role(ECHO) {
        println!("{ECHO_LINE}");
    }
}

#[test]
fn role_exit() {
    if in_role(EXIT) {
        std::process::exit(EXIT_CODE);
    }
}

#[test]
fn a_bound_child_ends_when_its_parent_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let pids_file = dir.path().join("pids");
    let mut cleanup = Cleanup {
        parent: Some(start_parent(PARENT_PAIR, &pids_file)),
        pids: Vec::new(),
    };
    let pids = wait_for_pids(&pids_file, 2);
    let (bound, control) = (pids[0], pids[1]);
    cleanup.pids.extend([bound, control]);
    assert!(alive(bound) && alive(control));

    let parent = cleanup.parent.as_mut().unwrap();
    parent.kill().unwrap();
    parent.wait().unwrap();

    assert!(
        wait_until(|| !alive(bound)),
        "the bound child outlived its parent"
    );
    cleanup.pids.retain(|pid| *pid != bound);
    assert!(
        alive(control),
        "killing the parent also ended the unbound control child"
    );
}

#[test]
fn a_child_bound_from_a_thread_that_ends_lives_while_its_parent_does() {
    let dir = tempfile::tempdir().unwrap();
    let pids_file = dir.path().join("pids");
    let mut cleanup = Cleanup {
        parent: Some(start_parent(PARENT_THREAD, &pids_file)),
        pids: Vec::new(),
    };
    let pids = wait_for_pids(&pids_file, 1);
    cleanup.pids.push(pids[0]);

    std::thread::sleep(Duration::from_secs(1));

    assert!(
        alive(pids[0]),
        "the bound child ended with the thread that started it"
    );
}

#[tokio::test]
async fn a_bound_child_runs_and_reports() {
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        BoundCommand::new(std::env::current_exe().unwrap())
            .args(role_args(ECHO))
            .env(ROLE, ECHO)
            .output(),
    )
    .await
    .expect("the bound child did not finish")
    .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(ECHO_LINE));
}

#[tokio::test]
async fn the_exit_of_a_bound_child_is_seen() {
    let mut child = BoundCommand::new(std::env::current_exe().unwrap())
        .args(role_args(EXIT))
        .env(ROLE, EXIT)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .await
        .unwrap();

    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .expect("the exit of the bound child was not seen")
        .unwrap();

    assert_eq!(status.code(), Some(EXIT_CODE));
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    let exists = unsafe { libc::kill(pid as libc::pid_t, 0) == 0 };
    exists && !is_zombie(pid)
}

/// A killed orphan stays a zombie until init reaps it, which a container may never do.
#[cfg(target_os = "linux")]
fn is_zombie(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit(')')
                .next()
                .and_then(|rest| rest.trim_start().chars().next())
        })
        == Some('Z')
}

#[cfg(target_os = "macos")]
fn is_zombie(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    // SAFETY: sending SIGKILL to a pid this test started.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: the handle is opened, queried and closed here.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut code = 0u32;
        let queried = GetExitCodeProcess(process, &mut code).is_ok();
        let _ = CloseHandle(process);
        queried && code == STILL_ACTIVE.0 as u32
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    // SAFETY: the handle is opened, used and closed here.
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(process, 1);
            let _ = CloseHandle(process);
        }
    }
}
