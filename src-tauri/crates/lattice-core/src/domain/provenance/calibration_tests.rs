use super::*;
use crate::domain::provenance::policy::POLICY;

const DAY: i64 = 86_400;

fn outcome(level: TrustLevel, sufficient: bool, right: bool, at: i64) -> Outcome {
    Outcome {
        level,
        kind: ClaimKind::Specification,
        decision_was_sufficient: sufficient,
        right,
        at,
    }
}

/// `n` verdicts at `level` on specification claims that held (or failed).
fn verdicts(level: TrustLevel, n: usize, held: bool, from: i64) -> Vec<Outcome> {
    let sufficient = level >= TrustLevel::Authoritative;
    (0..n)
        .map(|i| outcome(level, sufficient, held == sufficient, from + i as i64 * DAY))
        .collect()
}

fn bars(outcomes: &[Outcome]) -> EffectiveBars {
    effective_bars(
        &StaticBars::table(),
        &Calibration::from_outcomes(outcomes),
        &POLICY,
    )
}

#[test]
fn the_incomplete_beta_is_exact() {
    assert!((regularized_beta(0.5, 2.0, 3.0) - 0.6875).abs() < 1e-12);
    assert!((regularized_beta(0.3, 1.0, 1.0) - 0.3).abs() < 1e-12);
    // I_x(a, b) = P(Binomial(a + b − 1, x) ≥ a) for whole a, b.
    let binomial_tail = |n: u32, x: f64, a: u32| -> f64 {
        (a..=n)
            .map(|k| {
                let choose = (1..=k).fold(1.0, |acc, i| acc * f64::from(n - k + i) / f64::from(i));
                choose * x.powi(k as i32) * (1.0 - x).powi((n - k) as i32)
            })
            .sum()
    };
    for (x, a, b) in [(0.9, 30u32, 3u32), (0.75, 5, 9), (0.2, 2, 40)] {
        let exact = binomial_tail(a + b - 1, x, a);
        assert!(
            (regularized_beta(x, f64::from(a), f64::from(b)) - exact).abs() < 1e-10,
            "{x} {a} {b}"
        );
    }
    assert!(
        (regularized_beta(0.9, 4.5, 0.7) + regularized_beta(0.1, 0.7, 4.5) - 1.0).abs() < 1e-12
    );
}

#[test]
fn an_outcome_held_when_the_gate_was_right_to_accept_or_wrong_to_queue() {
    assert!(
        outcome(TrustLevel::Primary, true, true, 0).held(),
        "verifier agreed"
    );
    assert!(
        !outcome(TrustLevel::Primary, true, false, 0).held(),
        "removed after acceptance"
    );
    assert!(
        outcome(TrustLevel::Established, false, false, 0).held(),
        "user confirmed a queued claim"
    );
    assert!(
        !outcome(TrustLevel::Established, false, true, 0).held(),
        "user removed a queued claim"
    );
}

#[test]
fn the_prior_is_neutral_and_its_strength_matches_the_uniform_priors_evidence() {
    let prior = Prior::of(&POLICY);
    assert!(
        (regularized_beta(POLICY.accept_reliability, prior.alpha, prior.beta) - 0.5).abs() < 1e-9
    );
    assert!((Posterior::default().probability_reliable(&POLICY) - 0.5).abs() < 1e-9);
    // Straight held verdicts a cold cell needs to relax, under a prior.
    let to_relax = |alpha: f64, beta: f64| {
        (0u32..)
            .find(|k| {
                1.0 - regularized_beta(POLICY.accept_reliability, alpha + f64::from(*k), beta)
                    >= POLICY.bar_move_probability
            })
            .unwrap()
    };
    let uniform = to_relax(1.0, 1.0);
    assert_eq!(uniform, 21);
    let weight_for = |weight: f64| {
        let prior = Prior::of(&Policy {
            prior_strength: weight,
            ..POLICY
        });
        to_relax(prior.alpha, prior.beta)
    };
    let derived = (1..)
        .find(|weight| weight_for(f64::from(*weight)) >= uniform)
        .unwrap();
    assert_eq!(
        f64::from(derived),
        POLICY.prior_strength,
        "prior_strength is derived, not chosen"
    );
    let to_tighten = (0u32..)
        .find(|f| {
            regularized_beta(
                POLICY.accept_reliability,
                prior.alpha,
                prior.beta + f64::from(*f),
            ) >= POLICY.bar_move_probability
        })
        .unwrap();
    assert_eq!(to_tighten, 2);
}

