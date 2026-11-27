use super::{lock, panic_text, JobRuntime};

impl JobRuntime {
    /// Whether the runtime holds `id`'s last failure: only while it is
    /// deferred.
    pub(in super::super) fn remembers_failure(&self, id: &str) -> bool {
        lock(&self.deferred).contains_key(id)
    }
}

#[test]
fn panic_text_extracts_str_and_string_payloads() {
    let static_str: Box<dyn std::any::Any + Send> = Box::new("boom");
    assert_eq!(panic_text(static_str.as_ref()), "boom");

    let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("owned boom"));
    assert_eq!(panic_text(owned.as_ref()), "owned boom");

    let number: Box<dyn std::any::Any + Send> = Box::new(42usize);
    assert_eq!(panic_text(number.as_ref()), "a non-text panic payload");
}

#[tokio::test]
async fn release_cancel_refcounts_nested_cancellations_until_final_release() {
    let runtime = JobRuntime::new(crate::shared::persistence::Database::new(
        crate::shared::testing::sqlite::migrated_in_memory()
            .await
            .expect("in-memory db"),
    ));
    runtime.begin_cancel("job-1");
    runtime.begin_cancel("job-1");
    assert!(!runtime.release_cancel("job-1"));
    assert!(runtime.release_cancel("job-1"));
    assert!(runtime.release_cancel("job-1"));
}
