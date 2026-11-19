//! Stage 4: every mutation through its strategy; the unproven are dropped and the drill's files and spans are the
//! baseline with the proven edits applied again.

use super::support::{
    baseline, build, files, idle_grader, judge_answer, literals_edit, literals_mutation,
    operator_edit, operator_mutation, planted, spec, unchecked, FakeRunner, ScriptedModel,
    KILLING_TEST, LITERALS, OPERATOR,
};
use crate::drills::build::{Mutation, Planted, Trail, Verification};
use crate::tests::pool;
use lattice_core::domain::learning::drills::{
    apply, check_key, Drill, Edit, LineSpan, Material, Origin, Proof, Severity,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

fn span(path: &str, line: u32) -> LineSpan {
    LineSpan {
        path: path.into(),
        start: line,
        end: line,
    }
}

#[tokio::test]
async fn every_mutation_proven_gives_the_files_the_spans_and_the_planted_key() -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([judge_answer(true)]);
    let runner = FakeRunner::killing_the_operator();
    let grader = idle_grader();
    let proved = build(&db, &model, Some(&runner), &grader)
        .prove(&spec(), &baseline(), &planted())
        .await?;
    assert_eq!(proved.proven.len(), 2);
    assert!(proved.dropped.is_empty());
    let (expected, _) = apply(&files(), &[operator_edit(), literals_edit()]).unwrap();
    assert_eq!(proved.files, expected);
    assert_eq!(
        proved.spans,
        vec![span("src/limiter.py", 13), span("src/config.py", 8)]
    );
    assert_eq!(
        proved.proven[0].proof,
        Proof::Killed {
            test: KILLING_TEST.into()
        }
    );
    assert!(matches!(&proved.proven[1].proof, Proof::Judged { .. }));
    assert_eq!(model.purposes(), ["judge"], "the killed one asks nothing");
    // The killing test runs on both versions; the suite on the smell.
    assert_eq!(runner.runs().len(), 3);

    let key = proved.planted_key(&unchecked());
    assert_eq!(key[0].id, "planted-1");
    assert_eq!(key[0].pattern, OPERATOR);
    assert_eq!(key[0].severity, Severity::Major);
    assert_eq!(key[0].span, span("src/limiter.py", 13));
    assert_eq!(key[0].origin, Origin::Planted);
    assert_eq!(key[0].fix.before, operator_edit().after);
    assert_eq!(key[0].fix.after, operator_edit().before);
    assert_eq!(key[1].id, "planted-2");
    assert_eq!(key[1].pattern, LITERALS);
    let drill = Drill {
        id: "d".into(),
        material: Material::code("python"),
        title: "A limiter".into(),
        files: proved.files.clone(),
        key,
    };
    assert_eq!(check_key(&files(), &drill), Ok(()));
    Ok(())
}

#[tokio::test]
async fn an_unproven_mutation_is_dropped_with_why_and_the_files_are_applied_again_without_it(
) -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([judge_answer(false)]);
    let runner = FakeRunner::killing_the_operator();
    let grader = idle_grader();
    let proved = build(&db, &model, Some(&runner), &grader)
        .prove(&spec(), &baseline(), &planted())
        .await?;
    assert_eq!(proved.proven.len(), 1);
    assert_eq!(proved.proven[0].mutation.pattern, OPERATOR);
    assert_eq!(proved.dropped.len(), 1);
    assert_eq!(proved.dropped[0].mutation, literals_mutation());
    assert_eq!(
        proved.dropped[0].why,
        "The original code already has this problem."
    );
    assert_eq!(proved.dropped[0].trail.calls[0].purpose, "judge");
    let (expected, _) = apply(&files(), &[operator_edit()]).unwrap();
    assert_eq!(proved.files, expected);
    assert_eq!(
        proved.files[1],
        files()[1],
        "the dropped edit is not in the code"
    );
    assert_eq!(proved.spans, vec![span("src/limiter.py", 13)]);
    Ok(())
}

#[tokio::test]
async fn a_mutation_that_cannot_be_undone_once_another_is_dropped_is_dropped_too() -> Result<()> {
    let db = Database::new(pool().await?);
    // The first puts a line into the code that the second's original text also holds: undoing the first is certain
    // only while the second stays in.
    let first = Mutation {
        pattern: "correctness.stale_state".into(),
        edit: Edit {
            path: "src/limiter.py".into(),
            before: "        self.clock = clock\n".into(),
            after: "        recent.append(now)\n".into(),
        },
        severity: Severity::Major,
        consequence: "The clock is never kept.".into(),
        explanation: "The clock is dropped. Keep it.".into(),
        verification: Verification::Unobservable("It is a state error that needs a clock.".into()),
        lesson_sentences: vec![],
    };
    let second = Mutation {
        pattern: "correctness.off_by_one".into(),
        edit: Edit {
            path: "src/limiter.py".into(),
            before: "        recent.append(now)\n".into(),
            after: "        recent.append(now - 1)\n".into(),
        },
        severity: Severity::Major,
        consequence: "A call is counted a tick early.".into(),
        explanation: "The time is shifted. Record the time itself.".into(),
        verification: Verification::Unobservable("No test can observe the shift.".into()),
        lesson_sentences: vec![],
    };
    let planted = Planted {
        mutations: vec![first, second],
        trail: Trail::default(),
    };
    let model = ScriptedModel::new([judge_answer(true), judge_answer(false)]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let proved = build(&db, &model, Some(&runner), &grader)
        .prove(&spec(), &baseline(), &planted)
        .await?;
    assert!(proved.proven.is_empty());
    assert_eq!(proved.dropped.len(), 2);
    assert!(
        proved.dropped[1].why.contains("more than once"),
        "{}",
        proved.dropped[1].why
    );
    assert_eq!(proved.files, files());
    assert!(proved.spans.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_build_without_a_runner_proves_every_defect_by_judgement() -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([judge_answer(true), judge_answer(true)]);
    let grader = idle_grader();
    let proved = build(&db, &model, None, &grader)
        .prove(&spec(), &baseline(), &planted())
        .await?;
    assert_eq!(model.purposes(), ["judge", "judge"]);
    assert!(proved
        .proven
        .iter()
        .all(|item| matches!(item.proof, Proof::Judged { .. })));
    assert_eq!(proved.proven[0].mutation, operator_mutation());
    Ok(())
}
