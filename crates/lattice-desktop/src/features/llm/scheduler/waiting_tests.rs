//! A job whose requests queue is told so, once for each wait and however the
//! wait ends.

#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;
use lattice_core::shared::runtime::jobs::{observing, AdmissionObserver, QueueExit};
use parking_lot::Mutex as SyncMutex;
use std::time::Duration;

#[derive(Default)]
struct Recorder(SyncMutex<Vec<&'static str>>);

impl Recorder {
    fn events(&self) -> Vec<&'static str> {
        self.0.lock().clone()
    }
}

impl AdmissionObserver for Recorder {
    fn queued(&self) {
        self.0.lock().push("queued");
    }
    fn left_queue(&self, exit: QueueExit) {
        self.0.lock().push(match exit {
            QueueExit::Granted => "granted",
            QueueExit::Abandoned => "abandoned",
        });
    }
}

fn background() -> AdmissionRequest {
    AdmissionRequest {
        priority: InferencePriority::Background,
        tokens: 1,
        cache_key: None,
        rank: None,
    }
}

fn single_slot() -> Arc<InferenceScheduler> {
    Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(1),
    ))
}

async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

async fn busy(scheduler: &Arc<InferenceScheduler>) -> Permit {
    scheduler
        .admit(
            AdmissionRequest {
                priority: InferencePriority::Interactive,
                ..background()
            },
            None,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn a_request_that_waits_for_a_slot_reports_queued_and_then_granted() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let blocker = busy(&scheduler).await;
    let waiting = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(observing(recorder.clone(), async move {
            scheduler.admit(background(), None).await.map(|_| ())
        }))
    };
    settle().await;
    assert_eq!(
        recorder.events(),
        ["queued"],
        "a request that had to wait for a slot must report queued, and nothing more while it waits"
    );
    drop(blocker);
    tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        recorder.events(),
        ["queued", "granted"],
        "a queued request must report granted once a slot frees"
    );
}

#[tokio::test]
async fn a_request_granted_at_once_reports_nothing() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let permit = observing(recorder.clone(), scheduler.admit(background(), None))
        .await
        .unwrap();
    assert!(
        recorder.events().is_empty(),
        "a request granted at once was never queued, so it must report nothing: {:?}",
        recorder.events()
    );
    drop(permit);
}

#[tokio::test]
async fn a_request_outside_any_observer_scope_queues_without_reporting() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let blocker = busy(&scheduler).await;
    let waiting = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(async move { scheduler.admit(background(), None).await.map(|_| ()) })
    };
    let observed = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(observing(recorder.clone(), async move {
            scheduler.admit(background(), None).await.map(|_| ())
        }))
    };
    settle().await;
    drop(blocker);
    waiting.await.unwrap().unwrap();
    observed.await.unwrap().unwrap();
    assert_eq!(
        recorder.events(),
        ["queued", "granted"],
        "only the request made inside the scope reports, once"
    );
}

#[tokio::test]
async fn a_request_cancelled_while_queued_leaves_nothing_queued() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let _blocker = busy(&scheduler).await;
    let cancel = CancellationToken::new();
    let waiting = {
        let scheduler = Arc::clone(&scheduler);
        let cancel = cancel.clone();
        tokio::spawn(observing(recorder.clone(), async move {
            scheduler
                .admit(background(), Some(&cancel))
                .await
                .map(|_| ())
        }))
    };
    settle().await;
    assert_eq!(recorder.events(), ["queued"]);
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.is_err(),
        "the cancelled request leaves with an error"
    );
    assert_eq!(
        recorder.events(),
        ["queued", "abandoned"],
        "a request cancelled while queued must clear its queued state"
    );
}

#[tokio::test]
async fn a_request_whose_future_is_dropped_while_queued_leaves_nothing_queued() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let _blocker = busy(&scheduler).await;
    let waiting = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(observing(recorder.clone(), async move {
            scheduler.admit(background(), None).await.map(|_| ())
        }))
    };
    settle().await;
    assert_eq!(recorder.events(), ["queued"]);
    waiting.abort();
    let _ = waiting.await;
    assert_eq!(
        recorder.events(),
        ["queued", "abandoned"],
        "a request dropped while queued must clear its queued state"
    );
}
