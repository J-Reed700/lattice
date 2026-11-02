//! The tokio runtime every task of the app runs on.

/// Hands Tauri the runtime its tasks run on. Call it before anything spawns a task.
pub fn install() -> std::io::Result<()> {
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
}
