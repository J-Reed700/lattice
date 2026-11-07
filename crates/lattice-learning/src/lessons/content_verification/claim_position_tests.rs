//! Where the ledger has a claim in its class's graph: read from the steps it
//! records for it, from the entry of the class it was sorted into or from the
//! post-rewrite node of the claim it succeeds (acceptance design §2.9).
use super::claim_ledger_tests::{identify_findings, pass_report, passage_finding, CELL};
use super::*;
use crate::lessons::call_plan::{Planned, PlannedAt};
use lattice_core::domain::learning::acceptance::{
    decide, AcceptancePolicy, End, Goal, Outcome, Position, Reading, Step, Taken, Then, Walked,
};
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLocation, UnitKind,
};

const SENTENCE: &str = "The reader hands over one row at a time.";

fn at(
    policy: &dyn AcceptancePolicy,
    class: Class,
    (changed_class, reasked): (bool, bool),
    node: &str,
) -> Position {
    Position {
        class,
        changed_class,
        reasked,
        node: policy.graph(class).id_of(node).unwrap(),
    }
}

fn taken(reading: Reading, outcome: Outcome) -> Taken {
    Taken {
        step: Step::Read(reading),
        outcome,
    }
}

fn rewrite_step(goal: Goal, outcome: Outcome) -> Taken {
    Taken {
        step: Step::Rewrite(goal),
        outcome,
    }
}

/// A claim of section 0 the ledger holds: extracted, then sorted as `sorted`
/// says, in order.
async fn claim(
    ledger: &mut claim_ledger::ClaimLedger,
    name: &str,
    statement: &str,
    sorted: &[(Class, ClassifiedBy)],
) -> Result<ClaimId> {
    let id = ClaimId(name.into());
    ledger
        .append(
            &id,
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", SENTENCE, SENTENCE),
                statement: statement.into(),
            },
        )
        .await?;
    for &(class, by) in sorted {
        ledger
            .append(&id, ClaimEvent::Classified { class, by })
            .await?;
    }
    Ok(id)
}

/// Records `steps` for `id`, each with the decision the policy makes of it,
/// from `start`.
async fn step(
    ledger: &mut claim_ledger::ClaimLedger,
    policy: &dyn AcceptancePolicy,
    id: &ClaimId,
    start: Position,
    steps: &[Taken],
) -> Result<()> {
    let mut here = Some(start);
    for &taken in steps {
        let (then, next) = decide(policy, here.unwrap(), taken).unwrap();
        here = next;
        ledger
            .append(
                id,
                ClaimEvent::Stepped {
                    policy: policy.version().into(),
                    step: taken.step,
                    outcome: taken.outcome,
                    then,
                    reason: "Because.".into(),
                    rewrite_ahead: false,
                },
            )
            .await?;
    }
    Ok(())
}

async fn superseded(
    ledger: &mut claim_ledger::ClaimLedger,
    id: &ClaimId,
    successor: &ClaimId,
    by: Actor,
) -> Result<()> {
    ledger
        .append(
            id,
            ClaimEvent::Superseded {
                successor: Some(successor.clone()),
                by,
            },
        )
        .await?;
    Ok(())
}

async fn open() -> Result<claim_ledger::ClaimLedger> {
    claim_ledger::ClaimLedger::open(&Model::new(), "{}").await
}

