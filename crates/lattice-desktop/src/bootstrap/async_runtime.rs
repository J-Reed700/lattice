//! The tokio runtime every task of the app runs on.

use std::sync::OnceLock;

/// Each worker and blocking thread's stack: the `RUST_MIN_STACK` every test
/// runs with (`.cargo/config.toml`), which a packaged app never sees.
pub const THREAD_STACK_BYTES: usize = 32 * 1024 * 1024;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Hands Tauri a runtime whose threads have [`THREAD_STACK_BYTES`]. Call it
/// before anything spawns a task; Tauri accepts one runtime.
pub fn install() -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("tokio-rt-worker")
        .thread_stack_size(THREAD_STACK_BYTES)
        .build()?;
    let runtime = RUNTIME.get_or_init(|| runtime);
    tauri::async_runtime::set(runtime.handle().clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const PROBE: &str = "LATTICE_STACK_PROBE";
    const NAME: &str =
        "bootstrap::async_runtime::tests::a_task_on_the_app_runtime_has_the_stack_the_tests_run_with";
    /// Four times tokio's own 2 MiB default.
    const PROBE_BYTES: usize = 8 * 1024 * 1024;
    const FRAME_BYTES: usize = 32 * 1024;

    /// Goes `remaining` bytes down the stack and returns how many frames it took.
    #[inline(never)]
    fn use_stack(remaining: usize) -> usize {
        let frame = std::hint::black_box([1u8; FRAME_BYTES]);
        if remaining <= frame.len() {
            return usize::from(frame[0]);
        }
        use_stack(remaining - frame.len())
            + usize::from(std::hint::black_box(frame[frame.len() - 1]))
    }

    #[test]
    fn a_task_on_the_app_runtime_has_the_stack_the_tests_run_with() {
        if std::env::var_os(PROBE).is_some() {
            install().expect("the app's runtime");
            let task = tauri::async_runtime::spawn(async { use_stack(PROBE_BYTES) });
            let frames = tauri::async_runtime::block_on(task).expect("the probe task");
            assert_eq!(frames, PROBE_BYTES / FRAME_BYTES);
            return;
        }
        // A packaged app never sees cargo's RUST_MIN_STACK, so the probe runs without it, in a child copy of this test.
        let status = Command::new(std::env::current_exe().expect("this test binary"))
            .args(["--exact", NAME, "--nocapture"])
            .env(PROBE, "1")
            .env_remove("RUST_MIN_STACK")
            .status()
            .expect("the probe");
        assert!(
            status.success(),
            "a task needing {PROBE_BYTES} bytes of stack overflowed the app's runtime: {status}"
        );
    }

    #[test]
    fn the_app_threads_get_at_least_the_stack_the_tests_run_with() {
        let config = include_str!("../../.cargo/config.toml");
        let tests: usize = config
            .lines()
            .find_map(|line| line.trim().strip_prefix("RUST_MIN_STACK = "))
            .map(|value| value.trim_matches('"').parse().expect("a byte count"))
            .expect("RUST_MIN_STACK in .cargo/config.toml");
        assert!(
            THREAD_STACK_BYTES >= tests,
            "the app's threads get {THREAD_STACK_BYTES} bytes and its tests {tests}"
        );
    }
}
