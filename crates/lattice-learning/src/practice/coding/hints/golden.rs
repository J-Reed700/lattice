#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::golden_support::{
    assert_golden, render_grounded, rich_observation, Golden,
};
use crate::practice::coding::problem::CodingTask;

const SETTINGS: [CodingSetting; 3] = [
    CodingSetting::Coached,
    CodingSetting::Mock,
    CodingSetting::Watched,
];

const RUNGS: [CodeHintRung; 4] = [
    CodeHintRung::Nudge,
    CodeHintRung::Idea,
    CodeHintRung::Plan,
    CodeHintRung::Line,
];

const REFUSED_ANSWER: &str = "{\"text\":\"Remember them:\n```python\nx\n```\",\"reason\":\"It shows the step.\",\"lines\":[],\"path\":null}";

#[test]
fn golden_hints() {
    let mut golden = Golden::default();
    for setting in SETTINGS {
        let (problem, observation) = rich_observation(CodingTask::Problem, setting);
        for rung in RUNGS {
            for in_role in [false, true] {
                let request = request_for(&problem, &observation, rung, in_role);
                golden.add(
                    &format!("Problem {setting:?} {rung:?} in_role={in_role}"),
                    &render_grounded(&request),
                );
            }
        }
    }
    for setting in SETTINGS {
        let (problem, observation) = rich_observation(CodingTask::BugFix, setting);
        for rung in RUNGS {
            let request = request_for(&problem, &observation, rung, false);
            golden.add(
                &format!("BugFix {setting:?} {rung:?} in_role=false"),
                &render_grounded(&request),
            );
        }
    }
    let why = "A Nudge hint holds no fenced code block. Say it in words.";
    for (setting, rung, in_role) in [
        (CodingSetting::Mock, CodeHintRung::Nudge, false),
        (CodingSetting::Watched, CodeHintRung::Line, true),
    ] {
        let (problem, observation) = rich_observation(CodingTask::Problem, setting);
        let request = request_for(&problem, &observation, rung, in_role);
        let again = reask(request, REFUSED_ANSWER, why);
        golden.add(
            &format!("Asked again after a refusal: {setting:?} {rung:?} in_role={in_role}"),
            &render_grounded(&again),
        );
    }
    assert_golden("hints", include_str!("../golden/hints.txt"), golden.text());
}