#[tokio::test]
async fn a_claim_with_no_steps_stands_at_the_entry_of_the_strongest_class_it_was_sorted_into(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sorted = [
        (Class::Reasoning, ClassifiedBy::Model),
        (Class::Sourced, ClassifiedBy::Writer),
        (Class::Common, ClassifiedBy::Rule),
    ];
    let cited = claim(&mut ledger, "cited", "Cited.", &sorted).await?;
    let reasoned = claim(
        &mut ledger,
        "reasoned",
        "Reasoned.",
        &[(Class::Reasoning, ClassifiedBy::Model)],
    )
    .await?;

    assert_eq!(
        ledger.position(&policy, &cited),
        Some(Walked::At(at(
            &policy,
            Class::Sourced,
            (false, false),
            "s.read"
        )))
    );
    assert_eq!(
        ledger.position(&policy, &reasoned),
        Some(Walked::At(at(
            &policy,
            Class::Reasoning,
            (false, false),
            "r.read"
        )))
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_with_no_class_or_not_in_the_ledger_has_no_position() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let unsorted = claim(&mut ledger, "unsorted", "Unsorted.", &[]).await?;
    // The evidence's word is not the class the claim was sorted into.
    let by_evidence = claim(
        &mut ledger,
        "by-evidence",
        "By evidence.",
        &[(Class::Common, ClassifiedBy::Evidence)],
    )
    .await?;

    assert_eq!(ledger.position(&policy, &unsorted), None);
    assert_eq!(ledger.position(&policy, &by_evidence), None);
    assert_eq!(ledger.position(&policy, &ClaimId("unknown".into())), None);
    Ok(())
}

#[tokio::test]
async fn a_claim_with_recorded_steps_stands_where_they_walk_it_to() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sorted = [(Class::Reasoning, ClassifiedBy::Model)];
    let start = at(&policy, Class::Reasoning, (false, false), "r.read");
    let goal = Goal::AgreeWithThePassages;

    let walking = claim(&mut ledger, "walking", "Walking.", &sorted).await?;
    let steps = [
        taken(Reading::Consistency, Outcome::Refuted),
        rewrite_step(goal, Outcome::NotApplied),
    ];
    step(&mut ledger, &policy, &walking, start, &steps).await?;
    assert_eq!(
        ledger.position(&policy, &walking),
        Some(Walked::At(at(
            &policy,
            Class::Reasoning,
            (false, false),
            "r.rewrite.again"
        )))
    );

    // To its end: the cut takes the statement out of its sentence.
    let cut = claim(&mut ledger, "cut", "Cut.", &sorted).await?;
    let steps = [
        taken(Reading::Consistency, Outcome::Refuted),
        rewrite_step(goal, Outcome::NotApplied),
        rewrite_step(goal, Outcome::NotApplied),
        Taken {
            step: Step::Cut,
            outcome: Outcome::Cut,
        },
    ];
    step(&mut ledger, &policy, &cut, start, &steps).await?;
    assert_eq!(
        ledger.position(&policy, &cut),
        Some(Walked::Ended(End::Removed {
            section_goes: false
        }))
    );

    // An answer that cannot be read is asked once more, and then the lesson
    // stops.
    let unread = claim(&mut ledger, "unread", "Unread.", &sorted).await?;
    let ask = taken(Reading::Consistency, Outcome::Unjudged);
    step(&mut ledger, &policy, &unread, start, &[ask]).await?;
    assert_eq!(
        ledger.position(&policy, &unread),
        Some(Walked::At(at(
            &policy,
            Class::Reasoning,
            (false, true),
            "r.read"
        )))
    );
    ledger
        .append(
            &unread,
            ClaimEvent::Stepped {
                policy: policy.version().into(),
                step: ask.step,
                outcome: ask.outcome,
                then: Then::Stop,
                reason: "Unreadable.".into(),
                rewrite_ahead: false,
            },
        )
        .await?;
    assert_eq!(ledger.position(&policy, &unread), Some(Walked::Stopped));
    Ok(())
}

#[tokio::test]
async fn a_claim_whose_class_changed_by_evidence_stands_in_the_graph_it_entered() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "cited",
        "Cited.",
        &[(Class::Sourced, ClassifiedBy::Writer)],
    )
    .await?;

    // Every part common, none stated: the citation goes and the statement is
    // checked as Common, which the ledger records as the evidence's class.
    let start = at(&policy, Class::Sourced, (false, false), "s.read");
    let steps = [
        taken(Reading::Parts, Outcome::AllCommon),
        Taken {
            step: Step::Uncite,
            outcome: Outcome::Done,
        },
    ];
    step(&mut ledger, &policy, &id, start, &steps).await?;
    ledger
        .append(
            &id,
            ClaimEvent::Classified {
                class: Class::Common,
                by: ClassifiedBy::Evidence,
            },
        )
        .await?;

    assert_eq!(
        ledger.position(&policy, &id),
        Some(Walked::At(at(
            &policy,
            Class::Common,
            (true, false),
            "c.read"
        )))
    );
    // The class the ledger holds is now the evidence's; the walk began from
    // the one it was sorted into.
    assert_eq!(
        ledger.entry(&id).and_then(|entry| entry.class()),
        Some(Class::Common)
    );
    Ok(())
}

