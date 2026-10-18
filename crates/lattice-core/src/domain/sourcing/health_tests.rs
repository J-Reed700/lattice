//! The breaker's transitions, its half-open probe, the `open_for`
//! derivation, the caller's wait, and the quota window, as tables.

#![cfg(test)]

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};

use super::*;
use crate::domain::sourcing::descriptor::Quota;

const SECOND: Duration = Duration::from_secs(1);
const HOUR: Duration = Duration::from_secs(60 * 60);

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 18, 14, 0, 0)
        .single()
        .unwrap()
}

fn at(offset: Duration) -> DateTime<Utc> {
    t0() + TimeDelta::from_std(offset).unwrap()
}

fn rate(min_interval: Duration) -> RatePolicy {
    RatePolicy {
        min_interval,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://example.com/policy",
    }
}

fn status(code: u16) -> FinderResult {
    FinderResult::Failed(FailureCause::Status {
        code,
        retry_after: None,
    })
}

fn status_after(code: u16, wait: Duration) -> FinderResult {
    FinderResult::Failed(FailureCause::Status {
        code,
        retry_after: Some(wait),
    })
}

fn transport() -> FinderResult {
    FinderResult::Failed(FailureCause::Transport("connection reset".into()))
}

fn closed(refusals: u32, transport: u32) -> BreakerState {
    BreakerState::Closed {
        refusals,
        transport,
    }
}

/// The state after recording `events` at t0, or `Open` with the window end.
#[derive(Debug, PartialEq)]
enum Expect {
    Closed(u32, u32),
    OpenUntil(Duration),
}

#[test]
fn transitions() {
    let policy = SourcingPolicy::default();
    let min = 2 * SECOND;
    let mut honours_not = rate(min);
    honours_not.honours_retry_after = false;
    let mut archive = rate(min);
    archive.trip_on_first_refusal = true;

    let challenge = FinderResult::Failed(FailureCause::Challenge);
    let parse = FinderResult::Failed(FailureCause::Parse("no results block".into()));
    let rate_limited = FinderResult::Failed(FailureCause::RateLimited { until: at(HOUR) });

    // First derived opening: min_interval × 2^1.
    let first_open = Expect::OpenUntil(min * 2);
    let cases: Vec<(&str, RatePolicy, Vec<FinderResult>, Expect)> = vec![
        (
            "one 429 stays closed",
            rate(min),
            vec![status(429)],
            Expect::Closed(1, 0),
        ),
        (
            "two 429s open",
            rate(min),
            vec![status(429), status(429)],
            first_open,
        ),
        (
            "a challenge then a 403 open",
            rate(min),
            vec![challenge.clone(), status(403)],
            Expect::OpenUntil(min * 2),
        ),
        (
            "a success breaks the refusal streak",
            rate(min),
            vec![status(429), FinderResult::Answered, status(429)],
            Expect::Closed(1, 0),
        ),
        (
            "an empty answer is a success",
            rate(min),
            vec![status(429), FinderResult::Empty, status(429)],
            Expect::Closed(1, 0),
        ),
        (
            "two transport failures stay closed",
            rate(min),
            vec![transport(), transport()],
            Expect::Closed(0, 2),
        ),
        (
            "three transport failures open",
            rate(min),
            vec![transport(), transport(), transport()],
            Expect::OpenUntil(min * 2),
        ),
        (
            "a 5xx is a transport failure",
            rate(min),
            vec![status(502), status(503), status(500)],
            Expect::OpenUntil(min * 2),
        ),
        (
            "a refusal resets the transport streak",
            rate(min),
            vec![transport(), transport(), status(429), transport()],
            Expect::Closed(0, 1),
        ),
        (
            "a parse failure neither counts nor resets",
            rate(min),
            vec![status(429), parse.clone(), status(429)],
            Expect::OpenUntil(min * 2),
        ),
        (
            "404s are neutral",
            rate(min),
            vec![status(404), status(404), status(404)],
            Expect::Closed(0, 0),
        ),
        (
            "the caller's budget is neutral",
            rate(min),
            vec![FinderResult::Failed(FailureCause::Budget); 3],
            Expect::Closed(0, 0),
        ),
        (
            "Retry-After opens at once for the host's wait",
            rate(min),
            vec![status_after(429, 60 * SECOND)],
            Expect::OpenUntil(60 * SECOND),
        ),
        (
            "a rate-limit reset opens until the host's time",
            rate(min),
            vec![rate_limited],
            Expect::OpenUntil(HOUR),
        ),
        (
            "Retry-After from a host that does not honour it is a refusal",
            honours_not,
            vec![status_after(429, 60 * SECOND)],
            Expect::Closed(1, 0),
        ),
        (
            "a first-refusal host opens on one 429",
            archive,
            vec![status(429)],
            Expect::OpenUntil(min * 2),
        ),
        (
            "skips change nothing",
            rate(min),
            vec![status(429), FinderResult::Skipped(SkipReason::OutOfBudget)],
            Expect::Closed(1, 0),
        ),
    ];

    for (case, rate, events, expected) in cases {
        let mut breaker = Breaker::new();
        for event in &events {
            breaker.record(event, t0(), &rate, &policy);
        }
        let actual = match breaker.state() {
            BreakerState::Closed {
                refusals,
                transport,
            } => Expect::Closed(*refusals, *transport),
            BreakerState::Open { until, .. } => {
                Expect::OpenUntil((*until - t0()).to_std().unwrap())
            }
            other => panic!("{case}: unexpected {other:?}"),
        };
        assert_eq!(actual, expected, "{case}");
    }
}

