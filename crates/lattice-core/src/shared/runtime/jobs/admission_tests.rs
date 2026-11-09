#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;
use std::sync::Mutex;

#[derive(Default)]
struct Recorder(Mutex<Vec<&'static str>>);

impl AdmissionObserver for Recorder {
    fn queued(&self) {
        self.0.lock().unwrap().push("queued");
    }
    fn left_queue(&self, exit: QueueExit) {
        self.0.lock().unwrap().push(match exit {
            QueueExit::Granted => "granted",
            QueueExit::Abandoned => "abandoned",
        });
    }
    fn holding(&self) {
        self.0.lock().unwrap().push("holding");
    }
    fn released(&self) {
        self.0.lock().unwrap().push("released");
    }
}

#[tokio::test]
async fn there_is_no_observer_outside_a_scope() {
    assert!(
        current_observer().is_none(),
        "work outside an observer scope must not reach any observer"
    );
}

#[tokio::test]
async fn work_inside_a_scope_reaches_its_observer_through_nested_futures() {
    let recorder = Arc::new(Recorder::default());
    observing(recorder.clone(), async {
        let (nested, sibling) = futures::join!(
            async {
                tokio::task::yield_now().await;
                current_observer()
            },
            async { current_observer() }
        );
        for seen in [nested, sibling] {
            let seen = seen.expect("a call made inside an observer scope must reach its observer");
            seen.queued();
        }
    })
    .await;
    assert_eq!(
        *recorder.0.lock().unwrap(),
        ["queued", "queued"],
        "both nested calls must reach the scope's own observer"
    );
}

#[tokio::test]
async fn a_scope_does_not_leak_to_work_started_outside_it() {
    let recorder = Arc::new(Recorder::default());
    let outside = tokio::spawn(async { current_observer().is_none() });
    observing(recorder, async {
        assert!(current_observer().is_some(), "inside the scope");
    })
    .await;
    assert!(
        outside.await.unwrap(),
        "a task spawned outside has no observer"
    );
}