#[tokio::test]
async fn a_successor_starts_at_the_post_rewrite_node_of_the_claim_it_follows() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sorted = [(Class::Reasoning, ClassifiedBy::Model)];
    let before = claim(&mut ledger, "before", "Before.", &sorted).await?;
    let start = at(&policy, Class::Reasoning, (false, false), "r.read");
    let steps = [
        taken(Reading::Consistency, Outcome::Refuted),
        rewrite_step(Goal::AgreeWithThePassages, Outcome::Rewritten),
    ];
    step(&mut ledger, &policy, &before, start, &steps).await?;
    let after = claim(&mut ledger, "after", "After.", &sorted).await?;
    // A successor that was never sorted has the claim it follows to start from.
    let unsorted = claim(&mut ledger, "unsorted", "Unsorted.", &[]).await?;
    superseded(&mut ledger, &before, &after, Actor::Machine).await?;
    superseded(&mut ledger, &before, &unsorted, Actor::Machine).await?;

    let after_rewrite = at(&policy, Class::Reasoning, (false, false), "r.final");
    assert_eq!(
        ledger.position(&policy, &after),
        Some(Walked::At(after_rewrite))
    );
    assert_eq!(
        ledger.position(&policy, &unsorted),
        Some(Walked::At(after_rewrite))
    );
    // Its own steps go on from there.
    step(
        &mut ledger,
        &policy,
        &after,
        after_rewrite,
        &[taken(Reading::Consistency, Outcome::Stands)],
    )
    .await?;
    assert_eq!(
        ledger.position(&policy, &after),
        Some(Walked::At(at(
            &policy,
            Class::Reasoning,
            (false, false),
            "r.reveto"
        )))
    );
    Ok(())
}

#[tokio::test]
async fn a_successor_takes_the_class_and_the_spent_re_ask_its_predecessor_had_come_to() -> Result<()>
{
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let before = claim(
        &mut ledger,
        "before",
        "Before.",
        &[(Class::Sourced, ClassifiedBy::Writer)],
    )
    .await?;
    // An unreadable answer asked again, every part common (the citation goes
    // and Common's check takes it on), a refusal, the passages' word and a
    // rewrite: at Common's post-rewrite node, a class changed, a re-ask spent.
    let steps = [
        taken(Reading::Parts, Outcome::Unjudged),
        taken(Reading::Parts, Outcome::AllCommon),
        Taken {
            step: Step::Uncite,
            outcome: Outcome::Done,
        },
        taken(Reading::Knowledge, Outcome::Refuted),
        taken(Reading::Ground, Outcome::Refuted),
        rewrite_step(Goal::AsTheFieldHasIt, Outcome::Rewritten),
    ];
    let start = at(&policy, Class::Sourced, (false, false), "s.read");
    step(&mut ledger, &policy, &before, start, &steps).await?;
    let after = claim(&mut ledger, "after", "After.", &[]).await?;
    superseded(&mut ledger, &before, &after, Actor::Machine).await?;

    assert_eq!(
        ledger.position(&policy, &after),
        Some(Walked::At(at(
            &policy,
            Class::Common,
            (true, true),
            "c.final"
        )))
    );
    Ok(())
}

