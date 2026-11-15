use super::spans::{last_sentence_end, sentence_ends};
use super::*;
use crate::dto::{GenerateLearningProgramRequestDto, LearningCourseDepth};
use crate::lessons::content_verification::tests::fixture_task;
use crate::lessons::content_verification::tests::{fixture_completion, fixture_prompts};
use crate::lessons::course_generation_tests::ScriptedModel;
use crate::reference_collection::ReferencePassage;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::shared::error::AppError;

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

/// An answer in the layer's form: `(item handle, passage handle or none, reason)`.
fn entries(picks: &[(&str, &str, &str)]) -> Value {
    json!({"entries": picks.iter().map(|(of, candidate, reason)| json!({"of": of, "candidate": candidate, "reason": reason})).collect::<Vec<_>>()})
}

/// The items a request shows, by the handle each is shown under.
fn shown_items(task: &Value) -> Vec<String> {
    task["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect()
}

type Reply = Box<dyn Fn(&Value, usize) -> Value + Send + Sync>;

/// Answers each request with `reply(task, nth)`, and keeps the prompts.
struct Answering {
    reply: Reply,
    prompts: std::sync::Mutex<Vec<String>>,
}

fn answering(reply: impl Fn(&Value, usize) -> Value + Send + Sync + 'static) -> Answering {
    Answering {
        reply: Box::new(reply),
        prompts: std::sync::Mutex::new(vec![]),
    }
}

#[async_trait::async_trait]
impl LLMPort for Answering {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            let nth = {
                let mut prompts = self.prompts.lock().unwrap();
                prompts.push(prompt.clone());
                prompts.len() - 1
            };
            let task = fixture_task(&prompt).unwrap_or(Value::Null);
            replies.push((self.reply)(&task, nth).to_string());
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "scripted-review"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn the_chooser_links_items_to_the_apps_own_passage_text() {
    let mut draft = draft(candidate());
    let llm = model(vec![entries(&[
        (
            "i1",
            "p2",
            "The passage addresses interpreting observed differences.",
        ),
        (
            "i2",
            "p1",
            "The passage explains random assignment and confounding.",
        ),
        (
            "i3",
            "p2",
            "The passage is about uncertainty estimates, not assignment.",
        ),
        ("i4", "none", "Neither passage plans treatment groups."),
    ])]);
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
    let payload = fixture_task(&prompts[0]).unwrap();
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
    let llm = model(vec![entries(&[
        ("i1", "p2", "ok"),
        ("i2", "p1", "ok"),
        ("i3", "none", "ok"),
        ("i4", "none", "ok"),
    ])]);
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
    let again = model(vec![entries(&[(
        "i1",
        "p1",
        "The passage addresses uncertainty estimates.",
    )])]);
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
    assert_eq!(
        fixture_task(&prompts[0]).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        draft.candidate["modules"][0]["lessons"][0]["quote"],
        UNCERTAINTY
    );
}

#[tokio::test]
async fn an_unusable_decision_leaves_only_that_item_unlinked_and_never_fails() {
    // Two differing entries for one item, and none for another that is never answered.
    let mut draft = draft(candidate());
    let llm = answering(|task, nth| {
        if nth == 0 {
            return entries(&[
                ("i1", "p1", "ok"),
                ("i1", "p2", "ok"),
                ("i3", "p2", "ok"),
                ("i4", "none", "ok"),
            ]);
        }
        json!({"entries": shown_items(task).iter().filter(|id| *id != "i2").map(|id| json!({"of": id, "candidate": "p1", "reason": "ok"})).collect::<Vec<_>>()})
    });
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
    // Two positive receipts and one declined set; the unanswered item has
    // none, so the next linking run asks about it again.
    assert_eq!(draft.citation_receipts.len(), 3);
    // One source only: the summary's one candidate is p1; p2 names no
    // passage shown, so the summary stays unlinked without a receipt.
    let mut draft = draft_with(
        json!({"modules":[{"title":"Comparisons","prerequisiteIndices":[],
        "summary":{"text":"Compare treatment groups.","sourceIndex":null,"quote":""},"outcomes":[],"lessons":[]}]}),
    );
    let llm = answering(|_, _| entries(&[("i1", "p2", "ok")]));
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
    let llm = answering(|_, _| json!({"unexpected":true}));
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
async fn an_item_no_decision_reaches_loses_its_author_citation_and_gets_no_receipt() {
    let mut draft = draft(candidate());
    let llm = answering(
        |task, _| json!({"entries": shown_items(task).iter().filter(|id| *id != "i3").map(|id| json!({"of": id, "candidate": "none", "reason": "ok"})).collect::<Vec<_>>()}),
    );
    let log = link(
        &llm,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    let kept = &draft.candidate["modules"][0]["outcomes"][1];
    assert_eq!(kept["sourceIndex"], Value::Null);
    assert_eq!(kept["quote"], "");
    assert_eq!(log.removed_unchosen, 1);
    // The three answered items are declined; the unanswered one has no receipt.
    assert_eq!(draft.citation_receipts.len(), 3);
}

#[tokio::test]
async fn an_entry_missing_from_an_answer_is_asked_for_again_alone_and_the_others_stand() {
    let mut draft = draft(candidate());
    let llm = answering(|_, nth| {
        if nth == 0 {
            return entries(&[("i1", "p2", "ok"), ("i2", "p1", "ok"), ("i3", "none", "ok")]);
        }
        entries(&[("i4", "none", "ok")])
    });
    link(
        &llm,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert_eq!(
        shown_items(&fixture_task(&prompts[1]).unwrap()),
        ["i4".to_owned()]
    );
    assert_eq!(draft.citation_receipts.len(), 4);
    assert_eq!(
        draft.candidate["modules"][0]["summary"]["quote"],
        UNCERTAINTY
    );
}

#[tokio::test]
async fn a_choice_without_a_reason_is_corrected_alone() {
    let mut draft = draft(
        json!({"modules":[{"title":"Comparisons","prerequisiteIndices":[],
        "summary":{"text":"Explain why random assignment limits confounding.","sourceIndex":null,"quote":""},
        "outcomes":[],"lessons":[]}]}),
    );
    let llm = answering(|_, nth| {
        entries(&[(
            "i1",
            "p1",
            if nth == 0 {
                " "
            } else {
                "It explains assignment."
            },
        )])
    });
    link(
        &llm,
        &mut draft,
        &sources()[..1],
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("Give the reason for this choice in reason."));
    assert_eq!(
        draft.candidate["modules"][0]["summary"]["quote"],
        ASSIGNMENT
    );
}

#[test]
fn a_room_that_holds_no_passage_of_an_item_leaves_the_item_out() {
    let told = |text: &str| Told(text.to_owned());
    let item = |at: usize, candidates: Vec<usize>| Item {
        at,
        kind: told("module summary"),
        module_title: told("Harbour"),
        text: told("Read the tide tables."),
        lesson_title: None,
        candidates: Internal::of(candidates),
    };
    let passage = |at: usize| Passage {
        at,
        title: told("Almanac"),
        quote: told("Tide tables are revised each spring."),
    };
    let input = Input {
        items: vec![item(0, vec![0]), item(1, vec![1])],
        passages: Packed::new(vec![passage(0), passage(1)], |_| 10),
    };
    let cut = OutlineLinking::within(&input, 10).unwrap();
    assert_eq!(cut.items.len(), 1);
    assert_eq!(cut.items[0].at, 0);
    assert_eq!(cut.passages.len(), 1);
}

fn plain_item(at: usize, candidates: Vec<usize>) -> Item {
    let told = |text: &str| Told(text.to_owned());
    Item {
        at,
        kind: told("module summary"),
        module_title: told("Harbour"),
        text: told("Read the tide tables."),
        lesson_title: None,
        candidates: Internal::of(candidates),
    }
}

fn plain_passages(count: usize) -> Vec<Passage> {
    (0..count)
        .map(|at| Passage {
            at,
            title: Told("Almanac".to_owned()),
            quote: Told(format!("Tide table {at} is revised each spring.")),
        })
        .collect()
}

#[test]
fn a_room_holding_two_of_five_items_shows_those_with_all_their_candidates() {
    let passages = plain_passages(10);
    let costs = vec![10; 10];
    let items: Vec<Item> = (0..5)
        .map(|at| plain_item(at, vec![at * 2, at * 2 + 1]))
        .collect();
    let open: Vec<&Item> = items.iter().collect();
    let first = OutlineLinking::within(&input_for(&open, &passages, &costs), 40).unwrap();
    assert_eq!(first.items.iter().map(|i| i.at).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(
        first.passages.iter().map(|p| p.at).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert!(first
        .items
        .iter()
        .all(|item| item.candidates.value().len() == 2));
    let rest: Vec<&Item> = items.iter().skip(2).collect();
    let later = OutlineLinking::within(&input_for(&rest, &passages, &costs), 1_000).unwrap();
    assert_eq!(later.items.len(), 3);
    assert_eq!(later.passages.len(), 6);
}

#[test]
fn one_item_whose_candidates_outgrow_the_room_is_offered_the_best_that_fit() {
    let passages = plain_passages(4);
    let costs = vec![10; 4];
    let items = [plain_item(0, vec![0, 1, 2, 3])];
    let open: Vec<&Item> = items.iter().collect();
    let cut = OutlineLinking::within(&input_for(&open, &passages, &costs), 25).unwrap();
    assert_eq!(cut.items.len(), 1);
    assert_eq!(cut.items[0].candidates.value(), &vec![0, 1]);
    assert_eq!(cut.passages.len(), 2);
    assert!(
        OutlineLinking::within(&input_for(&open, &passages, &costs), 0)
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn an_item_that_does_not_fit_is_passed_over_and_a_later_one_is_still_shown() {
    let passages = plain_passages(4);
    let costs = vec![10, 50, 10, 10];
    let items = [
        plain_item(0, vec![0]),
        plain_item(1, vec![1]),
        plain_item(2, vec![2]),
    ];
    let open: Vec<&Item> = items.iter().collect();
    let cut = OutlineLinking::within(&input_for(&open, &passages, &costs), 25).unwrap();
    assert_eq!(cut.items.iter().map(|i| i.at).collect::<Vec<_>>(), [0, 2]);
    assert_eq!(
        cut.passages.iter().map(|p| p.at).collect::<Vec<_>>(),
        [0, 2]
    );
}

#[test]
fn a_passage_two_items_share_is_counted_once() {
    let passages = plain_passages(3);
    let costs = vec![10; 3];
    let items = [
        plain_item(0, vec![0, 1]),
        plain_item(1, vec![1, 2]),
        plain_item(2, vec![0, 2]),
    ];
    let open: Vec<&Item> = items.iter().collect();
    let cut = OutlineLinking::within(&input_for(&open, &passages, &costs), 30).unwrap();
    assert_eq!(cut.items.len(), 3);
    assert_eq!(cut.passages.len(), 3);
}

#[tokio::test]
async fn an_items_candidates_do_not_shrink_as_the_outline_grows() {
    let outline = |count: usize| {
        let outcomes: Vec<Value> = (1..count)
            .map(|_| json!({"text":"Explain iron oxide reduction and its equilibrium.","sourceIndex":null,"quote":""}))
            .collect();
        draft(json!({"modules":[{"title":"Iron","prerequisiteIndices":[],
            "summary":{"text":"Explain iron oxide reduction and its equilibrium.","sourceIndex":null,"quote":""},
            "outcomes":outcomes,"lessons":[]}]}))
    };
    let mut offered = Vec::new();
    for count in [9, 30] {
        let llm = notation_model(8_192, |text| text.chars().count() / 4, json!({}));
        let mut draft = outline(count);
        link(
            &llm,
            &mut draft,
            &reaction_sources(),
            None,
            &OutlineProgress::default(),
        )
        .await
        .unwrap();
        let prompts = llm.inner.prompts.lock().unwrap();
        let task = fixture_task(&prompts[0]).unwrap();
        offered.push(task["items"][0]["candidates"].as_array().unwrap().len());
    }
    assert!(offered[0] > 0);
    assert_eq!(offered[0], offered[1]);
}

#[test]
fn the_prompt_names_nothing_the_app_owns() {
    let mut marks = Marks::new();
    let input = Input {
        items: vec![Item {
            at: marks.reference(100_001_usize),
            kind: marks.told("module summary".to_owned()),
            module_title: marks.told("Harbour tides".to_owned()),
            text: marks.told("Explain how the tide tables are read.".to_owned()),
            lesson_title: None,
            candidates: marks.internal(vec![100_002_usize]),
        }],
        passages: Packed::new(
            vec![Passage {
                at: marks.reference(100_002_usize),
                title: marks.told("Harbour almanac".to_owned()),
                quote: marks.told("Tide tables are revised each spring.".to_owned()),
            }],
            |_| 10,
        ),
    };
    holds::<OutlineLinking>(&input, &marks);
}

#[tokio::test]
async fn an_author_fragment_is_not_offered_and_the_whole_sentence_replaces_it() {
    let mut draft = draft(
        json!({"modules":[{"title":"Comparisons","prerequisiteIndices":[],
        "summary":{"text":"Explain why random assignment limits confounding.","sourceIndex":0,"quote":"Random assignment limits confounding when treatment groups"},
        "outcomes":[],"lessons":[]}]}),
    );
    let llm = model(vec![entries(&[(
        "i1",
        "p1",
        "The passage explains random assignment and confounding.",
    )])]);
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
    let payload = fixture_task(&llm.prompts.lock().unwrap()[0]).unwrap();
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
async fn sixty_four_items_are_all_decided_in_requests_that_each_show_candidates() {
    use crate::lessons::course_generation_tests::LINKS_SENTINEL;
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
    let prompts = llm.prompts.lock().unwrap().clone();
    for prompt in prompts.iter() {
        let request = fixture_task(prompt).unwrap();
        assert!(request["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| !i["candidates"].as_array().unwrap().is_empty()));
    }
    // Every item was decided: each has its own declined receipt.
    assert_eq!(draft.citation_receipts.len(), 64);
}

/// Counts exactly with `exact` where the estimate assumes four characters a
/// token, as a tokenizer does on reaction notation, and refuses, as the
/// scheduler's gate does, a prompt its exact count says fills the window.
struct Notation {
    inner: ScriptedModel,
    window: usize,
    exact: fn(&str) -> usize,
    refusals: std::sync::atomic::AtomicUsize,
}

fn notation_model(window: usize, exact: fn(&str) -> usize, picks: Value) -> Notation {
    use crate::lessons::course_generation_tests::LINKS_SENTINEL;
    Notation {
        inner: ScriptedModel {
            outputs: std::sync::Mutex::new(vec![format!("{LINKS_SENTINEL}{picks}}}")].into()),
            prompts: std::sync::Mutex::new(vec![]),
        },
        window,
        exact,
        refusals: std::sync::atomic::AtomicUsize::new(0),
    }
}

#[async_trait::async_trait]
impl LLMPort for Notation {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        use lattice_core::application::ports::llm_port::CompletionInput;
        let rendered = request
            .input()
            .iter()
            .flat_map(|item| match item {
                CompletionInput::Message { role, content } => vec![role.clone(), content.clone()],
                _ => Vec::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let prompt_tokens = (self.exact)(&rendered);
        if prompt_tokens >= self.window {
            self.refusals
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens,
                window: self.window,
                counted_exactly: true,
                message: "over the window".into(),
            });
        }
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        "notation"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        Ok((self.exact)(text))
    }
    fn counts_tokens_exactly(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A paragraph of about 1,100 characters of reaction notation, prose by the
/// quote rules, that the iron-oxide item retrieves.
fn reaction_notes(i: usize) -> String {
    (0..11)
        .map(|k| format!("In trial {i}-{k}, iron oxide reduction Fe₂O₃ + 3CO → 2Fe + 3CO₂ reaches equilibrium when ΔG° falls to zero."))
        .collect::<Vec<_>>()
        .join(" ")
}

fn reaction_sources() -> Vec<LearningSourceDto> {
    (0..16)
        .map(|i| source(&format!("run-{i}"), &format!("Run {i}"), &reaction_notes(i)))
        .collect()
}

fn reaction_draft() -> OutlineDraft {
    draft(json!({"modules":[{"title":"Iron","prerequisiteIndices":[],
        "summary":{"text":"Explain iron oxide reduction and its equilibrium.","sourceIndex":null,"quote":""},
        "outcomes":[],"lessons":[]}]}))
}

#[tokio::test]
async fn one_item_on_notation_twice_as_dense_as_estimated_is_offered_the_candidates_that_fit() {
    let sources = reaction_sources();
    let mut draft = reaction_draft();
    let llm = notation_model(
        8_192,
        |text| text.chars().count() / 2,
        json!({"Explain iron oxide reduction and its equilibrium.":"iron oxide reduction"}),
    );
    let log = link(
        &llm,
        &mut draft,
        &sources,
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    let prompts = llm.inner.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1, "one chooser call, never refused");
    assert_eq!(llm.refusals.load(std::sync::atomic::Ordering::SeqCst), 0);
    let listed = fixture_task(&prompts[0]).unwrap()["passages"]
        .as_array()
        .unwrap()
        .len();
    assert!(listed > 0 && listed < sources.len(), "{listed} passages");
    assert_eq!(log.linked, 1);
    assert!(draft.candidate["modules"][0]["summary"]["sourceIndex"].is_u64());
}

#[tokio::test]
async fn one_item_whose_every_candidate_overflows_the_window_stays_unlinked_without_a_receipt() {
    let sources = reaction_sources();
    let mut draft = reaction_draft();
    let llm = notation_model(
        4_096,
        |text| text.chars().count() * 2,
        json!({"Explain iron oxide reduction and its equilibrium.":"iron oxide reduction"}),
    );
    let log = link(
        &llm,
        &mut draft,
        &sources,
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert_eq!(llm.refusals.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!((log.linked, log.items), (0, 1));
    assert_eq!(
        draft.candidate["modules"][0]["summary"]["sourceIndex"],
        Value::Null
    );
    assert!(draft.citation_receipts.is_empty());
}

#[tokio::test]
async fn a_chooser_reason_is_kept_whole() {
    let reason = "The passage explains random assignment and confounding. ".repeat(10);
    let mut draft = draft(candidate());
    let llm = model(vec![entries(&[
        ("i1", "p2", reason.as_str()),
        ("i2", "p1", reason.as_str()),
        ("i3", "p2", reason.as_str()),
        ("i4", "none", reason.as_str()),
    ])]);
    link(
        &llm,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();
    assert_eq!(draft.citation_receipts.len(), 4);
    assert!(
        draft
            .citation_receipts
            .values()
            .all(|receipt| receipt.reason == reason),
        "every receipt keeps the whole reason, {} characters",
        reason.chars().count()
    );
}

/// Records the progress the run reports while the chooser is being asked.
struct Watching<'a> {
    inner: ScriptedModel,
    progress: &'a OutlineProgress,
    seen: std::sync::Mutex<Vec<(OutlineStage, Option<String>)>>,
}

#[async_trait::async_trait]
impl LLMPort for Watching<'_> {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        let now = self.progress.snapshot();
        self.seen.lock().unwrap().push((now.stage, now.detail));
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn two_modules() -> OutlineDraft {
    let summary = |text: &str| json!({"text":text,"sourceIndex":null,"quote":""});
    draft(json!({"modules":[
        {"title":"Comparisons","prerequisiteIndices":[],
         "summary":summary("Compare treatment groups."),"outcomes":[],"lessons":[]},
        {"title":"Assignment","prerequisiteIndices":[],
         "summary":summary("Explain how random assignment limits confounding when groups are compared."),
         "outcomes":[],"lessons":[]}
    ]}))
}

fn watching(progress: &OutlineProgress) -> Watching<'_> {
    Watching {
        inner: model(vec![json!({"@links":{
            "Explain how random assignment limits confounding when groups are compared.":"Random assignment"
        }})]),
        progress,
        seen: std::sync::Mutex::new(vec![]),
    }
}

#[tokio::test]
async fn linking_one_module_leaves_the_stage_the_revision_is_in() {
    let progress = OutlineProgress::default();
    progress.stage(OutlineStage::Repairing);
    let llm = watching(&progress);
    link(&llm, &mut two_modules(), &sources(), Some(1), &progress)
        .await
        .unwrap();
    assert_eq!(progress.snapshot().stage, OutlineStage::Repairing);
    assert_eq!(llm.seen.lock().unwrap()[0].0, OutlineStage::Repairing);
}

#[tokio::test]
async fn linking_the_whole_outline_is_the_citation_stage() {
    let progress = OutlineProgress::default();
    progress.stage(OutlineStage::Repairing);
    let llm = watching(&progress);
    link(&llm, &mut two_modules(), &sources(), None, &progress)
        .await
        .unwrap();
    assert_eq!(progress.snapshot().stage, OutlineStage::CheckingCitations);
    assert_eq!(
        llm.seen.lock().unwrap()[0].0,
        OutlineStage::CheckingCitations
    );
}

#[tokio::test]
async fn linking_one_module_names_the_module_and_says_one_item() {
    let progress = OutlineProgress::default();
    let llm = watching(&progress);
    link(&llm, &mut two_modules(), &sources(), Some(1), &progress)
        .await
        .unwrap();
    assert_eq!(
        llm.seen.lock().unwrap()[0].1.as_deref(),
        Some("Module 2 of 2: linking 1 item to references (0 already linked)")
    );
    assert_eq!(
        progress.snapshot().detail.as_deref(),
        Some("Module 2 of 2: linked 1 item; 1 of 1 decided")
    );
}

#[tokio::test]
async fn linking_the_whole_outline_says_one_outline_item() {
    let progress = OutlineProgress::default();
    let llm = watching(&progress);
    let mut one = draft(json!({"modules":[
        {"title":"Assignment","prerequisiteIndices":[],
         "summary":{"text":"Explain how random assignment limits confounding when groups are compared.","sourceIndex":null,"quote":""},
         "outcomes":[],"lessons":[]}
    ]}));
    link(&llm, &mut one, &sources(), None, &progress)
        .await
        .unwrap();
    assert_eq!(
        llm.seen.lock().unwrap()[0].1.as_deref(),
        Some("Linking 1 outline item to references (0 already linked)")
    );
    assert_eq!(
        progress.snapshot().detail.as_deref(),
        Some("Linked 1 outline item; 1 of 1 decided")
    );
}

#[tokio::test]
async fn prompt_snapshot_outline_linking() {
    use crate::planning::outline_reference_tests::Capture;
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let mut draft = draft(candidate());
    let llm = model(vec![entries(&[
        (
            "i1",
            "p2",
            "The passage addresses interpreting observed differences.",
        ),
        (
            "i2",
            "p1",
            "The passage explains random assignment and confounding.",
        ),
        (
            "i3",
            "p2",
            "The passage is about uncertainty estimates, not assignment.",
        ),
        ("i4", "none", "Neither passage plans treatment groups."),
    ])]);
    let capture = Capture::of::<OutlineLinking>(&llm);

    link(
        &capture,
        &mut draft,
        &sources(),
        None,
        &OutlineProgress::default(),
    )
    .await
    .unwrap();

    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "outline-linking",
        &capture.requests(),
    );
}

/// Records every text the retrieval measures.
struct Measuring {
    inner: ScriptedModel,
    measured: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl LLMPort for Measuring {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.measured.lock().unwrap().push(text.to_owned());
        self.inner.count_tokens(text)
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn each_passage_is_measured_once_for_an_item() {
    let llm = Measuring {
        inner: model(vec![]),
        measured: std::sync::Mutex::new(vec![]),
    };
    let sources: Vec<LearningSourceDto> = (0..40)
        .map(|i| {
            source(
                &format!("s{i}"),
                &format!("Study {i}"),
                &format!("Random assignment in study {i} limits confounding when the treatment groups are compared. Trial {i} reports the comparison in plain words for every reader."),
            )
        })
        .collect();
    let collection = ReferenceCollection::lexical(&sources).unwrap();
    let hashes = source_hashes(&sources);
    let mut item = Pending {
        path: "/modules/0/summary".into(),
        kind: ItemKind::Summary,
        text: "Random assignment limits confounding.".into(),
        module_title: "Assignment".into(),
        lesson_title: None,
        candidates: vec![],
        author: None,
    };
    candidates_for(&llm, &mut item, &collection, &sources, &hashes, 1_000_000)
        .await
        .unwrap();
    assert!(
        item.candidates.len() > 16,
        "the scenario pages past the first page, got {} candidates",
        item.candidates.len()
    );
    let measured = llm.measured.lock().unwrap();
    let distinct: std::collections::HashSet<&String> = measured.iter().collect();
    assert_eq!(
        measured.len(),
        distinct.len(),
        "each passage's quote is measured once, {} measurements of {} distinct texts",
        measured.len(),
        distinct.len()
    );
}
