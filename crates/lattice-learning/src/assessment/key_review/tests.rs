#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::scripted::{answering, solved, solved_text};
use super::*;
use crate::lessons::course_generation_tests::RequestLog;
use crate::lessons::structured_call::task_messages;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::ScriptedModel;

const WORKING: &str = "Torn stamps are set aside.";

fn candidate(
    format: LearningItemFormat,
    prompt: &str,
    options: &[&str],
    key: Key,
) -> LearningAssessmentCandidate {
    LearningAssessmentCandidate {
        id: "item".into(),
        outcome_ids: vec![],
        format,
        difficulty: 3,
        prompt: prompt.into(),
        options: options.iter().map(|option| (*option).to_owned()).collect(),
        left: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key,
        explanation: "Alpha follows.".into(),
        distractor_rationales: vec![],
    }
}

fn choice(prompt: &str) -> LearningAssessmentCandidate {
    candidate(
        LearningItemFormat::SingleChoice,
        prompt,
        &["Alpha", "Bravo", "Charlie"],
        Key::Choice(0),
    )
}

fn explanation(prompt: &str) -> LearningAssessmentCandidate {
    candidate(LearningItemFormat::Explanation, prompt, &[], Key::Rubric)
}

fn cloze() -> LearningAssessmentCandidate {
    candidate(
        LearningItemFormat::Cloze,
        "The kettle boils at {{1}} degrees and the lid shuts after {{2}} seconds.",
        &[],
        Key::Cloze(vec![]),
    )
}

fn matching() -> LearningAssessmentCandidate {
    let mut item = candidate(
        LearningItemFormat::Matching,
        "Match each stamp to its place.",
        &["The tray", "The box"],
        Key::Matching(vec![1, 0]),
    );
    item.left = vec!["A torn stamp".into(), "A whole stamp".into()];
    item
}

fn ordering() -> LearningAssessmentCandidate {
    candidate(
        LearningItemFormat::Ordering,
        "Put the steps in order.",
        &["Weigh", "Stamp", "Post"],
        Key::Ordering(vec!["Weigh".into(), "Stamp".into(), "Post".into()]),
    )
}

/// The reply with the entry named `of` left out.
fn without(mut reply: Value, of: &str) -> Value {
    if let Some(entries) = reply["entries"].as_array_mut() {
        entries.retain(|entry| entry["of"] != of);
    }
    reply
}

/// A model that answers every question of every form.
fn everything() -> ScriptedModel {
    let model = answering::<SolvePicks>(ScriptedModel::new(), |task, _| {
        solved(task, |_| vec![0])
    });
    let model = answering::<SolveText>(model, |task, _| solved_text(task, "Because it follows."));
    let model = answering::<SolveBlanks>(model, |task, _| {
        let entries: Vec<Value> = task["questions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|question| {
                let blanks: Vec<Value> = question["blanks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|blank| json!({"of": blank["id"], "answer": "100"}))
                    .collect();
                json!({"of": question["id"], "reason": WORKING, "blanks": blanks})
            })
            .collect();
        json!({"entries": entries})
    });
    let model = answering::<SolveMatches>(model, |task, _| {
        let entries: Vec<Value> = task["questions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|question| {
                let pairs = json!([
                    {"of": question["left"][0]["id"], "matches": question["options"][1]["id"]},
                    {"of": question["left"][1]["id"], "matches": question["options"][0]["id"]}
                ]);
                json!({"of": question["id"], "reason": WORKING, "pairs": pairs})
            })
            .collect();
        json!({"entries": entries})
    });
    answering::<SolveOrder>(model, |task, _| {
        let options = &task["question"]["options"];
        json!({"order": [options[1]["id"], options[0]["id"], options[2]["id"]]})
    })
}

fn told(marks: &mut Marks, format_at: usize) -> Question {
    Question {
        at: marks.reference(100_000 + format_at),
        way: Way::Picks,
        format: marks.told(LearningItemFormat::SingleChoice),
        prompt: marks.told("Which option follows from the premises?".to_owned()),
        quote: marks.told("The premises are given before the options.".to_owned()),
        options: Vec::new(),
        left: Vec::new(),
        blanks: Vec::new(),
    }
}

fn questions(question: Question) -> Questions {
    Questions {
        questions: vec![question],
    }
}

#[test]
fn the_choice_solve_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let mut question = told(&mut marks, 1);
    question.options = vec![
        marks.told("Alpha is the first letter.".to_owned()),
        marks.told("Bravo is the second letter.".to_owned()),
    ];
    holds::<SolvePicks>(&questions(question), &marks);
}

