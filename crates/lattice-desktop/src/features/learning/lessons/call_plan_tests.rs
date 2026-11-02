#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;

fn statements(sourced: usize, reasoning: usize, common: usize) -> Vec<Counted> {
    let of = |class, count| {
        vec![
            Counted {
                class,
                shares: false,
            };
            count
        ]
    };
    [
        of(Class::Sourced, sourced),
        of(Class::Reasoning, reasoning),
        of(Class::Common, common),
    ]
    .concat()
}

fn fates(class: Class, fate: Fate, count: usize) -> Vec<(Class, Fate)> {
    vec![(class, fate); count]
}

/// The replay's shares (§3.11): 36 Sourced, 45 Reasoning and 8 Common
/// statements, sorted in 7 calls, in 3 sections that took cuts.
fn replay() -> CallPlan {
    CallPlan::new(7, &statements(36, 45, 8), 3)
}

#[test]
fn before_any_check_the_ceiling_takes_every_term_at_its_most() {
    // The calls of sorting and of the first checks, 7 + 36 + 45 + 3·45 +
    // 3·8, then F at 36 + 8 statements, G at 45, K at 3, V at 45, D at 8 and
    // U at 36 + 3·45 + 3·8.
    assert_eq!(replay().ceiling(), 247 + 3 * 44 + 6 * 45 + 3 + 45 + 8 + 195);
}

#[test]
fn the_replays_first_checks_bring_the_ceiling_to_the_designs_278() {
    let mut plan = replay();
    let mut lighter = fates(Class::Sourced, Fate::ToSource, 36);
    lighter.extend(fates(Class::Reasoning, Fate::Passed, 42));
    lighter.extend(fates(Class::Reasoning, Fate::Failed, 2));
    lighter.push((Class::Reasoning, Fate::Vetoed { stated: false }));
    lighter.extend(fates(Class::Common, Fate::Passed, 8));
    plan.lighter_checked(&lighter);
    // G is down to 3 and V to 1, D to 0; F waits for the strict check.
    assert_eq!(plan.ceiling(), 247 + 3 * 36 + 6 * 3 + 3 + 1 + 36);

    let mut strict = fates(Class::Sourced, Fate::Passed, 33);
    strict.extend(fates(Class::Sourced, Fate::Failed, 3));
    plan.strict_checked(&strict);
    assert_eq!(plan.ceiling(), 247 + 3 * 3 + 6 * 3 + 3 + 1);
    assert_eq!(plan.ceiling(), 278);
}

#[test]
fn a_common_statement_sent_to_its_source_check_does_not_raise_the_ceiling() {
    let mut plan = CallPlan::new(1, &statements(1, 1, 2), 1);
    let mut ceilings = vec![plan.ceiling()];
    plan.lighter_checked(&[
        (Class::Sourced, Fate::ToSource),
        (Class::Reasoning, Fate::Passed),
        (Class::Common, Fate::Passed),
        (Class::Common, Fate::ToSource),
    ]);
    ceilings.push(plan.ceiling());
    // The Common statement sent on fails its source check as a Sourced one
    // would, and the follow-up it could have was counted from the start.
    plan.strict_checked(&[
        (Class::Sourced, Fate::Passed),
        (Class::Common, Fate::Failed),
    ]);
    ceilings.push(plan.ceiling());
    assert_eq!(ceilings, [41, 24, 17]);
}

#[test]
fn a_sourced_statement_that_fails_its_rewrite_keeps_every_call_on_its_way_to_the_cut() {
    // The first check, the re-check against what research found, the
    // rewrite, its re-check and the flow repair after the cut.
    let mut plan = CallPlan::new(0, &statements(1, 0, 0), 1);
    plan.lighter_checked(&[(Class::Sourced, Fate::ToSource)]);
    plan.strict_checked(&[(Class::Sourced, Fate::Failed)]);
    assert_eq!(plan.ceiling(), 1 + 3 + 1);

    // The later passes check the lesson's statements again: the ceiling
    // stays, whatever they find.
    plan.lighter_checked(&[(Class::Sourced, Fate::ToSource)]);
    plan.strict_checked(&[(Class::Sourced, Fate::Passed)]);
    assert_eq!(plan.ceiling(), 5);
}

#[test]
fn a_contradicted_reasoning_statement_keeps_its_rewrite_and_recheck_before_any_cut() {
    let mut plan = CallPlan::new(0, &statements(0, 1, 0), 1);
    plan.lighter_checked(&[(Class::Reasoning, Fate::Failed)]);
    // The consistency call and the veto's samples, G's six, K's one.
    assert_eq!(plan.ceiling(), 1 + 3 + 6 + 1);
}

#[test]
fn a_statement_whose_answer_could_not_be_read_keeps_its_shares_until_it_is_asked_again() {
    let mut plan = CallPlan::new(0, &statements(1, 1, 1), 1);
    let before = plan.ceiling();
    plan.lighter_checked(&[
        (Class::Reasoning, Fate::Unanswered),
        (Class::Common, Fate::Unanswered),
        (Class::Sourced, Fate::ToSource),
    ]);
    plan.strict_checked(&[(Class::Sourced, Fate::Unanswered)]);
    assert_eq!(plan.ceiling(), before);
}

#[test]
fn statements_sharing_a_sentence_add_the_most_one_check_of_their_class_makes() {
    let alone = CallPlan::new(0, &statements(1, 1, 1), 1).ceiling();
    let sharing: Vec<Counted> = statements(1, 1, 1)
        .into_iter()
        .map(|statement| Counted {
            shares: true,
            ..statement
        })
        .collect();
    assert_eq!(CallPlan::new(0, &sharing, 1).ceiling(), alone + 1 + 5 + 4);
}

#[test]
fn the_cuts_known_bring_the_flow_repairs_to_the_sections_that_took_one_and_never_raise_them() {
    let mut plan = replay();
    let before = plan.ceiling();

    // Of the lesson's three sections that could take cuts, one did.
    plan.cuts_known(1);
    assert_eq!(plan.ceiling(), before - 2);

    // Knowing them again, or more of them, changes nothing.
    plan.cuts_known(1);
    plan.cuts_known(3);
    assert_eq!(plan.ceiling(), before - 2);

    // A lesson with no cut has no flow repair.
    plan.cuts_known(0);
    assert_eq!(plan.ceiling(), before - 3);
}

#[test]
fn no_first_check_raises_the_ceiling() {
    let all = [
        Fate::Passed,
        Fate::Failed,
        Fate::Vetoed { stated: true },
        Fate::Vetoed { stated: false },
        Fate::ToSource,
        Fate::Unanswered,
    ];
    for class in [Class::Sourced, Class::Reasoning, Class::Common] {
        for fate in all {
            let mut plan = CallPlan::new(2, &statements(1, 1, 1), 1);
            let sorted = plan.ceiling();
            plan.lighter_checked(&[(class, fate)]);
            let lighter = plan.ceiling();
            plan.strict_checked(&[(class, fate)]);
            assert!(
                lighter <= sorted && plan.ceiling() <= lighter,
                "{class:?} {fate:?}"
            );
        }
    }
}

#[test]
fn a_strict_verdict_is_a_fate() {
    assert_eq!(Fate::of_strict(ClaimVerdict::Supported), Fate::Passed);
    assert_eq!(Fate::of_strict(ClaimVerdict::Contradicted), Fate::Failed);
    assert_eq!(Fate::of_strict(ClaimVerdict::Unsupported), Fate::Failed);
    assert_eq!(Fate::of_strict(ClaimVerdict::Unverified), Fate::Unanswered);
}
