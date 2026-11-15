use super::*;

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn grown_to(target: usize) -> CongestionWindow {
    let mut window = CongestionWindow::new();
    while window.limit() < target {
        window.accepted(true);
    }
    window
}

#[test]
fn a_new_window_allows_one_request_and_is_open() {
    let window = CongestionWindow::new();
    assert_eq!(window.limit(), 1);
    assert!(window.is_open(Instant::now()));
    assert_eq!(window.open_at(), None);
}

#[test]
fn without_a_throttle_each_acceptance_with_demand_grows_the_window_by_one() {
    let mut window = CongestionWindow::new();
    for expected in 2..=10 {
        window.accepted(true);
        assert_eq!(window.limit(), expected);
    }
}

#[test]
fn an_acceptance_without_demand_does_not_grow_the_window() {
    let mut window = grown_to(4);
    for _ in 0..50 {
        window.accepted(false);
    }
    assert_eq!(window.limit(), 4);
}

#[test]
fn a_throttle_halves_the_window_and_sets_the_threshold() {
    let mut window = grown_to(8);
    window.throttled(Instant::now(), Some(secs(1)));
    assert_eq!(window.limit(), 4);
    assert_eq!(window.threshold, Some(4));
}

#[test]
fn a_throttle_never_takes_the_window_below_one() {
    let mut window = CongestionWindow::new();
    let mut now = Instant::now();
    for _ in 0..3 {
        window.throttled(now, Some(secs(1)));
        now += secs(2);
        assert_eq!(window.limit(), 1);
    }
}

#[test]
fn below_the_threshold_the_window_grows_by_one_per_acceptance() {
    let mut window = grown_to(16);
    window.throttled(Instant::now(), Some(secs(1)));
    window.threshold = Some(12);
    assert_eq!(window.limit(), 8);
    for expected in 9..=12 {
        window.accepted(true);
        assert_eq!(window.limit(), expected);
    }
}

#[test]
fn at_the_threshold_the_window_grows_by_one_per_window_of_acceptances() {
    let mut window = grown_to(8);
    window.throttled(Instant::now(), Some(secs(1)));
    assert_eq!(window.limit(), 4);
    for _ in 0..3 {
        window.accepted(true);
        assert_eq!(window.limit(), 4);
    }
    window.accepted(true);
    assert_eq!(window.limit(), 5);
    for _ in 0..4 {
        window.accepted(true);
        assert_eq!(window.limit(), 5);
    }
    window.accepted(true);
    assert_eq!(window.limit(), 6);
}

#[test]
fn acceptances_without_demand_do_not_count_toward_additive_growth() {
    let mut window = grown_to(4);
    window.throttled(Instant::now(), Some(secs(1)));
    assert_eq!(window.limit(), 2);
    window.accepted(true);
    for _ in 0..20 {
        window.accepted(false);
    }
    window.accepted(true);
    assert_eq!(window.limit(), 3);
}

#[test]
fn a_throttle_with_retry_after_closes_admission_until_then() {
    let mut window = CongestionWindow::new();
    let now = Instant::now();
    let reopen = window.throttled(now, Some(secs(45)));
    assert_eq!(reopen, now + secs(45));
    assert_eq!(window.open_at(), Some(reopen));
    assert!(!window.is_open(now + secs(44)));
    assert!(window.is_open(now + secs(45)));
}

#[test]
fn a_retry_after_is_not_capped() {
    let mut window = CongestionWindow::new();
    let now = Instant::now();
    assert_eq!(window.throttled(now, Some(secs(3600))), now + secs(3600));
}

#[test]
fn throttles_without_retry_after_back_off_one_two_four_seconds() {
    let mut window = CongestionWindow::new();
    let mut now = Instant::now();
    for wait in [1, 2, 4, 8] {
        assert_eq!(window.throttled(now, None), now + secs(wait));
        now += secs(wait);
    }
}

#[test]
fn an_acceptance_restarts_the_backoff() {
    let mut window = CongestionWindow::new();
    let mut now = Instant::now();
    window.throttled(now, None);
    now += secs(1);
    window.throttled(now, None);
    now += secs(2);
    window.accepted(false);
    assert_eq!(window.throttled(now, None), now + secs(1));
}

#[test]
fn throttles_arriving_while_closed_halve_the_window_once() {
    let mut window = grown_to(8);
    let now = Instant::now();
    window.throttled(now, Some(secs(10)));
    for _ in 0..5 {
        window.throttled(now + secs(1), Some(secs(10)));
    }
    assert_eq!(window.limit(), 4);
    assert_eq!(window.threshold, Some(4));
}

#[test]
fn a_throttle_while_closed_only_moves_the_reopening_later() {
    let mut window = CongestionWindow::new();
    let now = Instant::now();
    window.throttled(now, Some(secs(10)));
    assert_eq!(
        window.throttled(now + secs(1), Some(secs(30))),
        now + secs(31)
    );
    assert_eq!(window.open_at(), Some(now + secs(31)));
    assert_eq!(
        window.throttled(now + secs(2), Some(secs(1))),
        now + secs(31)
    );
    assert_eq!(window.open_at(), Some(now + secs(31)));
}

#[test]
fn throttles_while_closed_do_not_lengthen_the_backoff() {
    let mut window = CongestionWindow::new();
    let now = Instant::now();
    window.throttled(now, None);
    window.throttled(now, None);
    window.throttled(now, None);
    let after = now + secs(1);
    assert_eq!(window.throttled(after, None), after + secs(2));
}

#[test]
fn a_zero_retry_after_still_waits_the_backoff_refusals_have_earned() {
    let mut window = CongestionWindow::new();
    let now = Instant::now();
    let first = window.throttled(now, Some(Duration::ZERO));
    assert_eq!(first, now + secs(1));
    assert_eq!(
        window.throttled(first, Some(Duration::ZERO)),
        first + secs(2)
    );
}

#[test]
fn a_retry_after_beyond_the_clock_waits_the_backoff_instead_of_none() {
    let mut window = CongestionWindow::new();
    let now = Instant::now();
    assert_eq!(window.throttled(now, Some(Duration::MAX)), now + secs(1));
}

#[test]
fn a_throttle_after_reopening_halves_again() {
    let mut window = grown_to(8);
    let now = Instant::now();
    window.throttled(now, Some(secs(1)));
    window.throttled(now + secs(2), Some(secs(1)));
    assert_eq!(window.limit(), 2);
}
