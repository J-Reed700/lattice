use super::spans::{last_sentence_end, sentence_ends};
use super::*;
use crate::features::learning::dto::{GenerateLearningProgramRequestDto, LearningCourseDepth};
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use crate::features::learning::reference_collection::ReferencePassage;

const ASSIGNMENT: &str = "Random assignment limits confounding when treatment groups are compared.";
const UNCERTAINTY: &str =
    "Observed differences require an uncertainty estimate before causal interpretation.";

fn source(id: &str, title: &str, excerpt: &str) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: title.into(),
        url: None,
        excerpt: excerpt.into(),
        acquired_at: 0,
    }
}

fn sources() -> Vec<LearningSourceDto> {
    vec![
        source("assignment", "Assignment guide", ASSIGNMENT),
        source("uncertainty", "Uncertainty guide", UNCERTAINTY),
    ]
}

fn draft(candidate: Value) -> OutlineDraft {
    OutlineDraft::new(
        GenerateLearningProgramRequestDto {
            goal: "Design experiments".into(),
            prior_knowledge: String::new(),
            minutes_per_session: 30,
            document_ids: vec![],
            source_urls: vec![],
            course_depth: Some(LearningCourseDepth::Focused),
        },
        candidate,
        &sources(),
    )
}

/// Every item shares a word with both sources, so each is offered two
/// candidates, p1 (assignment) and p2 (uncertainty), in retrieval order.
fn candidate() -> Value {
    json!({"modules":[{
        "title":"Comparisons","prerequisiteIndices":[],
        "summary":{"text":"Compare treatment groups and interpret the observed differences.","sourceIndex":null,"quote":""},
        "outcomes":[
            {"text":"Explain how random assignment limits confounding when groups are compared.","sourceIndex":null,"quote":""},
            {"text":"Attach an uncertainty estimate to observed differences between groups.","sourceIndex":0,"quote":ASSIGNMENT}
        ],
        "lessons":[
            {"title":"Groups","objective":"Plan the treatment groups of an experiment and the observed outcome.","sourceIndex":1,"quote":"These words are not in the saved reference at all."}
        ]
    }]})
}

fn model(outputs: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: std::sync::Mutex::new(outputs.into_iter().map(|v| v.to_string()).collect()),
        prompts: std::sync::Mutex::new(vec![]),
    }
}

fn payload(prompt: &str) -> Value {
    serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap()
}

#[tokio::test]
async fn the_chooser_links_items_to_the_apps_own_passage_text() {
    let mut draft = draft(candidate());
    let llm = model(vec![json!({"links":[
        {"id":"/modules/0/summary","candidate":"p2","reason":"The passage addresses interpreting observed differences."},
        {"id":"/modules/0/outcomes/0","candidate":"p1","reason":"The passage explains random assignment and confounding."},
        {"id":"/modules/0/outcomes/1","candidate":"p2","reason":"The passage is about uncertainty estimates, not assignment."},
        {"id":"/modules/0/lessons/0","candidate":"none","reason":"Neither passage plans treatment groups."}
    ]})]);
    let log = link(
        &llm,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        log,
        LinkLog {
            linked: 3,
            items: 4,
            removed_unmatched: 1,
            removed_unchosen: 1
        }
    );
    let module = &draft.candidate["modules"][0];
    assert_eq!(module["summary"]["sourceIndex"], 1);
    assert_eq!(module["summary"]["quote"], UNCERTAINTY);
    assert_eq!(module["outcomes"][0]["sourceIndex"], 0);
    assert_eq!(module["outcomes"][0]["quote"], ASSIGNMENT);
    // The author's exact citation was offered and replaced by a better one.
    assert_eq!(module["outcomes"][1]["sourceIndex"], 1);
    assert_eq!(module["outcomes"][1]["quote"], UNCERTAINTY);
    // The author's inexact citation was removed before the chooser ran.
    assert_eq!(module["lessons"][0]["sourceIndex"], Value::Null);
    assert_eq!(module["lessons"][0]["quote"], "");
    assert_eq!(cited_count(&draft.candidate), 3);
    assert_eq!(item_count(&draft.candidate), 4);
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    let payload = payload(&prompts[0]);
    assert_eq!(payload["passages"][0]["passage"], ASSIGNMENT);
    assert_eq!(payload["passages"][1]["passage"], UNCERTAINTY);
    assert_eq!(payload["passages"].as_array().unwrap().len(), 2);
    assert_eq!(payload["items"][3]["lessonTitle"], "Groups");
    assert_eq!(payload["items"][3]["moduleTitle"], "Comparisons");
    assert!(payload["items"][1]["candidates"]
        .as_array()
        .unwrap()
        .contains(&json!("p1")));
    // Three positive receipts and one declined set.
    assert_eq!(draft.citation_receipts.len(), 4);
    assert_eq!(
        draft
            .citation_receipts
            .values()
            .filter(|r| r.covers)
            .count(),
        3
    );
}