#[test]
fn without_verdicts_the_static_table_stands() {
    let bars = bars(&[]);
    for (kind, lowest) in StaticBars::table().entries {
        assert_eq!(bars.lowest_accepted(kind), Some(lowest));
        assert_eq!(bars.moved_reason(kind), None);
    }
}

#[test]
fn confirmed_queued_claims_relax_a_bar_and_say_since_when() {
    let start = 1_790_000_000;
    let outcomes = verdicts(TrustLevel::Established, 40, true, start);
    let bars = bars(&outcomes);
    assert_eq!(
        bars.lowest_accepted(ClaimKind::Specification),
        Some(TrustLevel::Established)
    );
    let bar = bars.bar(ClaimKind::Specification).unwrap();
    let established = bar
        .levels
        .iter()
        .find(|l| l.level == TrustLevel::Established)
        .unwrap();
    let BarBasis::Relaxed {
        held,
        failed,
        since,
        probability,
    } = established.basis
    else {
        panic!("relaxed expected: {:?}", established.basis);
    };
    assert_eq!((held, failed), (40, 0));
    assert_eq!(
        since,
        start + 20 * DAY,
        "the 21st straight held verdict moved it"
    );
    assert!(probability >= POLICY.bar_move_probability);
    let reason = bars.moved_reason(ClaimKind::Specification).unwrap();
    assert!(
        reason.text.contains("from authoritative to established"),
        "{}",
        reason.text
    );
    assert!(
        reason.text.contains("40 verdicts, 100% held"),
        "{}",
        reason.text
    );
    assert_eq!(
        bars.lowest_accepted(ClaimKind::Historical),
        Some(TrustLevel::Authoritative),
        "other kinds unmoved"
    );
}

#[test]
fn too_few_verdicts_do_not_relax() {
    let bars = bars(&verdicts(TrustLevel::Established, 20, true, 0));
    assert_eq!(
        bars.lowest_accepted(ClaimKind::Specification),
        Some(TrustLevel::Authoritative)
    );
}

#[test]
fn accepted_claims_that_fail_tighten_a_bar_and_a_later_run_of_held_ones_restores_it() {
    let mut outcomes = verdicts(TrustLevel::Authoritative, 3, false, 0);
    let tightened = bars(&outcomes);
    assert_eq!(
        tightened.lowest_accepted(ClaimKind::Specification),
        Some(TrustLevel::Primary)
    );
    assert!(tightened
        .moved_reason(ClaimKind::Specification)
        .unwrap()
        .text
        .contains("no longer accepted alone"));
    outcomes.extend(verdicts(TrustLevel::Authoritative, 60, true, 10 * DAY));
    assert_eq!(
        bars(&outcomes).lowest_accepted(ClaimKind::Specification),
        Some(TrustLevel::Authoritative)
    );
}

#[test]
fn a_tightened_primary_leaves_no_level_accepted_alone() {
    let mut outcomes = verdicts(TrustLevel::Primary, 4, false, 0);
    for outcome in &mut outcomes {
        outcome.kind = ClaimKind::Measurement;
    }
    let bars = bars(&outcomes);
    assert_eq!(bars.lowest_accepted(ClaimKind::Measurement), None);
    assert!(bars
        .moved_reason(ClaimKind::Measurement)
        .unwrap()
        .text
        .contains("to no single source"));
}

#[test]
fn a_relaxed_level_below_a_tightened_one_does_not_open_the_bar() {
    let mut outcomes = verdicts(TrustLevel::Authoritative, 3, false, 0);
    outcomes.extend(verdicts(TrustLevel::Established, 40, true, 0));
    assert_eq!(
        bars(&outcomes).lowest_accepted(ClaimKind::Specification),
        Some(TrustLevel::Primary)
    );
}

#[test]
fn distrusted_outcomes_are_never_calibrated() {
    let calibration = Calibration::from_outcomes(&verdicts(TrustLevel::Distrusted, 50, true, 0));
    assert!(calibration.cells.is_empty());
}
