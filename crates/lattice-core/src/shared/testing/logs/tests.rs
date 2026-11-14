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
