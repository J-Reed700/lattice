#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use super::*;
use std::time::Duration;

/// Set on a child copy of this test binary to pick which role test does anything.
const ROLE: &str = "LATTICE_SIDECAR_CHILD_TEST_ROLE";
const TALKER: &str = "role_talker";
const SLEEPER: &str = "role_sleeper";
const TALKER_EXIT: i32 = 3;

fn in_role(role: &str) -> bool {
    std::env::var(ROLE).is_ok_and(|value| value == role)
}

fn role(role: &str) -> BoundCommand {
    BoundCommand::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("features::llm::engine::sidecar_manager::child::tests::{role}"),
            "--nocapture",
        ])
        .env(ROLE, role)
}

#[test]
fn role_talker() {
    if in_role(TALKER) {
        use std::io::Write;
        let mut err = std::io::stderr();
        err.write_all(b"first\nsecond\r").unwrap();
        err.flush().unwrap();
        err.write_all(b"last").unwrap();
        err.flush().unwrap();
        std::process::exit(TALKER_EXIT);
    }
}

#[test]
fn role_sleeper() {
    if in_role(SLEEPER) {
        std::thread::sleep(Duration::from_secs(300));
    }
}

async fn every_event(mut rx: Receiver<ServerEvent>) -> Vec<ServerEvent> {
    let mut events = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .expect("the sidecar's events stopped")
    {
        events.push(event);
    }
    events
}

#[tokio::test]
async fn output_arrives_line_by_line_and_the_exit_comes_last() {
    let (rx, _process) = start(role(TALKER)).await.unwrap();

    let events = every_event(rx).await;

    let stderr: Vec<&[u8]> = events
        .iter()
        .filter_map(|event| match event {
            ServerEvent::Stderr(line) => Some(line.as_slice()),
            _ => None,
        })
        .collect();
    assert_eq!(stderr, [&b"first\n"[..], b"second\r", b"last"]);
    match events.last() {
        Some(ServerEvent::Terminated(report)) => assert_eq!(report.code, Some(TALKER_EXIT)),
        other => panic!("the last event was {other:?}"),
    }
}

#[tokio::test]
async fn a_kill_requested_at_once_ends_the_process_and_is_reported() {
    let (rx, process) = start(role(SLEEPER)).await.unwrap();

    process.kill();
    process.kill();

    let events = every_event(rx).await;
    match events.last() {
        Some(ServerEvent::Terminated(report)) => assert_ne!(report.code, Some(0)),
        other => panic!("the last event was {other:?}"),
    }
}

#[tokio::test]
async fn a_line_ends_at_a_newline_else_a_carriage_return_else_the_end() {
    let mut reader = BufReader::with_capacity(3, &b"ab\ncd\ref\r\nlast"[..]);
    let mut lines = Vec::new();
    loop {
        let mut line = Vec::new();
        if read_line(&mut reader, &mut line).await.unwrap() == 0 {
            break;
        }
        lines.push(line);
    }
    assert_eq!(
        lines,
        [&b"ab\n"[..], b"cd\r", b"ef\r", b"\n", b"last"].map(<[u8]>::to_vec)
    );
}
