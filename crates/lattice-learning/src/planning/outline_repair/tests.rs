use super::rewrite::{preserve_valid_citations, strip_new_citations};
use super::*;
use crate::outline_linking::{receipt_key, CitationReceipts};

#[test]
fn citation_regressions_are_reverted_only_for_unchanged_claims_with_receipted_evidence() {
    let quote = "The `for` loop visits each item in the collection.";
    let source = LearningSourceDto {
        id: "source".into(),
        title: "Guide".into(),
        url: None,
        excerpt: quote.into(),
        acquired_at: 0,
    };
    let sources = std::slice::from_ref(&source);
    let hash = &outline_linking::source_hashes(sources)[0];
    let model = "test";
    let receipt = |kind: ItemKind, text: &str| {
        (
            receipt_key(model, kind, text, hash, quote),
            CitationReceipt {
                covers: true,
                reason: "Covers the item.".into(),
                checked_at: 0,
            },
        )
    };
    let receipts: CitationReceipts = [
        receipt(ItemKind::Summary, "A stable claim about loops."),
        receipt(ItemKind::Lesson, "Explain collection iteration."),
    ]
    .into_iter()
    .collect();
    let before = json!({"summary":{"text":"A stable claim about loops.","sourceIndex":0,"quote":quote},"outcomes":[{"text":"An unverified existing citation.","sourceIndex":0,"quote":quote}],"lessons":[{"objective":"Explain collection iteration.","sourceIndex":0,"quote":quote}]});
    let mut after = before.clone();
    after["summary"]["quote"] = json!("The for loop visits each item in the collection.");
    after["outcomes"][0]["sourceIndex"] = Value::Null;
    after["outcomes"][0]["quote"] = json!("");
    after["lessons"][0]["objective"] = json!("Explain how iteration skips every other item.");
    after["lessons"][0]["sourceIndex"] = Value::Null;
    after["lessons"][0]["quote"] = json!("");
    preserve_valid_citations(&before, &mut after, &[Some(0)], sources, &receipts, model);
    // Unchanged text with a receipted citation: the mangled quote is restored.
    assert_eq!(after["summary"]["quote"], quote);
    // Unchanged text whose old citation has no receipt: stays dropped.
    assert_eq!(after["outcomes"][0]["sourceIndex"], Value::Null);
    // Changed text never inherits the old citation.
    assert_eq!(after["lessons"][0]["sourceIndex"], Value::Null);
    assert_eq!(after["lessons"][0]["quote"], "");
}

#[test]
fn a_rewrite_keeps_the_citations_it_was_given_and_never_introduces_one() {
    let before = json!({"summary":{"text":"Kept.","sourceIndex":0,"quote":"An existing quote."},"outcomes":[{"text":"Plain.","sourceIndex":null,"quote":""}],"lessons":[{"objective":"Plain.","sourceIndex":null,"quote":""}]});
    let mut after = before.clone();
    after["outcomes"][0]["sourceIndex"] = json!(0);
    after["outcomes"][0]["quote"] = json!("A quote the rewrite added.");
    after["lessons"][0]["sourceIndex"] = json!(1);
    after["lessons"][0]["quote"] = json!("Another added quote.");
    strip_new_citations(&before, &mut after, &[Some(0)]);
    assert_eq!(after["summary"]["quote"], "An existing quote.");
    assert_eq!(after["outcomes"][0]["sourceIndex"], Value::Null);
    assert_eq!(after["outcomes"][0]["quote"], "");
    assert_eq!(after["lessons"][0]["sourceIndex"], Value::Null);
    assert_eq!(after["lessons"][0]["quote"], "");
}

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

fn lesson(title: &str, objective: &str, from: Value) -> Value {
    json!({"title":title,"objective":objective,"estimatedMinutes":30,"sourceIndex":null,"quote":"","from":from})
}

fn answered(froms: &[Value]) -> Value {
    json!({"lessons":froms.iter().enumerate().map(|(i, from)| lesson(&format!("Lesson {i}"), "Objective.", from.clone())).collect::<Vec<_>>()})
}

#[test]
fn an_answer_that_inserts_a_lesson_names_each_lessons_origin_and_loses_the_from_field() {
    let mut module = answered(&[json!(0), json!(null), json!(1), json!(2)]);
    let origins = rewrite::lesson_origins(&mut module, 3).unwrap();
    assert_eq!(origins, vec![Some(0), None, Some(1), Some(2)]);
    assert!(module["lessons"]
        .as_array()
        .unwrap()
        .iter()
        .all(|lesson| lesson.get("from").is_none()));
}