#[test]
fn the_text_solve_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let question = told(&mut marks, 2);
    holds::<SolveText>(&questions(question), &marks);
}

#[test]
fn the_cloze_solve_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let mut question = told(&mut marks, 3);
    question.blanks = vec![Told("{{1}}".to_owned())];
    holds::<SolveBlanks>(&questions(question), &marks);
}

#[test]
fn the_matching_solve_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let mut question = told(&mut marks, 4);
    question.left = vec![marks.told("A torn stamp arrives.".to_owned())];
    question.options = vec![marks.told("The repair tray.".to_owned())];
    holds::<SolveMatches>(&questions(question), &marks);
}

#[test]
fn the_ordering_solve_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let mut question = told(&mut marks, 5);
    question.options = vec![
        marks.told("Weigh the parcel.".to_owned()),
        marks.told("Stamp the parcel.".to_owned()),
    ];
    holds::<SolveOrder>(&questions(question), &marks);
}

#[tokio::test]
async fn prompt_snapshot_quiz_key_solve() {
    let inner = everything();
    let model = RequestLog::new(&inner);
    let items = [
        choice("Which option follows from the premises?"),
        explanation("Explain what follows from the premises."),
        cloze(),
        matching(),
        ordering(),
    ];
    let pairs: Vec<(&LearningAssessmentCandidate, &str)> =
        items.iter().map(|item| (item, "Reference quote.")).collect();

    solve_items(&model, &pairs).await.unwrap();

    model.assert_snapshot::<SolvePicks>();
    model.assert_snapshot::<SolveText>();
    model.assert_snapshot::<SolveBlanks>();
    model.assert_snapshot::<SolveMatches>();
    model.assert_snapshot::<SolveOrder>();
}

#[tokio::test]
async fn every_answer_form_is_read() {
    let model = everything();
    let items = [
        choice("Which option follows from the premises?"),
        explanation("Explain what follows from the premises."),
        cloze(),
        matching(),
        ordering(),
    ];
    let pairs: Vec<(&LearningAssessmentCandidate, &str)> =
        items.iter().map(|item| (item, "Reference quote.")).collect();

    let solutions = solve_items(&model, &pairs).await.unwrap();

    let answers: Vec<Option<Answer>> = solutions
        .into_iter()
        .map(|solution| solution.map(|solution| solution.answer))
        .collect();
    assert_eq!(
        answers,
        vec![
            Some(Answer::Picks(vec![0])),
            Some(Answer::Text("Because it follows.".into())),
            Some(Answer::Blanks(vec!["100".into(), "100".into()])),
            Some(Answer::Matches(vec![1, 0])),
            Some(Answer::Order(vec![
                "Stamp".into(),
                "Weigh".into(),
                "Post".into()
            ])),
        ]
    );
}

#[tokio::test]
async fn questions_of_one_form_are_asked_together_and_forms_apart() {
    let model = everything();
    let items = [
        choice("Which option follows from the premises?"),
        choice("Which option holds for the second case?"),
        explanation("Explain what follows from the premises."),
    ];
    let pairs: Vec<(&LearningAssessmentCandidate, &str)> =
        items.iter().map(|item| (item, "Reference quote.")).collect();

    solve_items(&model, &pairs).await.unwrap();

    assert_eq!(model.count::<SolvePicks>(), 1);
    assert_eq!(model.count::<SolveText>(), 1);
    assert_eq!(model.inputs::<SolvePicks>()[0].questions.len(), 2);
}

#[tokio::test]
async fn the_options_of_each_question_are_labelled_from_the_first() {
    let model = everything();
    let items = [
        choice("Which option follows from the premises?"),
        choice("Which option holds for the second case?"),
    ];
    let pairs: Vec<(&LearningAssessmentCandidate, &str)> =
        items.iter().map(|item| (item, "Reference quote.")).collect();

    solve_items(&model, &pairs).await.unwrap();

    let sent = &model.sent()[0];
    let task: Value = serde_json::from_str(task_messages(&sent.request).1).unwrap();
    assert_eq!(task["questions"][0]["id"], "q1");
    assert_eq!(task["questions"][1]["id"], "q2");
    assert_eq!(task["questions"][0]["options"][0]["id"], "o1");
    assert_eq!(task["questions"][1]["options"][0]["id"], "o1");
}

