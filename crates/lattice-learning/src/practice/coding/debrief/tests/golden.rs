#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::golden_support::{add_requests, assert_golden, Golden, Recorder};

fn rich_evidence() -> SessionEvidence {
    let mut evidence = evidence();
    let mut second = run("r2", CodingRunScope::Shown);
    second.files = file("def two_sum(numbers, target):\n    return [0, 0]\n");
    if let Some(result) = second.results.first_mut() {
        result.status = CaseStatus::Failed;
        result.actual = Some("[0, 0]".into());
    }
    evidence.runs.push(second);
    evidence.examples.push(example(1));
    evidence.hints.push(hint(1, Rung::Idea));
    evidence
}

#[tokio::test]
async fn golden_debrief() {
    let evidence = rich_evidence();
    let mut golden = Golden::default();
    golden.add("system", &system());
    golden.add(
        "schema",
        &serde_json::to_string_pretty(&schema()).expect("a schema writes out"),
    );
    golden.add("prompt", &prompt(&evidence).expect("a prompt"));

    let mut faulty = good_rows();
    if let Some(first) = faulty.first_mut() {
        *first = row(
            "problem_solving",
            Some(3),
            vec![quote("t1", "words the learner never said")],
        );
    }
    let model = Recorder::new([answer(faulty), answer(good_rows())]);
    let debrief = CodingDebriefer { llm: &model }.debrief(&evidence).await;
    assert!(debrief.is_ok(), "{debrief:?}");
    add_requests(
        &mut golden,
        "debrief with a fault in the first answer",
        &model,
    );

    assert_golden(
        "debrief",
        include_str!("../../golden/debrief.txt"),
        golden.text(),
    );
}