#[tokio::test]
async fn the_successor_of_a_claim_that_had_ended_still_starts_after_its_rewrite() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sorted = [(Class::Reasoning, ClassifiedBy::Model)];
    let passed = claim(&mut ledger, "passed", "Passed.", &sorted).await?;
    let start = at(&policy, Class::Reasoning, (false, false), "r.read");
    let steps = [
        taken(Reading::Consistency, Outcome::Stands),
        taken(Reading::Veto, Outcome::Stands),
    ];
    step(&mut ledger, &policy, &passed, start, &steps).await?;
    // A neighbour's rewrite changed its sentence: it is read again.
    let again = claim(&mut ledger, "again", "Again.", &sorted).await?;
    superseded(&mut ledger, &passed, &again, Actor::Machine).await?;

    assert_eq!(
        ledger.position(&policy, &passed),
        Some(Walked::Ended(End::Consistent))
    );
    assert_eq!(
        ledger.position(&policy, &again),
        Some(Walked::At(at(
            &policy,
            Class::Reasoning,
            (false, false),
            "r.final"
        )))
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_the_user_edited_began_afresh_and_a_record_that_loops_still_ends() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sorted = [(Class::Sourced, ClassifiedBy::Writer)];
    let before = claim(&mut ledger, "before", "Before.", &sorted).await?;
    let edited = claim(&mut ledger, "edited", "Edited.", &sorted).await?;
    superseded(&mut ledger, &before, &edited, Actor::User).await?;
    assert_eq!(
        ledger.position(&policy, &edited),
        Some(Walked::At(at(
            &policy,
            Class::Sourced,
            (false, false),
            "s.read"
        )))
    );

    // Claims that each follow the other cannot start each other for ever.
    let (a, b) = (
        claim(&mut ledger, "a", "A.", &sorted).await?,
        claim(&mut ledger, "b", "B.", &sorted).await?,
    );
    superseded(&mut ledger, &a, &b, Actor::Machine).await?;
    superseded(&mut ledger, &b, &a, Actor::Machine).await?;
    assert!(ledger.position(&policy, &a).is_some());
    assert!(ledger.position(&policy, &b).is_some());
    Ok(())
}

#[tokio::test]
async fn the_sort_is_told_where_each_live_statement_is_and_a_statement_with_more_to_do_wins(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sorted = [(Class::Reasoning, ClassifiedBy::Model)];
    let start = at(&policy, Class::Reasoning, (false, false), "r.read");
    let passed = claim(&mut ledger, "passed", "Said twice.", &sorted).await?;
    step(
        &mut ledger,
        &policy,
        &passed,
        start,
        &[
            taken(Reading::Consistency, Outcome::Stands),
            taken(Reading::Veto, Outcome::Stands),
        ],
    )
    .await?;
    claim(&mut ledger, "waiting", "Said twice.", &sorted).await?;
    claim(&mut ledger, "unsorted", "Never sorted.", &[]).await?;

    let rewrites = ledger.rewrites(&policy);

    assert_eq!(rewrites.walked(0, "Said twice."), Some(Walked::At(start)));
    assert_eq!(rewrites.walked(0, "Never sorted."), None);
    assert_eq!(rewrites.walked(1, "Said twice."), None);
    Ok(())
}

