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
    fn holding(&self) {
        self.0.lock().push("holding");
    }
    fn released(&self) {
        self.0.lock().push("released");
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
async fn a_request_that_waits_for_a_slot_reports_queued_holding_and_granted_and_then_released() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let blocker = busy(&scheduler).await;
    let waiting = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(observing(recorder.clone(), async move {
            scheduler.admit(background(), None).await
        }))
    };
    settle().await;
    assert_eq!(
        recorder.events(),
        ["queued"],
        "a request that had to wait for a slot must report queued, and nothing more while it waits"
    );
    drop(blocker);
    let permit = tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        recorder.events(),
        ["queued", "holding", "granted"],
        "a queued request must report that it holds the slot, then granted, once a slot frees"
    );
    drop(permit);
    assert_eq!(
        recorder.events(),
        ["queued", "holding", "granted", "released"],
        "dropping the permit must report the slot released"
    );
}

#[tokio::test]
async fn a_request_granted_at_once_reports_holding_and_then_released() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let permit = observing(recorder.clone(), scheduler.admit(background(), None))
        .await
        .unwrap();
    assert_eq!(
        recorder.events(),
        ["holding"],
        "a request granted at once was never queued, so it must report only that it holds a slot"
    );
    drop(permit);
    assert_eq!(
        recorder.events(),
        ["holding", "released"],
        "dropping the permit must report the slot released"
    );
}

#[tokio::test]
async fn a_request_outside_any_observer_scope_reports_nothing() {
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
            scheduler.admit(background(), None).await
        }))
    };
    settle().await;
    drop(blocker);
    waiting.await.unwrap().unwrap();
    let permit = observed.await.unwrap().unwrap();
    assert_eq!(
        recorder.events(),
        ["queued", "holding", "granted"],
        "only the request made inside the scope reports, once"
    );
    drop(permit);
}

#[tokio::test]
async fn two_requests_of_one_scope_on_one_slot_hand_it_over_in_order() {
    let scheduler = single_slot();
    let recorder = Arc::new(Recorder::default());
    let first = observing(recorder.clone(), scheduler.admit(background(), None))
        .await
        .unwrap();
    let second = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(observing(recorder.clone(), async move {
            scheduler.admit(background(), None).await
        }))
    };
    settle().await;
    assert_eq!(
        recorder.events(),
        ["holding", "queued"],
        "the first request holds the slot and the second queues behind it"
    );
    drop(first);
    let second = tokio::time::timeout(Duration::from_secs(1), second)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        recorder.events(),
        ["holding", "queued", "released", "holding", "granted"],
        "the first slot is released before the second request holds it and is granted"
    );
    drop(second);
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