#[test]
fn half_open_admits_one_probe_and_its_result_decides() {
    let policy = SourcingPolicy::default();
    let rate = rate(2 * SECOND);
    let mut breaker = Breaker::new();

    breaker.record(&status(429), t0(), &rate, &policy);
    breaker.record(&status(429), t0(), &rate, &policy);
    assert_eq!(breaker.trips(), 1);
    assert_eq!(breaker.next_allowed_at(), Some(at(4 * SECOND)));

    let refused = breaker.admit(at(SECOND), &rate);
    assert_eq!(
        refused,
        Admission::Refuse(SkipReason::CircuitOpen {
            until: at(4 * SECOND),
            last: Box::new(FailureCause::Status {
                code: 429,
                retry_after: None
            }),
        })
    );

    assert_eq!(breaker.admit(at(4 * SECOND), &rate), Admission::Probe);
    assert!(matches!(
        breaker.admit(at(4 * SECOND), &rate),
        Admission::Refuse(SkipReason::CircuitOpen { .. })
    ));

    // A failed probe reopens with the next doubling: 2 s × 2^2 = 8 s.
    breaker.record(&status(429), at(5 * SECOND), &rate, &policy);
    assert_eq!(breaker.trips(), 2);
    assert_eq!(breaker.next_allowed_at(), Some(at(13 * SECOND)));

    // A neutral probe result lets the next call probe again.
    assert_eq!(breaker.admit(at(13 * SECOND), &rate), Admission::Probe);
    breaker.record(&status(404), at(13 * SECOND), &rate, &policy);
    assert_eq!(breaker.admit(at(13 * SECOND), &rate), Admission::Probe);

    // A successful probe closes it and forgets the trips.
    breaker.record(&FinderResult::Answered, at(14 * SECOND), &rate, &policy);
    assert_eq!(breaker.state(), &closed(0, 0));
    assert_eq!(breaker.trips(), 0);
    assert_eq!(breaker.admit(at(14 * SECOND), &rate), Admission::Go);
}

#[test]
fn a_later_host_instruction_extends_an_open_window() {
    let policy = SourcingPolicy::default();
    let rate = rate(2 * SECOND);
    let mut breaker = Breaker::new();
    breaker.record(&status_after(429, 10 * SECOND), t0(), &rate, &policy);
    breaker.record(&status_after(429, 60 * SECOND), t0(), &rate, &policy);
    assert_eq!(breaker.next_allowed_at(), Some(at(60 * SECOND)));
    // An earlier instruction does not shorten it.
    breaker.record(&status_after(429, 5 * SECOND), t0(), &rate, &policy);
    assert_eq!(breaker.next_allowed_at(), Some(at(60 * SECOND)));
}

#[test]
fn open_for_derivation() {
    let ceiling = SourcingPolicy::default().breaker_ceiling;
    let now = t0();
    let cases: Vec<(&str, FailureClass, Duration, u32, Duration)> = vec![
        (
            "the host's Retry-After, whatever the trips",
            FailureClass::Explicit {
                until: at(60 * SECOND),
            },
            2 * SECOND,
            5,
            60 * SECOND,
        ),
        (
            "a host's wait past the ceiling is honoured in full",
            FailureClass::Explicit {
                until: at(48 * HOUR),
            },
            2 * SECOND,
            1,
            48 * HOUR,
        ),
        (
            "a host time already past is no wait",
            FailureClass::Explicit { until: now },
            2 * SECOND,
            1,
            Duration::ZERO,
        ),
        (
            "first trip doubles",
            FailureClass::Refusal,
            2 * SECOND,
            1,
            4 * SECOND,
        ),
        (
            "third trip is ×8",
            FailureClass::Refusal,
            2 * SECOND,
            3,
            16 * SECOND,
        ),
        (
            "transport uses the same doubling",
            FailureClass::Transport,
            3 * SECOND,
            2,
            12 * SECOND,
        ),
        (
            "held to the ceiling",
            FailureClass::Refusal,
            HOUR,
            6,
            ceiling,
        ),
        (
            "an overflowing exponent is the ceiling",
            FailureClass::Refusal,
            SECOND,
            40,
            ceiling,
        ),
        (
            "no declared spacing, no window",
            FailureClass::Refusal,
            Duration::ZERO,
            3,
            Duration::ZERO,
        ),
    ];
    for (case, class, min_interval, trips, expected) in cases {
        let actual = open_for(class, now, &rate(min_interval), trips, ceiling);
        assert_eq!(actual, expected, "{case}");
    }
}

