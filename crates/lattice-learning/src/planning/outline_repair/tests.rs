use super::*;

#[test]
fn a_located_passage_past_a_quotations_ceiling_is_cited_by_the_chunk_it_was_built_around() {
    use crate::reference_collection::ReferencePassage;
    use lattice_core::application::services::claim_verification::ClaimVerdict;
    let excerpt = "A process records each observation with a stable identifier. ".repeat(30);
    let source = LearningSourceDto {
        id: "source".into(),
        title: "Guide".into(),
        url: None,
        excerpt: excerpt.clone(),
        acquired_at: 0,
    };
    let passage = |start: usize, end: usize| ReferencePassage {
        source_id: "source".into(),
        text: excerpt[start..end].to_owned(),
        start_byte: start,
        end_byte: end,
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
    };
    let located = passage(0, excerpt.len());
    let chunk = passage(305, 610);
    let found = super::evidence::locate(
        &located.text,
        ClaimVerdict::Supported,
        std::slice::from_ref(&located),
        std::slice::from_ref(&chunk),
        std::slice::from_ref(&source),
    );
    assert_eq!(
        found,
        Some(("source".to_owned(), chunk.text.trim().to_owned()))
    );
}

fn issue_at(path: &str) -> LearningOutlineIssueDto {
    LearningOutlineIssueDto {
        id: path.into(),
        path: path.into(),
        kind: LearningOutlineIssueKind::Plan,
        claim: String::new(),
        message: String::new(),
        attempts: 0,
        exhausted: false,
        quote: String::new(),
        source_id: None,
        searched: vec![],
    }
}

#[test]
fn bindings_and_open_findings_follow_the_lessons_to_their_new_positions() {
    let mut draft = OutlineDraft::new(
        crate::lessons::course_generation_tests::request(LearningCourseDepth::Course),
        json!({"modules":[]}),
        &[],
    );
    draft.bindings = Some(vec![
        OutlineModuleBinding {
            module_id: "m0".into(),
            lesson_ids: vec!["a".into(), "b".into()],
        },
        OutlineModuleBinding {
            module_id: "m1".into(),
            lesson_ids: vec!["c".into()],
        },
    ]);
    draft.review.issues = vec![
        issue_at("/modules/0"),
        issue_at("/modules/0/lessons/0"),
        issue_at("/modules/0/lessons/1/objective"),
        issue_at("/modules/1/lessons/0"),
    ];
    let lesson = |id: &str| LearningLessonDto {
        id: id.into(),
        title: id.into(),
        objective: id.into(),
        estimated_minutes: 30,
        preparation: LearningPreparation::Outline,
        blocks: vec![],
        completed: false,
    };
    let module = LearningModuleDto {
        id: "m0".into(),
        title: "M".into(),
        summary: "S".into(),
        outcomes: vec![],
        prerequisite_module_ids: vec![],
        project: None,
        lessons: vec![lesson("new"), lesson("a"), lesson("b")],
    };
    rewrite::follow_new_order(&mut draft, 0, &[None, Some(0), Some(1)], &module);
    let bindings = draft.bindings.as_ref().unwrap();
    assert_eq!(bindings[0].lesson_ids, vec!["new", "a", "b"]);
    assert_eq!(bindings[1].lesson_ids, vec!["c"]);
    let paths: Vec<_> = draft
        .review
        .issues
        .iter()
        .map(|i| i.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "/modules/0",
            "/modules/0/lessons/1",
            "/modules/0/lessons/2/objective",
            "/modules/1/lessons/0"
        ]
    );
}
