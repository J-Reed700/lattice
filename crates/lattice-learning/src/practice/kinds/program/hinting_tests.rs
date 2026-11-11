//! The engine's hint ladder, with a program's roles, asks for what today's coding hints ask for, byte for byte.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::practice::coding::golden_support::{render_completion, rich_session, Recorder};
use crate::practice::coding::problem::{CodingSetting, CodingTask, SOLUTION_PATH};
use crate::practice::kinds::program::observation_tests::{engine_observation, program_kind};
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::exercise_of;
use crate::practice::session::hints::give;
use crate::practice::session::kind::Hint;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::Rung;
use lattice_core::shared::error::Result;
use tokio_util::sync::CancellationToken;

const SETTINGS: [CodingSetting; 3] = [
    CodingSetting::Coached,
    CodingSetting::Mock,
    CodingSetting::Watched,
];

/// The ladder, with the names today's rungs have: the last rung points at lines.
const RUNGS: [(Rung, &str); 4] = [
    (Rung::Nudge, "Nudge"),
    (Rung::Idea, "Idea"),
    (Rung::Plan, "Plan"),
    (Rung::Point, "Line"),
];

const NUDGE_WITH_A_BLOCK: &str = "{\"text\":\"Remember them:\n```python\nx\n```\",\"reason\":\"It shows the step.\",\"lines\":[],\"path\":null}";

fn written(rung: Rung) -> String {
    if rung == Rung::Point {
        serde_json::json!({
            "text": "This return ends the loop on the first pass.",
            "reason": "It points at the return.",
            "lines": [3],
            "path": "solution.py",
        })
        .to_string()
    } else {
        serde_json::json!({
            "text": "What should the loop do with each value?",
            "reason": "It points at the accumulation.",
            "lines": [],
            "path": null,
        })
        .to_string()
    }
}

async fn given(
    llm: &dyn LLMPort,
    task: CodingTask,
    setting: CodingSetting,
    rung: Rung,
    in_role: bool,
) -> Result<Hint> {
    let (problem, session, seen) = rich_session(task, setting);
    let observation = engine_observation(&problem, &session, Some(&seen), false);
    give(
        llm,
        &program_kind(),
        &exercise_of(&problem).unwrap(),
        &observation,
        rung,
        in_role,
        CancellationToken::new(),
    )
    .await
}

/// What every request of today's hints golden says before its options, with the parts a model server never
/// receives left out: the history, the closing and the evidence, all empty on a hint request.
fn without_empty_frame(variant: &str) -> String {
    let history = variant.find("--- history ---\n").unwrap();
    let schema = variant.find("--- schema ---\n").unwrap();
    let options = variant.find("--- options ---\n").unwrap();
    format!("{}{}", &variant[..history], &variant[schema..options])
}

fn before_options(sent: &str) -> &str {
    &sent[..sent.find("--- options ---\n").unwrap()]
}

/// The bodies of the golden's variants, which are named `names` in the order they were added.
fn variants(golden: &str, names: &[String]) -> Vec<String> {
    let golden = golden.replace("\r\n", "\n");
    let mut found = Vec::new();
    let mut from = 0;
    for name in names {
        let header = format!("=== {name} ===\n");
        let at = from + golden[from..].find(&header).unwrap();
        found.push((at, at + header.len()));
        from = at + header.len();
    }
    found
        .iter()
        .enumerate()
        .map(|(index, (_, body))| {
            let end = found.get(index + 1).map_or(golden.len(), |(at, _)| *at);
            golden[*body..end].trim_end_matches('\n').to_owned()
        })
        .collect()
}

#[tokio::test]
async fn the_requests_say_what_the_coding_hints_golden_says() {
    let mut names = Vec::new();
    let mut sent = Vec::new();
    for setting in SETTINGS {
        for (rung, label) in RUNGS {
            for in_role in [false, true] {
                let model = Recorder::new([written(rung)]);
                let hint = given(&model, CodingTask::Problem, setting, rung, in_role)
                    .await
                    .unwrap();
                assert_eq!(hint.rung, rung);
                names.push(format!("Problem {setting:?} {label} in_role={in_role}"));
                sent.push(render_completion(&model.requests()[0]));
            }
        }
    }
    for setting in SETTINGS {
        for (rung, label) in RUNGS {
            let model = Recorder::new([written(rung)]);
            given(&model, CodingTask::BugFix, setting, rung, false)
                .await
                .unwrap();
            names.push(format!("BugFix {setting:?} {label} in_role=false"));
            sent.push(render_completion(&model.requests()[0]));
        }
    }
    let golden = variants(include_str!("../../coding/golden/hints.txt"), &names);
    for ((name, sent), expected) in names.iter().zip(&sent).zip(&golden) {
        assert_eq!(
            before_options(sent),
            without_empty_frame(expected),
            "hint request {name}"
        );
    }
}

#[tokio::test]
async fn a_refused_hint_is_asked_again_once_with_the_reason() {
    let model = Recorder::new([NUDGE_WITH_A_BLOCK.to_owned(), written(Rung::Nudge)]);
    let hint = given(
        &model,
        CodingTask::Problem,
        CodingSetting::Mock,
        Rung::Nudge,
        false,
    )
    .await
    .unwrap();
    assert_eq!(hint.rung, Rung::Nudge);
    let sent = model.requests();
    assert_eq!(sent.len(), 2);
    let again = render_completion(&sent[1]);
    assert!(again.contains(NUDGE_WITH_A_BLOCK), "{again}");
    assert!(
        again.contains("A Nudge hint holds no fenced code block. Say it in words."),
        "{again}"
    );
}

#[tokio::test]
async fn a_line_hints_lines_come_back_as_parts() {
    let model = Recorder::new([serde_json::json!({
        "text": "These lines end the loop early.",
        "reason": "They return on the first pass.",
        "lines": [2, 3],
        "path": "solution.py",
    })
    .to_string()]);
    let hint = given(
        &model,
        CodingTask::Problem,
        CodingSetting::Coached,
        Rung::Point,
        false,
    )
    .await
    .unwrap();
    assert_eq!(hint.rung, Rung::Point);
    assert_eq!(
        hint.parts,
        vec![part_of(SOLUTION_PATH, 2), part_of(SOLUTION_PATH, 3)]
    );
}

#[tokio::test]
async fn a_hint_of_another_rung_has_no_parts() {
    let model = Recorder::new([serde_json::json!({
        "text": "What should the loop do with each value?",
        "reason": "It points at the accumulation.",
        "lines": [2],
        "path": "solution.py",
    })
    .to_string()]);
    let hint = given(
        &model,
        CodingTask::Problem,
        CodingSetting::Coached,
        Rung::Nudge,
        false,
    )
    .await
    .unwrap();
    assert_eq!(hint.rung, Rung::Nudge);
    assert!(hint.parts.is_empty());
}
