//! A cloud provider's scheduler admits by what the provider's answers allow,
//! not by a number of ours.

#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;

fn ask(priority: InferencePriority) -> AdmissionRequest {
    AdmissionRequest {
        priority,
        tokens: 1,
        cache_key: None,
        rank: None,
    }
}

fn adaptive() -> Arc<InferenceScheduler> {
    Arc::new(InferenceScheduler::new("test", BackendCapacity::adaptive()))
}

/// Teach the window `slots` acceptances' worth of room, as if the provider
/// had taken that many requests at once.
fn grown_to(scheduler: &InferenceScheduler, slots: usize) {
    let mut state = scheduler.state.lock();
    if let Some(window) = state.congestion.as_mut() {
        while window.limit() < slots {
            window.accepted(true);
        }
    }
    scheduler.follow_window(&mut state);
}

async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

fn queue_one(
    scheduler: &Arc<InferenceScheduler>,
    priority: InferencePriority,
) -> tokio::task::JoinHandle<Permit> {
    let scheduler = Arc::clone(scheduler);
    tokio::spawn(async move { scheduler.admit(ask(priority), None).await.unwrap() })
}

#[test]
fn a_cloud_scheduler_runs_one_request_until_the_provider_has_answered() {
    let schedulers = SchedulerRegistry::default();
    assert_eq!(schedulers.cloud("openai").load().slots, 1);
}

#[tokio::test(start_paused = true)]
async fn an_adaptive_scheduler_admits_one_request_at_first() {
    let scheduler = adaptive();
    let _first = scheduler
        .admit(ask(InferencePriority::Background), None)
        .await
        .unwrap();
    let second = tokio::time::timeout(
        Duration::from_millis(50),
        scheduler.admit(ask(InferencePriority::Background), None),
    )
    .await;
    assert!(second.is_err(), "a second request waits for the provider");
}

#[tokio::test(start_paused = true)]
async fn an_acceptance_while_a_request_waits_grows_the_window_and_admits_it() {
    let scheduler = adaptive();
    let _first = scheduler
        .admit(ask(InferencePriority::Interactive), None)
        .await
        .unwrap();
    let waiting = queue_one(&scheduler, InferencePriority::Interactive);
    settle().await;
    assert_eq!(scheduler.queued(), 1);

    scheduler.accepted();
    let _second = waiting.await.unwrap();
    let load = scheduler.load();
    assert_eq!((load.slots, load.running, load.queued), (2, 2, 0));
}

#[tokio::test(start_paused = true)]
async fn an_acceptance_while_every_slot_is_in_use_grows_the_window() {
    let scheduler = adaptive();
    let _first = scheduler
        .admit(ask(InferencePriority::Interactive), None)
        .await
        .unwrap();
    scheduler.accepted();
    assert_eq!(scheduler.load().slots, 2);
}

#[tokio::test(start_paused = true)]
async fn acceptances_without_demand_do_not_grow_the_window() {
    let scheduler = adaptive();
    for _ in 0..10 {
        scheduler.accepted();
    }
    assert_eq!(scheduler.load().slots, 1);
    let permit = scheduler
        .admit(ask(InferencePriority::Interactive), None)
        .await
        .unwrap();
    drop(permit);
    for _ in 0..10 {
        scheduler.accepted();
    }
    assert_eq!(scheduler.load().slots, 1);
}

