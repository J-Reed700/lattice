#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::golden_support::{assert_golden, render_completion, Golden};

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
    let model = scripted(
        |sent, nth| match nth {
            0 => answer(good_entries()[..3].to_vec()),
            _ => answer(vec![entry(
                &handle_of(sent, "Communication"),
                "3",
                json!({"hints": ["h1"]}),
            )]),
        },
        |sent, nth| match nth {
            0 => json!({"entries": [
                {"of": "k1", "text": " "},
                {"of": "k2", "text": "O(n) time, O(n) space"},
            ]})
            .to_string(),
            _ => json!({"entries": [{
                "of": handle_of(sent, "approach"),
                "text": "Remember what you have seen in a dictionary.",
            }]})
            .to_string(),
        },
    );
    let debrief = CodingDebriefer { llm: &model }
        .debrief(&rich_evidence())
        .await;
    assert!(debrief.is_ok(), "{debrief:?}");

    let mut golden = Golden::default();
    for (name, requests) in [
        (
            "the rows, with a criterion missing from the first answer",
            sent_to::<DebriefAnswer>(&model),
        ),
        (
            "what it states, with a blank statement in the first answer",
            sent_to::<DebriefStatements>(&model),
        ),
    ] {
        let total = requests.len();
        for (at, sent) in requests.iter().enumerate() {
            golden.add(
                &format!("{name}, request {} of {total}", at + 1),
                &render_completion(&sent.request),
            );
        }
    }

    assert_golden(
        "debrief",
        include_str!("../../golden/debrief.txt"),
        golden.text(),
    );
}