#[tokio::test]
async fn receipts_make_a_second_run_free_and_only_changed_items_are_asked_again() {
    let mut draft = draft(candidate());
    let llm = model(vec![json!({"links":[
        {"id":"/modules/0/summary","candidate":"p2","reason":"ok"},
        {"id":"/modules/0/outcomes/0","candidate":"p1","reason":"ok"},
        {"id":"/modules/0/outcomes/1","candidate":"none","reason":"ok"},
        {"id":"/modules/0/lessons/0","candidate":"none","reason":"ok"}
    ]})]);
    link(
        &llm,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    // A rewrite re-attaches the author's declined citation; the declined
    // receipt and the linked items' receipts settle everything without a
    // call.
    draft.candidate["modules"][0]["outcomes"][1]["sourceIndex"] = json!(0);
    draft.candidate["modules"][0]["outcomes"][1]["quote"] = json!(ASSIGNMENT);
    let silent = model(vec![]);
    let log = link(
        &silent,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert!(silent.prompts.lock().unwrap().is_empty());
    assert_eq!(
        log,
        LinkLog {
            linked: 2,
            items: 4,
            removed_unmatched: 0,
            removed_unchosen: 1
        }
    );
    assert_eq!(
        draft.candidate["modules"][0]["summary"]["quote"],
        UNCERTAINTY
    );
    assert_eq!(
        draft.candidate["modules"][0]["outcomes"][1]["sourceIndex"],
        Value::Null
    );
    let hashes = source_hashes(&sources());
    assert!(is_verified(
        &draft.citation_receipts,
        silent.model_name(),
        ItemKind::Outcome,
        &draft.candidate["modules"][0]["outcomes"][0],
        &sources(),
        &hashes
    ));
    assert!(!is_verified(
        &draft.citation_receipts,
        "another-model",
        ItemKind::Outcome,
        &draft.candidate["modules"][0]["outcomes"][0],
        &sources(),
        &hashes
    ));
    // Changing one item's text reopens only that item. Its best-ranked
    // candidate (two matching terms) is the uncertainty passage, p1.
    draft.candidate["modules"][0]["lessons"][0]["objective"] =
        json!("Estimate the uncertainty of a difference between groups.");
    let again = model(vec![json!({"links":[
        {"id":"/modules/0/lessons/0","candidate":"p1","reason":"The passage addresses uncertainty estimates."}
    ]})]);
    let log = link(
        &again,
        &mut draft,
        &sources(),
        Some(0),
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert_eq!(log.linked, 3);
    let prompts = again.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(payload(&prompts[0])["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        draft.candidate["modules"][0]["lessons"][0]["quote"],
        UNCERTAINTY
    );
}

#[tokio::test]
async fn an_unusable_decision_leaves_only_that_item_unlinked_and_never_fails() {
    // A repeated id (first counts) and a missing id (outcomes/0) in one answer.
    let mut draft = draft(candidate());
    let llm = model(vec![json!({"links":[
        {"id":"/modules/0/summary","candidate":"p1","reason":"ok"},
        {"id":"/modules/0/summary","candidate":"p2","reason":"ok"},
        {"id":"/modules/0/outcomes/1","candidate":"p2","reason":"ok"},
        {"id":"/modules/0/lessons/0","candidate":"none","reason":"ok"}
    ]})]);
    link(
        &llm,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    let module = &draft.candidate["modules"][0];
    assert_eq!(module["summary"]["quote"], ASSIGNMENT);
    assert_eq!(module["outcomes"][0]["sourceIndex"], Value::Null);
    assert_eq!(module["outcomes"][1]["quote"], UNCERTAINTY);
    assert_eq!(module["lessons"][0]["sourceIndex"], Value::Null);
    // Two positive receipts and one declined set; the undecided item has
    // none, so the next linking run asks about it again.
    assert_eq!(draft.citation_receipts.len(), 3);
    // One source only: the summary's one candidate is p1; p2 names no
    // listed passage, so the summary stays unlinked without a receipt.
    let mut draft = draft_with(
        json!({"modules":[{"title":"Comparisons","prerequisiteIndices":[],
        "summary":{"text":"Compare treatment groups.","sourceIndex":null,"quote":""},"outcomes":[],"lessons":[]}]}),
    );
    let llm = model(vec![json!({"links":[
        {"id":"/modules/0/summary","candidate":"p2","reason":"ok"}
    ]})]);
    link(
        &llm,
        &mut draft,
        &sources()[..1],
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        draft.candidate["modules"][0]["summary"]["sourceIndex"],
        Value::Null
    );
    assert!(draft.citation_receipts.is_empty());
    // An unreadable answer is the same: nothing linked, nothing failed.
    let mut unreadable = super::tests::draft(candidate());
    let llm = model(vec![json!({"unexpected":true})]);
    link(
        &llm,
        &mut unreadable,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert!(unreadable.citation_receipts.is_empty());
}

#[tokio::test]
async fn an_author_fragment_is_not_offered_and_the_whole_sentence_replaces_it() {
    let mut draft = draft(
        json!({"modules":[{"title":"Comparisons","prerequisiteIndices":[],
        "summary":{"text":"Explain why random assignment limits confounding.","sourceIndex":0,"quote":"Random assignment limits confounding when treatment groups"},
        "outcomes":[],"lessons":[]}]}),
    );
    let llm = model(vec![json!({"links":[
        {"id":"/modules/0/summary","candidate":"p1","reason":"The passage explains random assignment and confounding."}
    ]})]);
    let log = link(
        &llm,
        &mut draft,
        &sources()[..1],
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        log,
        LinkLog {
            linked: 1,
            items: 1,
            removed_unmatched: 0,
            removed_unchosen: 1
        }
    );
    // The author's exact but unfinished sentence is not evidence a reader
    // can use, so only retrieval's whole sentence is offered.
    let payload = payload(&llm.prompts.lock().unwrap()[0]);
    assert_eq!(payload["items"][0]["candidates"], json!(["p1"]));
    let summary = &draft.candidate["modules"][0]["summary"];
    assert_eq!(summary["sourceIndex"], 0);
    assert_eq!(summary["quote"], ASSIGNMENT);
    let hashes = source_hashes(&sources()[..1]);
    assert!(is_verified(
        &draft.citation_receipts,
        llm.model_name(),
        ItemKind::Summary,
        summary,
        &sources()[..1],
        &hashes
    ));
}

fn draft_with(candidate: Value) -> OutlineDraft {
    draft(candidate)
}

#[test]
fn quotes_are_whole_sentences_within_the_limit_and_never_fragments() {
    let sentence = "Random assignment limits confounding when treatment groups are compared. ";
    let excerpt = format!("Intro line.\n{}Tail sentence here.", sentence.repeat(30));
    let start = excerpt.find("Random").unwrap();
    let passage = |start: usize, end: usize| ReferencePassage {
        source_id: "s".into(),
        text: excerpt[start..end].to_owned(),
        start_byte: start,
        end_byte: end,
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
        branch: None,
        branch_score: 0.0,
    };
    // Too long: trimmed to whole sentences under the limit.
    let quote = citable_span(
        &passage(start, excerpt.len()),
        &excerpt,
        "random assignment",
    )
    .unwrap();
    assert!(quote.chars().count() <= generation::MAX_QUOTE_CHARS);
    assert!(quote.ends_with("compared."));
    assert!(quote.starts_with("Random assignment"));
    generation::validate_excerpt_quote(&quote, &excerpt).unwrap();
    // Cut mid-sentence at both ends: the partial sentences are dropped.
    let mid_start = start + 7;
    let mid_end = start + sentence.len() * 3 + 10;
    let quote = citable_span(&passage(mid_start, mid_end), &excerpt, "random assignment").unwrap();
    assert!(quote.starts_with("Random assignment"));
    assert!(quote.ends_with("compared."));
    assert_eq!(quote.matches("compared.").count(), 2);
    // Too short to cite.
    assert!(citable_span(&passage(0, 11), &excerpt, "intro").is_none());
    // A closing quotation mark stays with its sentence.
    let quoted = "He said \"compare the groups fairly.\" Then he left the room quietly.";
    let short = ReferencePassage {
        source_id: "s".into(),
        text: quoted.into(),
        start_byte: 0,
        end_byte: quoted.len(),
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
        branch: None,
        branch_score: 0.0,
    };
    assert_eq!(last_sentence_end(quoted), Some(quoted.len()));
    assert_eq!(
        sentence_ends(quoted).next(),
        Some(quoted.find(" Then").unwrap())
    );
    assert_eq!(
        citable_span(&short, quoted, "compare groups").unwrap(),
        quoted
    );
}

#[test]
fn quotes_are_prose_and_never_headings_navigation_tables_or_link_lists() {
    let excerpt = "[Edit](https://example.org/edit/page.md)\n\n# Comparing groups\n\n| [Prev](https://example.org/a) | [Up](https://example.org/b) | Part II | [Home](https://example.org/) |\n| --- | --- | --- | --- |\n\n- [Assignment](https://example.org/assignment)\n- [Uncertainty](https://example.org/uncertainty)\n\nRandom assignment limits confounding when treatment groups are compared, as the [design guide](https://example.org/guide) explains.\nIt also makes the comparison fair across every measured and unmeasured trait.\n\n```\nassign(groups, seed=42);\n```\n\nConfidence intervals describe the uncertainty of an estimated difference between the groups.";
    let passage = ReferencePassage {
        source_id: "s".into(),
        text: excerpt.into(),
        start_byte: 0,
        end_byte: excerpt.len(),
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
        branch: None,
        branch_score: 0.0,
    };
    // The run sharing the item's terms wins; it keeps its inline link text.
    let quote = citable_span(
        &passage,
        excerpt,
        "random assignment confounding fair comparison",
    )
    .unwrap();
    assert!(quote.starts_with("Random assignment limits confounding"));
    assert!(quote.ends_with("unmeasured trait."));
    assert!(
        !quote.contains("Prev") && !quote.contains("# Comparing") && !quote.contains("assign(")
    );
    generation::validate_excerpt_quote(&quote, excerpt).unwrap();
    let quote = citable_span(&passage, excerpt, "confidence intervals uncertainty").unwrap();
    assert!(quote.starts_with("Confidence intervals"));
    // A page of only chrome yields no candidate at all.
    let chrome = "# What's new in the framework release notes overview\n\n| [Prev](a) | [Up](b) | [Home](c) |\n- [Edit](https://example.org/edit)";
    let only = ReferencePassage {
        text: chrome.into(),
        end_byte: chrome.len(),
        ..passage.clone()
    };
    assert!(citable_span(&only, chrome, "framework release notes").is_none());
    assert!(!is_prose(
        "What's new in ASP.NET Core in the latest release"
    ));
    assert!(is_prose("- Value objects can include methods that encapsulate domain logic, but those methods shouldn't produce side effects."));
}

#[tokio::test]
async fn sixty_four_items_are_linked_in_eight_calls_that_each_fit() {
    use crate::features::learning::lessons::course_generation_tests::LINKS_SENTINEL;
    let item = |i: usize| json!({"text":format!("Explain random assignment and uncertainty in comparison {i}."),"sourceIndex":null,"quote":""});
    let modules: Vec<Value> = (0..8)
        .map(|m| json!({
            "title":format!("Comparisons {m}"),"prerequisiteIndices":[],
            "summary":item(m * 8),
            "outcomes":(1..4).map(|i| item(m * 8 + i)).collect::<Vec<_>>(),
            "lessons":(4..8).map(|i| json!({"title":format!("Lesson {m}.{i}"),"objective":format!("Plan random assignment with an uncertainty estimate, case {}.", m * 8 + i),"sourceIndex":null,"quote":""})).collect::<Vec<_>>()
        }))
        .collect();
    let mut draft = draft(json!({ "modules": modules }));
    let llm = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![format!("{LINKS_SENTINEL}{{}}}}")].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let log = link(
        &llm,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert_eq!(log.items, 64);
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 8, "batches of eight, not one call for all");
    for prompt in prompts.iter() {
        let request = payload(prompt);
        let items = request["items"].as_array().unwrap();
        assert_eq!(items.len(), 8);
        assert!(items
            .iter()
            .all(|i| !i["candidates"].as_array().unwrap().is_empty()));
        assert!(fits(&llm, &request.to_string(), 200 + 120 * items.len()));
    }
}