#[tokio::test(start_paused = true)]
async fn a_throttle_closes_admission_until_the_provider_says_to_come_back() {
    let scheduler = adaptive();
    let delay = scheduler.throttled(Some(Duration::from_secs(10)));
    assert_eq!(delay, Duration::from_secs(10));
    let waiting = queue_one(&scheduler, InferencePriority::Interactive);
    settle().await;
    assert_eq!(scheduler.queued(), 1);

    tokio::time::advance(Duration::from_secs(9)).await;
    settle().await;
    assert_eq!(scheduler.queued(), 1, "still closed one second early");

    tokio::time::advance(Duration::from_secs(2)).await;
    let _permit = waiting.await.unwrap();
    assert_eq!(scheduler.queued(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_throttle_that_extends_the_closing_does_not_reopen_at_the_earlier_time() {
    let scheduler = adaptive();
    scheduler.throttled(Some(Duration::from_secs(10)));
    let waiting = queue_one(&scheduler, InferencePriority::Interactive);
    tokio::time::advance(Duration::from_secs(5)).await;
    scheduler.throttled(Some(Duration::from_secs(20)));

    tokio::time::advance(Duration::from_secs(6)).await;
    settle().await;
    assert_eq!(scheduler.queued(), 1, "reopened at the first timer");

    tokio::time::advance(Duration::from_secs(15)).await;
    let _permit = waiting.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_burst_of_throttles_halves_the_window_once() {
    let scheduler = adaptive();
    grown_to(&scheduler, 8);
    for _ in 0..5 {
        scheduler.throttled(Some(Duration::from_secs(1)));
    }
    assert_eq!(scheduler.load().slots, 4);
}

#[tokio::test(start_paused = true)]
async fn a_throttle_without_retry_after_backs_off_by_doubling() {
    let scheduler = adaptive();
    assert_eq!(scheduler.throttled(None), Duration::from_secs(1));
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(scheduler.throttled(None), Duration::from_secs(2));
}

#[tokio::test(start_paused = true)]
async fn until_open_resolves_at_once_when_nothing_was_throttled() {
    let scheduler = adaptive();
    tokio::time::timeout(Duration::ZERO, scheduler.until_open())
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn until_open_waits_for_the_reopening() {
    let scheduler = adaptive();
    scheduler.throttled(Some(Duration::from_secs(10)));
    assert!(
        tokio::time::timeout(Duration::from_secs(9), scheduler.until_open())
            .await
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(2), scheduler.until_open())
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_retired_slot_is_never_granted_again_and_goes_once_its_request_ends() {
    let scheduler = adaptive();
    grown_to(&scheduler, 4);
    let mut running = Vec::new();
    for _ in 0..4 {
        running.push(
            scheduler
                .admit(ask(InferencePriority::Interactive), None)
                .await
                .unwrap(),
        );
    }
    scheduler.throttled(Some(Duration::from_secs(5)));
    let load = scheduler.load();
    assert_eq!(
        (load.slots, load.running),
        (2, 4),
        "a shrink cancels nothing in flight"
    );

    let retired: Vec<Permit> = running.split_off(2);
    drop(retired);
    assert_eq!(scheduler.state.lock().slots.len(), 2);
    tokio::time::advance(Duration::from_secs(6)).await;
    let waiting = queue_one(&scheduler, InferencePriority::Interactive);
    settle().await;
    assert_eq!(scheduler.queued(), 1, "two are still in flight");

    drop(running.pop());
    let permit = waiting.await.unwrap();
    assert!(permit.grant.slot < 2);
}

#[tokio::test(start_paused = true)]
async fn a_request_in_flight_when_the_window_shrinks_still_finishes_and_frees_its_share() {
    let scheduler = adaptive();
    grown_to(&scheduler, 2);
    let first = scheduler
        .admit(ask(InferencePriority::Interactive), None)
        .await
        .unwrap();
    let second = scheduler
        .admit(ask(InferencePriority::Interactive), None)
        .await
        .unwrap();
    scheduler.throttled(Some(Duration::from_secs(1)));
    assert_eq!(scheduler.load().slots, 1);
    drop((first, second));
    assert_eq!(scheduler.load().running, 0);
}

#[tokio::test(start_paused = true)]
async fn a_dropped_scheduler_is_not_kept_alive_by_its_reopening_timer() {
    let scheduler = adaptive();
    scheduler.throttled(Some(Duration::from_secs(10)));
    let weak = Arc::downgrade(&scheduler);
    drop(scheduler);
    assert!(weak.upgrade().is_none());
    tokio::time::advance(Duration::from_secs(11)).await;
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn a_fixed_scheduler_ignores_what_the_provider_answers() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(2),
    ));
    scheduler.accepted();
    assert_eq!(
        scheduler.throttled(Some(Duration::from_secs(60))),
        Duration::ZERO
    );
    scheduler.until_open().await;
    let _first = scheduler
        .admit(ask(InferencePriority::Interactive), None)
        .await
        .unwrap();
    let _second = scheduler
        .admit(ask(InferencePriority::Interactive), None)
        .await
        .unwrap();
    assert_eq!(scheduler.load().slots, 2);
}