#[tokio::test]
async fn the_plans_statements_are_followed_to_the_claims_that_carry_their_sentences_on(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sorted = [(Class::Reasoning, ClassifiedBy::Model)];
    let start = at(&policy, Class::Reasoning, (false, false), "r.read");
    let first = claim(&mut ledger, "first", "First.", &sorted).await?;
    let steps = [
        taken(Reading::Consistency, Outcome::Refuted),
        rewrite_step(Goal::AgreeWithThePassages, Outcome::Rewritten),
    ];
    step(&mut ledger, &policy, &first, start, &steps).await?;
    let carried = claim(&mut ledger, "carried", "Carried.", &sorted).await?;
    superseded(&mut ledger, &first, &carried, Actor::Machine).await?;
    let after_rewrite = at(&policy, Class::Reasoning, (false, false), "r.final");
    let finished = [
        taken(Reading::Consistency, Outcome::Stands),
        taken(Reading::Veto, Outcome::Stands),
    ];
    step(&mut ledger, &policy, &carried, after_rewrite, &finished).await?;
    let second = claim(&mut ledger, "second", "Second.", &sorted).await?;
    let third = ClaimId("not-in-the-ledger".into());

    let planned = ledger.planned(&policy, &Value::Null, &[first, second, third]);

    let plan = |class, at| Planned {
        class,
        at,
        sentence: None,
    };
    assert_eq!(
        planned,
        [
            plan(Class::Reasoning, PlannedAt::Ended),
            plan(Class::Reasoning, PlannedAt::At(start)),
            plan(Class::Sourced, PlannedAt::Entry),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn the_ceiling_falls_as_the_ledger_walks_the_plans_claims_to_their_ends_and_never_rises(
) -> Result<()> {
    const FIRST: &str = "Mitochondria make most of a cell's ATP by oxidative phosphorylation.";
    const SECOND: &str = "A typical human cell contains exactly twelve mitochondria.";
    let policy = StandardPolicy::new();
    let lesson = json!({"blocks":[{"kind":"explanation","title":"Cell","body":CELL,"rubric":[]}],"questions":[]});
    let reasoned = Classed {
        class: Class::Reasoning,
        by: ClassifiedBy::Model,
    };
    let finding = |statement: &str| Finding {
        class: Some(reasoned),
        ..passage_finding(CELL, statement, true, "A source.")
    };

    let (done, trace) = crate::lesson_progress::traced(async {
        let mut ledger = open().await?;
        // The sort planned two Reasoning statements, in one section.
        let statement = Planned {
            class: Class::Reasoning,
            at: PlannedAt::Entry,
            sentence: None,
        };
        crate::lesson_progress::calls_planned(
            &policy,
            &[statement, statement],
            1,
        );
        let report = pass_report(vec![finding(FIRST), finding(SECOND)]);
        identify_findings(&mut ledger, &lesson, &report).await?;
        repair_loop::plan_moved(&policy, &ledger, &lesson);
        let planned = ledger.finding_claims().to_vec();
        assert_eq!(
            crate::lesson_progress::plan_claims(),
            Some(planned.clone())
        );

        // Each claim, in its turn, is read for consistency and passes the veto.
        let start = at(&policy, Class::Reasoning, (false, false), "r.read");
        let passes = [
            taken(Reading::Consistency, Outcome::Stands),
            taken(Reading::Veto, Outcome::Stands),
        ];
        for claim in planned {
            step(&mut ledger, &policy, &claim, start, &passes).await?;
            repair_loop::plan_moved(&policy, &ledger, &lesson);
        }
        Ok::<(), AppError>(())
    })
    .await;
    done?;

    // The sort's ceiling (0 sorting calls, 15 for each statement, 1 flow
    // repair), the same once the claims were identified with nothing done,
    // then 15 less as each ends.
    assert_eq!(trace.ceilings, [31, 31, 16, 1]);
    assert!(trace.only_came_down(), "{:?}", trace.ceilings);
    Ok(())
}

#[test]
fn statements_standing_on_one_sentence_share_a_group_and_the_rest_have_none() {
    let place = |unit, quote| claim_ledger::Place {
        unit,
        field: "body",
        quote,
    };
    let places = [
        place(0, "One. Two."),
        place(0, "One."),
        place(0, "Two."),
        place(0, "Three."),
        place(1, "One."),
        place(0, "Three."),
        place(0, ""),
        place(0, ""),
    ];

    let groups = claim_ledger::sentences(&places);

    // The passage holds both sentences, so the statements on them are one
    // group; the same words in another section, or no words, are not.
    assert_eq!(groups.len(), places.len());
    assert!(groups[0].is_some());
    assert_eq!(groups[0], groups[1]);
    assert_eq!(groups[0], groups[2]);
    assert!(groups[3].is_some());
    assert_eq!(groups[3], groups[5]);
    assert_ne!(groups[0], groups[3]);
    assert_eq!(groups[4], None);
    assert_eq!(groups[6], None);
    assert_eq!(groups[7], None);
    assert!(claim_ledger::sentences(&[]).is_empty());
}
