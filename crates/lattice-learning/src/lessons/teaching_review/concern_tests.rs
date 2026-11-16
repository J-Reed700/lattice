#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::tests::{issue, items, passage, reviewer, run};
use super::*;
use lattice_core::application::services::model_contract::shape::Vocabulary;

fn concerned(at: &str, concerns: &[&str]) -> Value {
    let concerns: Vec<Value> = concerns.iter().map(|c| json!({"concern":c})).collect();
    json!({"kind":"sectionDefect","passageId":at,"finding":"The section fails.","concerns":concerns,"continues":"none"})
}

fn kinds() -> Value {
    json!({"blocks":[
        {"kind":"explanation","body":"A plain idea is stated here."},
        {"kind":"worked_example","body":"Adding two and three gives five."}
    ]})
}

async fn lesson(answer: impl Fn(&Value) -> Vec<Value> + Send + Sync + 'static) -> Judged {
    let model = reviewer::<Concerns>(move |task, _| items(answer(task)));
    review(
        &model,
        "Review instructional quality.",
        json!({}),
        &kinds(),
        Scope::default(),
        Remit::Lesson,
    )
    .await
    .unwrap()
}

#[test]
fn the_issue_vocabulary_is_the_concerns_no_section_holds() {
    let words: Vec<LessonConcern> = IssueConcern::WORDS.iter().map(|(_, c)| c.0).collect();
    let expected: Vec<LessonConcern> = LessonConcern::ALL
        .into_iter()
        .filter(|concern| {
            !LearningBlockKind::ALL.iter().any(|k| concern.applies_to(k))
                || LearningBlockKind::ALL.iter().all(|k| concern.applies_to(k))
        })
        .collect();
    assert_eq!(words, expected);
    assert!(IssueConcern::WORDS
        .iter()
        .all(|(word, c)| *word == c.0.id()));
}

#[test]
fn the_check_vocabulary_is_the_concerns_some_section_kind_takes() {
    let words: Vec<LessonConcern> = CheckConcern::WORDS.iter().map(|(_, c)| c.0).collect();
    let expected: Vec<LessonConcern> = LessonConcern::ALL
        .into_iter()
        .filter(|concern| LearningBlockKind::ALL.iter().any(|k| concern.applies_to(k)))
        .collect();
    assert_eq!(words, expected);
    assert!(CheckConcern::WORDS
        .iter()
        .all(|(word, c)| *word == c.0.id()));
}

#[tokio::test]
async fn a_lesson_section_defect_keeps_only_a_concern_its_kind_takes() {
    let judged = lesson(|task| {
        vec![
            concerned(&passage(task, 0, 0), &["worked_example"]),
            concerned(&passage(task, 1, 0), &["worked_example"]),
        ]
    })
    .await;
    assert_eq!(judged.reported.len(), 1);
    assert_eq!(judged.reported[0].site, Some(FindingSite::Section(1)));
}

#[tokio::test]
async fn a_lesson_defect_that_names_no_concern_raises_no_finding() {
    let judged = lesson(|task| vec![concerned(&passage(task, 1, 0), &[])]).await;
    assert!(judged.reported.is_empty());
}

#[tokio::test]
async fn a_lesson_issue_is_read_with_the_concern_outside_every_section() {
    let judged = lesson(|_| {
        let mut promised = issue("The lesson promises that its tutor runs the learner's code.");
        promised["concern"] = json!("promise");
        vec![promised]
    })
    .await;
    assert_eq!(judged.reported.len(), 1);
    assert_eq!(judged.reported[0].site, None);
}

#[tokio::test]
async fn an_open_review_form_has_no_concern_field() {
    let model = reviewer::<Anything>(|_, _| items(vec![]));
    run(&model, Scope::default(), Remit::Open).await.unwrap();
    let sent = model.sent();
    let schema = sent
        .iter()
        .find_map(|s| {
            s.request
                .schema()
                .map(|schema| schema.as_json().to_string())
        })
        .unwrap();
    assert!(!schema.contains("concern"));
}
