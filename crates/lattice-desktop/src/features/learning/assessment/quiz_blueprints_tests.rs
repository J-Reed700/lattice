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
fn lesson_quizzes_need_a_prepared_lesson_and_ask_eight_items() {
    let settings = default_settings();
    let scope = |id: &str| LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(id.into()),
    };
    assert!(blueprint_for(&scope("l2"), &facts(), &settings).is_none());
    let quiz = blueprint_for(&scope("l1"), &facts(), &settings).unwrap();
    let items: usize = quiz.slots.iter().map(|s| s.requirement.count).sum();
    assert_eq!(items, 8);
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
fn module_tests_cover_prepared_lessons_and_say_so() {
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Module,
        id: Some("m1".into()),
    };
    let test = blueprint_for(&scope, &facts(), &default_settings()).unwrap();
    assert_eq!(test.coverage, Some((1, 2)));
    assert_eq!(test.passing_score, 0.75);
}
