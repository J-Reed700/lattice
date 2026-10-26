//! Unit tests for the pure blueprint derivation in quiz_blueprints.rs.
use super::*;

fn facts() -> CourseFacts {
    let lesson = |id: &str, module: &str, prepared: bool| LessonFacts {
        id: id.into(),
        module_id: module.into(),
        title: format!("Lesson {id}"),
        objective: "Explain it".into(),
        prepared,
        outcome_id: prepared.then(|| format!("o-{id}")),
        source_version_ids: vec![],
        quiz_formats: vec![],
    };
    CourseFacts {
        program_title: "Cell biology".into(),
        modules: vec![ModuleFacts {
            id: "m1".into(),
            title: "Enzymes".into(),
            outcome_ids: vec!["mo1".into()],
        }],
        lessons: vec![lesson("l1", "m1", true), lesson("l2", "m1", false)],
    }
}

#[test]
fn lesson_quizzes_need_a_prepared_lesson_and_ask_seven_items() {
    let settings = default_settings();
    let scope = |id: &str| LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(id.into()),
    };
    assert!(blueprint_for(&scope("l2"), &facts(), &settings).is_none());
    let quiz = blueprint_for(&scope("l1"), &facts(), &settings).unwrap();
    let items: usize = quiz.slots.iter().map(|s| s.requirement.count).sum();
    assert_eq!(items, 7);
    assert_eq!(quiz.passing_score, 0.7);
    let without_written = LearningQuizSettingsDto {
        written_answers: false,
        ..settings
    };
    let quiz = blueprint_for(&scope("l1"), &facts(), &without_written).unwrap();
    assert!(quiz
        .slots
        .iter()
        .all(|s| s.requirement.format != LearningItemFormat::Explanation));
}

#[test]
fn a_lesson_keeps_the_quiz_mix_its_blueprint_already_asks() {
    use LearningItemFormat::*;
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some("l1".into()),
    };
    for structured in [MultiSelect, Matching, Ordering] {
        for typed in [ShortAnswer, Cloze] {
            let asked = vec![SingleChoice, TrueFalse, structured, typed, Explanation];
            let mut kept = facts();
            kept.lessons[0].quiz_formats = asked.clone();
            let quiz = blueprint_for(&scope, &kept, &default_settings()).unwrap();
            let formats: Vec<LearningItemFormat> =
                quiz.slots.iter().map(|s| s.requirement.format).collect();
            assert_eq!(formats, asked);
        }
    }
}

#[test]
fn a_new_lesson_quiz_mixes_in_more_typed_answers() {
    use LearningItemFormat::*;
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some("l1".into()),
    };
    let quiz = blueprint_for(&scope, &facts(), &default_settings()).unwrap();
    let count = |format: LearningItemFormat| -> usize {
        quiz.slots
            .iter()
            .filter(|s| s.requirement.format == format)
            .map(|s| s.requirement.count)
            .sum()
    };
    assert_eq!(count(TrueFalse), 0, "a coin-flip guess is not retrieval");
    assert_eq!(count(SingleChoice), 3);
    assert_eq!(
        (count(ShortAnswer), count(Cloze), count(Explanation)),
        (1, 1, 1)
    );
    assert_eq!(count(MultiSelect) + count(Matching) + count(Ordering), 1);
}

#[test]
fn a_lesson_quiz_asks_earlier_lessons_again() {
    use LearningItemFormat::*;
    let mut course = facts();
    for id in ["l3", "l4"] {
        let mut lesson = course.lessons[0].clone();
        lesson.id = id.into();
        lesson.outcome_id = Some(format!("o-{id}"));
        course.lessons.push(lesson);
    }
    // Course order: l1 (prepared), l2 (outline), l3, l4.
    let quiz = |id: &str| {
        let scope = LearningQuizScopeDto {
            kind: LearningQuizScopeKind::Lesson,
            id: Some(id.into()),
        };
        blueprint_for(&scope, &course, &default_settings()).unwrap()
    };
    // The single-choice items a lesson's quiz asks about other lessons.
    let reviewed = |id: &str| -> Vec<String> {
        quiz(id)
            .slots
            .iter()
            .map(|s| &s.requirement)
            .filter(|r| r.outcome_id != format!("o-{id}"))
            .map(|r| {
                assert_eq!((r.format, r.count), (SingleChoice, 1));
                r.outcome_id.clone()
            })
            .collect()
    };
    assert!(reviewed("l1").is_empty());
    assert!(!quiz("l1").instructions.contains("earlier lessons"));
    // The lesson before (an outline lesson is skipped), then one further back.
    assert_eq!(reviewed("l3"), ["o-l1"]);
    assert_eq!(reviewed("l4"), ["o-l3", "o-l1"]);
    assert!(quiz("l4").instructions.contains("earlier lessons"));
}

#[test]
fn module_tests_cover_prepared_lessons_and_say_so() {
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Module,
        id: Some("m1".into()),
    };
    let test = blueprint_for(&scope, &facts(), &default_settings()).unwrap();
    assert_eq!(test.coverage, Some((1, 2)));
    assert_eq!(test.passing_score, 0.75);
}