#[test]
fn classify_table() {
    let now = t0();
    let honours = rate(SECOND);
    let mut ignores = rate(SECOND);
    ignores.honours_retry_after = false;
    let cases: Vec<(&str, FailureCause, &RatePolicy, FailureClass)> = vec![
        (
            "429 with Retry-After",
            FailureCause::Status {
                code: 429,
                retry_after: Some(30 * SECOND),
            },
            &honours,
            FailureClass::Explicit {
                until: at(30 * SECOND),
            },
        ),
        (
            "503 with Retry-After",
            FailureCause::Status {
                code: 503,
                retry_after: Some(30 * SECOND),
            },
            &honours,
            FailureClass::Explicit {
                until: at(30 * SECOND),
            },
        ),
        (
            "429 with an ignored Retry-After",
            FailureCause::Status {
                code: 429,
                retry_after: Some(30 * SECOND),
            },
            &ignores,
            FailureClass::Refusal,
        ),
        (
            "403",
            FailureCause::Status {
                code: 403,
                retry_after: None,
            },
            &honours,
            FailureClass::Refusal,
        ),
        (
            "challenge",
            FailureCause::Challenge,
            &honours,
            FailureClass::Refusal,
        ),
        (
            "503",
            FailureCause::Status {
                code: 503,
                retry_after: None,
            },
            &honours,
            FailureClass::Transport,
        ),
        (
            "transport",
            FailureCause::Transport("dns".into()),
            &honours,
            FailureClass::Transport,
        ),
        (
            "404",
            FailureCause::Status {
                code: 404,
                retry_after: None,
            },
            &honours,
            FailureClass::Neutral,
        ),
        (
            "451",
            FailureCause::Status {
                code: 451,
                retry_after: None,
            },
            &honours,
            FailureClass::Neutral,
        ),
        (
            "parse",
            FailureCause::Parse("x".into()),
            &honours,
            FailureClass::Neutral,
        ),
        (
            "budget",
            FailureCause::Budget,
            &honours,
            FailureClass::Neutral,
        ),
    ];
    for (case, cause, rate, expected) in cases {
        assert_eq!(classify(&cause, rate, now), expected, "{case}");
    }
}

#[test]
fn the_caller_waits_only_when_the_window_ends_before_its_deadline() {
    let policy = SourcingPolicy::default();
    let rate = rate(SECOND);
    let mut breaker = Breaker::new();
    assert_eq!(breaker.wait_before(t0(), SECOND), Some(Duration::ZERO));

    breaker.record(&status_after(429, 30 * SECOND), t0(), &rate, &policy);
    let cases = [
        (
            "window ends inside the budget",
            60 * SECOND,
            Some(30 * SECOND),
        ),
        (
            "window ends exactly at the deadline",
            30 * SECOND,
            Some(30 * SECOND),
        ),
        ("window outlasts the budget", 10 * SECOND, None),
    ];
    for (case, remaining, expected) in cases {
        assert_eq!(breaker.wait_before(t0(), remaining), expected, "{case}");
    }
    // The caller's deadline never shortened the host's window.
    assert_eq!(breaker.next_allowed_at(), Some(at(30 * SECOND)));
}

#[test]
fn a_spent_allowance_refuses_until_its_window_ends() {
    let mut rate = rate(SECOND);
    rate.quota = Some(Quota {
        requests: 2,
        per: HOUR,
    });
    let mut breaker = Breaker::new();
    assert_eq!(breaker.admit(t0(), &rate), Admission::Go);
    assert_eq!(breaker.admit(at(SECOND), &rate), Admission::Go);
    assert_eq!(
        breaker.admit(at(2 * SECOND), &rate),
        Admission::Refuse(SkipReason::QuotaSpent { until: at(HOUR) })
    );
    assert_eq!(breaker.admit(at(HOUR), &rate), Admission::Go);
}

#[test]
fn health_state_follows_the_breaker() {
    let policy = SourcingPolicy::default();
    let rate = rate(SECOND);
    let mut breaker = Breaker::new();
    assert_eq!(breaker.health_state(), HealthState::Healthy);
    breaker.record(&status_after(429, 5 * SECOND), t0(), &rate, &policy);
    assert!(matches!(breaker.health_state(), HealthState::Open { .. }));
    breaker.admit(at(5 * SECOND), &rate);
    assert_eq!(breaker.health_state(), HealthState::HalfOpen);
}
