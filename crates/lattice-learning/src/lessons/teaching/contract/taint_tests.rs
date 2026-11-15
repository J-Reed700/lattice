//! What the check's asks keep out of their requests, and what they must say.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::asks::{
    Course, Input, LessonContractCheck, LessonContractFind, LessonContractReports, Planned,
    Reported, ReportsInput,
};
use super::confirm::{ConfirmPlanned, ConfirmUse, Earlier, Gave, Given, PlannedInput, UseInput};
use super::sentence::{Lesson, Spot, Text, View};
use lattice_core::application::services::model_contract::shown::{Internal, Told};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use serde_json::json;

/// A lesson of one section of two sentences, and one question of one; the section can be named as a place when `named`.
fn lesson(marks: &mut Marks, named: bool) -> Lesson {
    let view = |marks: &mut Marks, order: usize, title: &str, body: &str, spot: Option<usize>| {
        let sentences = [
            format!("{body} comes first."),
            format!("{body} comes second."),
        ];
        for sentence in &sentences {
            marks.told(sentence.clone());
        }
        View {
            order: marks.internal(order),
            frame: marks
                .told(json!({"kind": "explanation", "title": title, "bodyOfTheSection": null})),
            texts: vec![Text {
                at: marks.internal("/bodyOfTheSection".to_owned()),
                of: marks.internal(100_002),
                words: Told(sentences.join(" ")),
            }],
            spot: spot.map(|at| Spot {
                at: marks.reference(at),
            }),
        }
    };
    let section = view(
        marks,
        100_001,
        "Opening section title",
        "Typed commands",
        named.then_some(100_005),
    );
    let question = view(
        marks,
        100_006,
        "Practice question title",
        "Listed names",
        None,
    );
    Lesson {
        sections: vec![section],
        questions: vec![question],
    }
}

fn course(marks: &mut Marks) -> Course {
    Course {
        prior: marks.told(json!("The learner has used a file manager.")),
        title: marks.told(json!("Working in the terminal")),
        earlier: marks.told(json!([{"title": "Looking around", "actions": ["Listing the files in a folder"], "terms": ["the folder concept"]}])),
        later: marks.told(json!([{"title": "Making files", "ideas": [], "terms": [], "actions": ["Creating a file"]}])),
        module: marks.told(json!("Shell basics")),
        outline: marks.told(json!([{"title": "Shell basics", "outcomes": ["Run commands"], "milestone": "A logbook of daily shell habits", "lessonTitles": ["Looking around"]}])),
    }
}

fn planned(marks: &mut Marks) -> Vec<Planned> {
    vec![Planned {
        id: marks.internal("planned-item-identity-1".to_owned()),
        kind: marks.told("action step".to_owned()),
        text: marks.told("Opening the terminal application".to_owned()),
    }]
}

fn reported(marks: &mut Marks) -> Vec<Reported> {
    vec![Reported {
        at: marks.reference(100_007),
        part: Some(marks.told("Section 1".to_owned())),
        defect: marks.told("The section promises what the lesson never teaches.".to_owned()),
        words: Some(marks.told("promises what the lesson".to_owned())),
    }]
}

#[test]
fn the_planned_ask_keeps_what_is_internal_out_and_says_what_the_model_must_know() {
    let mut marks = Marks::new();
    let input = Input {
        course: course(&mut marks),
        lesson: lesson(&mut marks, false),
        planned: planned(&mut marks),
        reported: Vec::new(),
    };
    holds::<LessonContractCheck>(&input, &marks);
}

#[test]
fn the_find_ask_keeps_what_is_internal_out_and_says_what_the_model_must_know() {
    let mut marks = Marks::new();
    let input = Input {
        course: course(&mut marks),
        lesson: lesson(&mut marks, false),
        planned: Vec::new(),
        reported: reported(&mut marks),
    };
    holds::<LessonContractFind>(&input, &marks);
}

#[test]
fn the_reports_ask_keeps_what_is_internal_out_and_says_what_the_model_must_know() {
    let mut marks = Marks::new();
    let input = ReportsInput {
        lesson: lesson(&mut marks, false),
        reported: reported(&mut marks),
    };
    holds::<LessonContractReports>(&input, &marks);
}

#[test]
fn the_confirming_read_of_a_planned_item_keeps_what_is_internal_out_and_says_what_the_model_must_know(
) {
    let mut marks = Marks::new();
    let input = PlannedInput {
        lesson: lesson(&mut marks, true),
        kind: marks.told("action step".to_owned()),
        text: marks.told("Opening the terminal application".to_owned()),
    };
    holds::<ConfirmPlanned>(&input, &marks);
}

#[test]
fn the_confirming_read_of_a_use_keeps_what_is_internal_out_and_says_what_the_model_must_know() {
    let mut marks = Marks::new();
    let lesson = lesson(&mut marks, false);
    let at = lesson
        .texts()
        .iter()
        .next()
        .and_then(|(_, units)| units.spans().get(1).map(|(unit, _)| *unit))
        .unwrap();
    marks.reference(serde_json::to_value(at).unwrap()["text"].clone());
    let given = |marks: &mut Marks, given: usize, words: &str| Given {
        at: marks.reference(Gave {
            lesson: 100_009,
            given,
        }),
        words: marks.told(words.to_owned()),
    };
    let input = UseInput {
        lesson,
        prior: Some(marks.told("The learner has used a file manager.".to_owned())),
        earlier: vec![Earlier {
            title: Some(given(&mut marks, 100_010, "Looking around")),
            actions: vec![given(&mut marks, 100_011, "Listing the files in a folder")],
            terms: Vec::new(),
        }],
        at: Internal::of(at),
        what: marks.told("the terminal application".to_owned()),
    };
    holds::<ConfirmUse>(&input, &marks);
}