#[test]
fn an_answer_that_adds_at_either_end_is_accepted() {
    let mut first = answered(&[json!(null), json!(0), json!(1)]);
    assert_eq!(
        rewrite::lesson_origins(&mut first, 2).unwrap(),
        vec![None, Some(0), Some(1)]
    );
    let mut last = answered(&[json!(0), json!(1), json!(null), json!(null)]);
    assert_eq!(
        rewrite::lesson_origins(&mut last, 2).unwrap(),
        vec![Some(0), Some(1), None, None]
    );
}

#[test]
fn an_answer_that_drops_repeats_or_reorders_a_lesson_is_refused_with_a_reason() {
    for froms in [
        vec![json!(0), json!(1)],
        vec![json!(0), json!(0), json!(1)],
        vec![json!(1), json!(0), json!(2)],
        vec![json!(0), json!(null), json!(2)],
        vec![json!(null), json!(null), json!(null)],
        vec![json!(0), json!(1), json!(2), json!(3)],
    ] {
        let mut module = answered(&froms);
        let error = rewrite::lesson_origins(&mut module, 3).unwrap_err();
        assert!(
            matches!(&error, AppError::InvalidInput(reason) if reason.contains("kept once, in its original order")),
            "{froms:?}: {error:?}"
        );
    }
}

#[test]
fn a_lesson_without_a_usable_from_is_refused() {
    for from in [json!("0"), json!(-1), json!(1.5)] {
        let mut module = answered(&[from.clone(), json!(1)]);
        assert!(rewrite::lesson_origins(&mut module, 2).is_err(), "{from}");
    }
    let mut missing = answered(&[json!(0), json!(1)]);
    missing["lessons"][1]
        .as_object_mut()
        .unwrap()
        .remove("from");
    assert!(rewrite::lesson_origins(&mut missing, 2).is_err());
}

#[test]
fn the_repair_schema_asks_for_from_and_the_course_schema_does_not() {
    let outline = generation::outline_schema(Some(LearningCourseDepth::Course), 0);
    let module = outline
        .pointer("/properties/modules/items")
        .unwrap()
        .clone();
    let repair = rewrite::repair_schema(module.clone(), 3);
    let lessons = repair
        .pointer("/properties/module/properties/lessons/items")
        .unwrap();
    assert!(lessons["required"]
        .as_array()
        .unwrap()
        .contains(&json!("from")));
    assert!(lessons["properties"].get("from").is_some());
    assert_eq!(lessons["additionalProperties"], json!(false));
    let course = module.pointer("/properties/lessons/items").unwrap();
    assert!(course["properties"].get("from").is_none());
    assert!(!course["required"]
        .as_array()
        .unwrap()
        .contains(&json!("from")));
}

#[test]
fn a_kept_lesson_after_an_insert_keeps_its_verified_citation_and_a_new_one_gets_none() {
    let quote = "The `for` loop visits each item in the collection.";
    let source = LearningSourceDto {
        id: "source".into(),
        title: "Guide".into(),
        url: None,
        excerpt: quote.into(),
        acquired_at: 0,
    };
    let sources = std::slice::from_ref(&source);
    let hash = &outline_linking::source_hashes(sources)[0];
    let model = "test";
    let receipts: CitationReceipts = [(
        receipt_key(
            model,
            ItemKind::Lesson,
            "Explain collection iteration.",
            hash,
            quote,
        ),
        CitationReceipt {
            covers: true,
            reason: "Covers the item.".into(),
            checked_at: 0,
        },
    )]
    .into_iter()
    .collect();
    let before = json!({"summary":{"text":"S.","sourceIndex":null,"quote":""},"outcomes":[],"lessons":[
        {"objective":"Something else.","sourceIndex":null,"quote":""},
        {"objective":"Explain collection iteration.","sourceIndex":0,"quote":quote}]});
    let mut after = json!({"summary":{"text":"S.","sourceIndex":null,"quote":""},"outcomes":[],"lessons":[
        {"objective":"Something else.","sourceIndex":null,"quote":""},
        {"objective":"A new lesson.","sourceIndex":0,"quote":quote},
        {"objective":"Explain collection iteration.","sourceIndex":null,"quote":""}]});
    let origins = [Some(0), None, Some(1)];
    preserve_valid_citations(&before, &mut after, &origins, sources, &receipts, model);
    strip_new_citations(&before, &mut after, &origins);
    assert_eq!(after["lessons"][2]["sourceIndex"], 0);
    assert_eq!(after["lessons"][2]["quote"], quote);
    assert_eq!(after["lessons"][1]["sourceIndex"], Value::Null);
    assert_eq!(after["lessons"][1]["quote"], "");
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
