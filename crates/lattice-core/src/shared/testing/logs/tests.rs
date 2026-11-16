use super::*;

#[test]
fn a_thread_reads_its_own_line_though_another_thread_first_wrote_it() {
    fn write() {
        tracing::warn!("a line two threads write");
    }
    let logs = capture_logs(tracing::Level::WARN);
    std::thread::spawn(write).join().unwrap();
    write();
    assert_eq!(logs.text().matches("a line two threads write").count(), 1);
}

#[test]
fn the_process_log_gets_a_line_from_another_thread_in_the_apps_format() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    let _log = write_process_log(&path).unwrap();
    std::thread::spawn(|| tracing::info!("a line for the process log"))
        .join()
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let line = text
        .lines()
        .find(|line| line.contains("a line for the process log"))
        .unwrap();
    assert!(line.contains(" INFO "), "{line}");
    assert!(
        line.contains("lattice_core::shared::testing::logs::tests: "),
        "{line}"
    );
    assert!(line.contains("tests.rs:"), "{line}");
}