#[tokio::test]
async fn a_missing_entry_is_asked_for_again_alone() {
    let second = "Which option holds for the second case?";
    let model = answering::<SolvePicks>(ScriptedModel::new(), |task, nth| {
        let reply = solved(task, |_| vec![1]);
        if nth == 0 {
            without(reply, "q2")
        } else {
            reply
        }
    });
    let items = [choice("Which option follows from the premises?"), choice(second)];
    let pairs: Vec<(&LearningAssessmentCandidate, &str)> =
        items.iter().map(|item| (item, "Reference quote.")).collect();

    let solutions = solve_items(&model, &pairs).await.unwrap();

    assert!(solutions.iter().all(Option::is_some));
    let asked = model.inputs::<SolvePicks>();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[1].questions.len(), 1);
    assert_eq!(asked[1].questions[0].prompt.0, second);
}

#[tokio::test]
async fn an_empty_reason_is_corrected() {
    let model = answering::<SolvePicks>(ScriptedModel::new(), |task, nth| {
        let mut reply = solved(task, |_| vec![0]);
        if nth == 0 {
            reply["entries"][0]["reason"] = json!("  ");
        }
        reply
    });
    let item = choice("Which option follows from the premises?");

    let solutions = solve_items(&model, &[(&item, "Reference quote.")])
        .await
        .unwrap();

    assert_eq!(solutions[0].as_ref().unwrap().reason, WORKING);
    assert!(model.count::<SolvePicks>() >= 2);
}

#[tokio::test]
async fn an_item_that_stays_unanswered_has_no_solution_and_the_rest_stand() {
    let model = answering::<SolvePicks>(ScriptedModel::new(), |task, _| {
        without(solved(task, |_| vec![0]), "q2")
    });
    let items = [
        choice("Which option follows from the premises?"),
        choice("Which option holds for the second case?"),
    ];
    let pairs: Vec<(&LearningAssessmentCandidate, &str)> =
        items.iter().map(|item| (item, "Reference quote.")).collect();

    let solutions = solve_items(&model, &pairs).await.unwrap();

    assert!(solutions[0].is_some());
    assert!(solutions[1].is_none());
}

#[tokio::test]
async fn a_solvers_reason_reaches_the_repair_whole() {
    let long = "The literal question follows from its premises and the option fits. ".repeat(40);
    let item = choice("Which option follows from the premises?");
    let model = answering::<SolvePicks>(ScriptedModel::new(), {
        let long = long.clone();
        move |task, _| {
            let mut reply = solved(task, |_| vec![1]);
            reply["entries"][0]["reason"] = json!(long);
            reply
        }
    });

    let solutions = solve_items(&model, &[(&item, "Reference quote.")])
        .await
        .unwrap();

    match compare(&item, solutions[0].as_ref().unwrap()) {
        KeyVerdict::Disputed(reason) => assert!(reason.contains(long.trim())),
        other => panic!("expected a dispute, got {other:?}"),
    }
}

#[test]
fn an_ordering_solve_with_no_reasoning_disputes_by_its_answer_alone() {
    let item = ordering();
    let solution = Solution {
        reason: String::new(),
        answer: Answer::Order(vec!["Stamp".into(), "Weigh".into(), "Post".into()]),
    };

    assert_eq!(
        compare(&item, &solution),
        KeyVerdict::Disputed("An independent solution answered order [\"Stamp\", \"Weigh\", \"Post\"]".into())
    );
}

#[test]
fn a_rubric_item_has_no_key_to_dispute() {
    let item = explanation("Explain what follows from the premises.");
    let solution = Solution {
        reason: WORKING.into(),
        answer: Answer::Text("Anything at all.".into()),
    };

    assert_eq!(compare(&item, &solution), KeyVerdict::Agreed);
}

#[test]
fn a_multi_select_key_agrees_with_the_same_options_in_any_order() {
    let item = candidate(
        LearningItemFormat::MultiSelect,
        "Which options follow?",
        &["Alpha", "Bravo", "Charlie"],
        Key::Choices(BTreeSet::from([0, 2])),
    );
    let agreeing = Solution {
        reason: WORKING.into(),
        answer: Answer::Picks(vec![2, 0]),
    };
    let missing = Solution {
        reason: WORKING.into(),
        answer: Answer::Picks(vec![0]),
    };

    assert_eq!(compare(&item, &agreeing), KeyVerdict::Agreed);
    assert!(matches!(compare(&item, &missing), KeyVerdict::Disputed(_)));
}
