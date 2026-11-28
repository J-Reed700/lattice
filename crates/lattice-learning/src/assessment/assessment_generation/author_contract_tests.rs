//! The quiz writers' answer layer: what they are told, how a written item is read,
//! and what is derived from it in code.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::assessment_engine::LearningBlueprintRequirement;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::services::model_contract::shape::{Ended, Listed, Written};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use std::collections::VecDeque;
use std::sync::Mutex;

const FIRING: &str = "The kiln is lit before dawn so that it reaches full heat by the time the first tray is ready. A tray left inside past noon cracks along its base.";
const GLAZING: &str = "Glaze is brushed on only after the first firing has cooled completely.";
const SHORT: &str = "Fire it slowly.";
const ASKED: &str = "When is the kiln lit so the first tray can be fired?";
const TEXTS: [&str; 4] = ["Before dawn", "At midday", "After dusk", "Not at all"];

fn passage(title: &str, text: &str) -> MaterialPassage {
    MaterialPassage {
        lesson_title: "Kiln work".into(),
        title: title.into(),
        text: text.into(),
        source_version_ids: vec![],
    }
}

fn kiln_need(format: LearningItemFormat, material: Vec<MaterialPassage>) -> AuthoringNeed {
    AuthoringNeed {
        requirement: LearningBlueprintRequirement {
            outcome_id: uuid::Uuid::new_v4().to_string(),
            format,
            count: 1,
            difficulty_min: 2,
            difficulty_max: 3,
        },
        outcome_title: "Firing a kiln".into(),
        outcome_description: "Say when a kiln is lit and what happens to a late tray.".into(),
        lesson_id: Some("lesson".into()),
        count: 1,
        pass: BankPass::Retakes,
        material,
        existing_prompts: vec![],
    }
}

fn single_choice() -> AuthoringNeed {
    kiln_need(
        LearningItemFormat::SingleChoice,
        vec![passage("Firing", FIRING)],
    )
}

/// A single-choice item as the model writes it, with the options marked in `right`.
fn kiln_choice(prompt: &str, right: &[usize], quote: &[&str]) -> Value {
    let options: Vec<Value> = TEXTS
        .iter()
        .enumerate()
        .map(|(at, text)| {
            let reason = if right.contains(&at) {
                String::new()
            } else {
                format!("{text} is not when the kiln is lit.")
            };
            json!({
                "text": text,
                "verdict": if right.contains(&at) { "right" } else { "wrong" },
                "reason": reason,
            })
        })
        .collect();
    json!({
        "prompt": prompt,
        "options": options,
        "explanation": "The kiln is lit early so that it is hot when the trays arrive.",
        "quote": quote,
        "difficulty": "2",
    })
}

/// The items `answer` holds, read as the layer reads them against the sentences of `passages` shown in order.
pub(super) fn read_items<B: Body>(passages: &[&MaterialPassage], answer: &str) -> Vec<RawItem> {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let held: Vec<Units> = passages
        .iter()
        .map(|passage| Units::of(&passage.text, Grain::Sentence))
        .collect();
    for units in &held {
        units.shown(&mut showing).unwrap();
    }
    let written = Written {
        text: answer,
        ended: Ended::Complete,
    };
    <New<Item<B, Fresh>> as Listed>::read(&written, &showing)
        .items
        .into_iter()
        .map(Item::into_raw)
        .collect()
}

fn one_item(passages: &[&MaterialPassage], item: &Value) -> RawItem {
    let mut items = read_items::<ChoiceBody>(passages, &json!({"items": [item]}).to_string());
    assert_eq!(items.len(), 1, "the item was not read");
    items.remove(0)
}

fn told_input(marks: &mut Marks) -> Input {
    let passage = Passage {
        lesson: marks.told("Kiln work".to_owned()),
        title: marks.told("Firing trays".to_owned()),
        text: marks.told("A tray left inside past noon cracks along its base.".to_owned()),
    };
    Input {
        outcome: marks.told("Firing a kiln correctly".to_owned()),
        description: marks.told("Say when a kiln is lit".to_owned()),
        format: marks.told(LearningItemFormat::SingleChoice),
        difficulty_min: Told(2),
        difficulty_max: Told(3),
        count: Told(2),
        do_not_repeat: marks.told(vec!["When does the glaze go on a tray?".to_owned()]),
        avoid: marks.told(vec!["The right option was the longest".to_owned()]),
        repair: marks.told(Some(json!({
            "instruction": "Write a question in this one's place that has none of these defects.",
            "question": "Which tray cracks along its base?",
            "options": ["One left inside past noon", "One fired before dawn"],
            "findings": ["The question depends on a value it never gives"],
        }))),
        written: marks.told(vec!["Which tray was fired before dawn?".to_owned()]),
        passages: Packed::new(vec![passage], |_| 1),
    }
}

#[test]
fn what_the_quiz_writers_are_told_reaches_their_request_and_no_constant_writes_a_label() {
    let mut marks = Marks::new();
    let input = told_input(&mut marks);
    holds::<QuizAuthor<ChoiceBody>>(&input, &marks);
    holds::<QuizAuthor<TruthBody>>(&input, &marks);
    holds::<QuizAuthor<SelectBody>>(&input, &marks);
    holds::<QuizAuthor<ShortBody>>(&input, &marks);
    holds::<QuizAuthor<ClozeBody>>(&input, &marks);
    holds::<QuizAuthor<PairsBody>>(&input, &marks);
    holds::<QuizAuthor<OrderBody>>(&input, &marks);
    holds::<QuizAuthor<RubricBody>>(&input, &marks);
    holds::<QuizRewrite<ChoiceBody>>(&input, &marks);
    holds::<QuizRewrite<RubricBody>>(&input, &marks);
}

#[test]
fn the_key_is_the_option_marked_right_wherever_it_stands() {
    let need = single_choice();
    let held = passage("Firing", FIRING);
    for (at, text) in TEXTS.iter().enumerate() {
        let raw = one_item(&[&held], &kiln_choice(ASKED, &[at], &["u1"]));
        let item = to_candidate(raw, &need, &[&held]).unwrap();
        let LearningAssessmentKey::Choice(key) = item.candidate.key else {
            panic!("expected a single-choice key");
        };
        assert_eq!(item.candidate.options[key], *text);
        for (option, reason) in item
            .candidate
            .options
            .iter()
            .zip(&item.candidate.distractor_rationales)
        {
            let expected = if *option == *text {
                String::new()
            } else {
                format!("{option} is not when the kiln is lit.")
            };
            assert_eq!(*reason, expected);
        }
    }
}

#[test]
fn a_single_choice_item_with_two_options_marked_right_is_refused_with_that_fault() {
    let need = single_choice();
    let held = passage("Firing", FIRING);
    let raw = one_item(&[&held], &kiln_choice(ASKED, &[0, 1], &["u1"]));
    let unbuilt = to_candidate(raw, &need, &[&held]).unwrap_err();
    assert!(
        unbuilt
            .faults
            .iter()
            .any(|fault| fault.contains("exactly one option marked right")),
        "{:?}",
        unbuilt.faults
    );
}

#[test]
fn the_quote_is_read_from_the_sentences_the_item_cites() {
    let need = single_choice();
    let held = passage("Firing", FIRING);
    let raw = one_item(&[&held], &kiln_choice(ASKED, &[0], &["u1", "u2"]));
    let item = to_candidate(raw, &need, &[&held]).unwrap();
    assert_eq!(item.source_quote, FIRING);
}

#[test]
fn a_quote_that_cites_no_shown_sentence_is_refused_and_the_item_is_told_so() {
    let need = single_choice();
    let held = passage("Firing", FIRING);
    let raw = one_item(&[&held], &kiln_choice(ASKED, &[0], &["u9"]));
    assert!(raw.cited.is_empty());
    let unbuilt = to_candidate(raw, &need, &[&held]).unwrap_err();
    assert!(
        unbuilt
            .faults
            .iter()
            .any(|fault| fault.contains("cites no sentence")),
        "{:?}",
        unbuilt.faults
    );
    assert_eq!(unbuilt.prompt, ASKED);
    assert_eq!(unbuilt.options.len(), TEXTS.len());
}

#[test]
fn a_quote_citing_multiple_passages_is_refused_so_quotes_stay_verbatim() {
    let need = single_choice();
    let first_id = uuid::Uuid::new_v4().to_string();
    let second_id = uuid::Uuid::new_v4().to_string();
    let mut first = passage("Firing", FIRING);
    first.source_version_ids = vec![first_id.clone(), "not-an-id".into()];
    let mut second = passage("Glazing", GLAZING);
    second.source_version_ids = vec![second_id.clone(), first_id.clone()];
    let raw = one_item(&[&first, &second], &kiln_choice(ASKED, &[0], &["u3", "u2"]));
    let unbuilt = to_candidate(raw, &need, &[&first, &second]).unwrap_err();
    assert!(
        unbuilt
            .faults
            .iter()
            .any(|f| f.contains("multiple passages")),
        "{:?}",
        unbuilt.faults
    );
}

#[test]
fn a_quote_mixing_sentences_across_passages_after_first_is_refused() {
    let need = single_choice();
    let first_id = uuid::Uuid::new_v4().to_string();
    let second_id = uuid::Uuid::new_v4().to_string();
    let mut first = passage("Firing", FIRING);
    first.source_version_ids = vec![first_id];
    let mut second = passage("Glazing", GLAZING);
    second.source_version_ids = vec![second_id];
    let raw = one_item(
        &[&first, &second],
        &kiln_choice(ASKED, &[0], &["u1", "u3", "u2"]),
    );
    let unbuilt = to_candidate(raw, &need, &[&first, &second]).unwrap_err();
    assert!(
        unbuilt
            .faults
            .iter()
            .any(|f| f.contains("multiple passages")),
        "{:?}",
        unbuilt.faults
    );
}

#[test]
fn a_quote_citing_a_sentence_of_one_of_two_passages_keeps_only_that_passages_ids() {
    let need = single_choice();
    let first_id = uuid::Uuid::new_v4().to_string();
    let second_id = uuid::Uuid::new_v4().to_string();
    let mut first = passage("Firing", FIRING);
    first.source_version_ids = vec![first_id.clone()];
    let mut second = passage("Glazing", GLAZING);
    second.source_version_ids = vec![second_id];
    let raw = one_item(&[&first, &second], &kiln_choice(ASKED, &[0], &["u1", "u2"]));
    let item = to_candidate(raw, &need, &[&first, &second]).unwrap();
    assert_eq!(item.source_quote, FIRING);
    assert_eq!(item.candidate.source_version_ids, vec![first_id]);
}

#[test]
fn the_author_is_told_items_rest_on_sentences_from_a_single_passage() {
    let said = system();
    assert!(said.contains("sentences of the passages"), "{said}");
    assert!(said.contains("from a single passage"), "{said}");
    assert!(said.contains("without mixing passages"), "{said}");
}

#[test]
fn a_quote_below_the_floor_is_refused_with_that_fault() {
    let need = single_choice();
    let held = passage("Firing", SHORT);
    let raw = one_item(&[&held], &kiln_choice(ASKED, &[0], &["u1"]));
    let unbuilt = to_candidate(raw, &need, &[&held]).unwrap_err();
    assert!(
        unbuilt
            .faults
            .iter()
            .any(|fault| fault.contains("shorter than")),
        "{:?}",
        unbuilt.faults
    );
}

#[test]
fn a_difficulty_that_is_not_a_number_leaves_the_item_unread() {
    let held = passage("Firing", FIRING);
    let mut item = kiln_choice(ASKED, &[0], &["u1"]);
    item["difficulty"] = json!("hard");
    let items = read_items::<ChoiceBody>(&[&held], &json!({"items": [item]}).to_string());
    assert!(items.is_empty());
}

/// Replies from a queue, the last one again when it runs out, and keeps each request's task and schema.
struct Queued {
    replies: Mutex<VecDeque<(String, &'static str)>>,
    seen: Mutex<Vec<(Value, String)>>,
    holds: bool,
}

fn queued(replies: Vec<(String, &'static str)>) -> Queued {
    Queued {
        replies: Mutex::new(replies.into()),
        seen: Mutex::new(Vec::new()),
        holds: false,
    }
}

/// Like `queued`, for a backend that holds an answer to its schema.
fn enforcing(replies: Vec<(String, &'static str)>) -> Queued {
    Queued {
        holds: true,
        ..queued(replies)
    }
}

#[async_trait::async_trait]
impl LLMPort for Queued {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        let schema = request
            .schema()
            .map(|schema| schema.as_json().to_string())
            .unwrap_or_default();
        self.seen
            .lock()
            .unwrap()
            .push((serde_json::from_str(user).unwrap_or(Value::Null), schema));
        let mut replies = self.replies.lock().unwrap();
        let (text, finish) = if replies.len() > 1 {
            replies.pop_front().unwrap_or_default()
        } else {
            replies.front().cloned().unwrap_or_default()
        };
        Ok(CompletionResponse {
            finish_reason: finish.into(),
            ..CompletionResponse::from_text(text)
        })
    }
    fn model_name(&self) -> &str {
        "queued"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        self.holds
    }
}

fn collection_of(need: &AuthoringNeed) -> Vec<LearningSourceDto> {
    need.material
        .iter()
        .enumerate()
        .map(|(index, m)| LearningSourceDto {
            id: format!("block:{index}"),
            title: m.title.clone(),
            url: None,
            excerpt: m.text.clone(),
            acquired_at: 0,
        })
        .collect()
}

#[tokio::test]
async fn an_answer_cut_off_after_an_item_keeps_it_and_asks_for_the_rest() -> Result<()> {
    let need = single_choice();
    let documents = collection_of(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let first = kiln_choice(ASKED, &[0], &["u1"]).to_string();
    let second = kiln_choice(
        "What happens to a tray left inside past noon?",
        &[1],
        &["u2"],
    );
    let llm = queued(vec![
        (format!("{{\"items\":[{first},{{\"prompt\":"), "length"),
        (json!({"items": [second]}).to_string(), "stop"),
    ]);
    let batch = Batch {
        need: &need,
        companies: &[],
        count: 2,
        notes: vec![],
        repair: None,
    };

    let written = batch.author(&llm, &collection).await?.unwrap();

    assert_eq!(written.len(), 2);
    assert!(written.iter().all(Result::is_ok), "{written:?}");
    let seen = llm.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].0["returnCount"], 2);
    assert!(seen[0].0.get("writtenSoFar").is_none());
    assert_eq!(seen[1].0["returnCount"], 1);
    assert_eq!(seen[1].0["writtenSoFar"], json!([ASKED]));
    Ok(())
}

#[tokio::test]
async fn a_rewrite_keeps_the_difficulty_of_the_item_it_replaces_and_does_not_write_one(
) -> Result<()> {
    let need = single_choice();
    let documents = collection_of(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let mut bare = kiln_choice(ASKED, &[0], &["u1"]);
    if let Some(fields) = bare.as_object_mut() {
        fields.remove("difficulty");
    }
    let llm = queued(vec![(bare.to_string(), "stop")]);
    let batch = Batch {
        need: &need,
        companies: &[],
        count: 1,
        notes: vec![],
        repair: Some(Repair {
            question: "Which tray cracks?".into(),
            options: vec!["One left inside".into()],
            findings: vec!["The question depends on a value it never gives.".into()],
            difficulty: 3,
        }),
    };

    let written = batch.author(&llm, &collection).await?.unwrap();

    let item = written.into_iter().next().unwrap().unwrap();
    assert_eq!(item.candidate.difficulty, 3);
    let seen = llm.seen.lock().unwrap();
    assert!(!seen[0].1.contains("difficulty"), "{}", seen[0].1);
    assert!(!seen[0].1.contains("blockIndex"), "{}", seen[0].1);
    assert_eq!(seen[0].0["repair"]["question"], "Which tray cracks?");
    Ok(())
}

#[tokio::test]
async fn a_rewrite_that_cannot_be_read_gives_no_replacement() -> Result<()> {
    let need = single_choice();
    let documents = collection_of(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let llm = queued(vec![("{\"unrelated\": 1}".to_owned(), "stop")]);
    let batch = Batch {
        need: &need,
        companies: &[],
        count: 1,
        notes: vec![],
        repair: Some(Repair {
            question: "Which tray cracks?".into(),
            options: vec![],
            findings: vec!["The question depends on a value it never gives.".into()],
            difficulty: 2,
        }),
    };

    assert!(batch.author(&llm, &collection).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn a_batch_asks_for_difficulty_but_names_no_index_or_block() -> Result<()> {
    let need = single_choice();
    let documents = collection_of(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let llm = queued(vec![("{\"items\":[]}".to_owned(), "stop")]);
    let batch = Batch {
        need: &need,
        companies: &[],
        count: 1,
        notes: vec![],
        repair: None,
    };

    batch.author(&llm, &collection).await?;

    let seen = llm.seen.lock().unwrap();
    let schema = &seen[0].1;
    assert!(schema.contains("difficulty"), "{schema}");
    for gone in [
        "blockIndex",
        "answerIndex",
        "correctIndices",
        "distractorRationales",
    ] {
        assert!(!schema.contains(gone), "{gone} in {schema}");
    }
    Ok(())
}

/// The `minItems` of every list named `name` in the properties of `schema`.
fn least_of(schema: &Value, name: &str, found: &mut Vec<Option<u64>>) {
    match schema {
        Value::Object(map) => {
            if let Some(list) = map.get("properties").and_then(|p| p.get(name)) {
                found.push(list["minItems"].as_u64());
            }
            for value in map.values() {
                least_of(value, name, found);
            }
        }
        Value::Array(values) => values.iter().for_each(|v| least_of(v, name, found)),
        _ => {}
    }
}

/// The schema a batch of `format` items is sent to a backend that holds answers to their schema.
async fn enforced_item_schema(format: LearningItemFormat) -> Value {
    let need = kiln_need(format, vec![passage("Firing", FIRING)]);
    let documents = collection_of(&need);
    let collection = ReferenceCollection::lexical(&documents).unwrap();
    let llm = enforcing(vec![("{\"items\":[]}".to_owned(), "stop")]);
    let batch = Batch {
        need: &need,
        companies: &[],
        count: 1,
        notes: vec![],
        repair: None,
    };
    batch.author(&llm, &collection).await.unwrap();
    let seen = llm.seen.lock().unwrap();
    serde_json::from_str(&seen[0].1).unwrap()
}

#[tokio::test]
async fn a_short_answer_asks_for_at_least_one_accepted_answer() {
    let schema = enforced_item_schema(LearningItemFormat::ShortAnswer).await;
    let mut found = Vec::new();
    least_of(&schema, "acceptedAnswers", &mut found);

    assert_eq!(found, vec![Some(1)], "{schema}");
}

#[tokio::test]
async fn a_cloze_asks_for_at_least_one_blank_and_one_accepted_answer_in_each() {
    let schema = enforced_item_schema(LearningItemFormat::Cloze).await;
    let (mut blanks, mut accepted) = (Vec::new(), Vec::new());
    least_of(&schema, "blanks", &mut blanks);
    least_of(&schema, "acceptedAnswers", &mut accepted);

    assert_eq!(blanks, vec![Some(1)], "{schema}");
    assert_eq!(accepted, vec![Some(1)], "{schema}");
}

#[test]
fn authored_content_faults_rejects_meta_talk_and_phantom_versions_but_allows_stated_or_standard_versions(
) {
    let mut meta = candidate_with(
        LearningItemFormat::SingleChoice,
        "Under the versioning rules of this course, which increment applies?",
        "Minor increments apply for backward compatible additions.",
        vec![],
    );
    meta.prompt = "Under the versioning rules of this course, which increment applies?".into();
    let faults = engine::checks::authored_content_faults(&meta, "");
    assert!(faults.iter().any(|f| f.contains("meta-talk")), "{faults:?}");

    let phantom = candidate_with(
        LearningItemFormat::Matching,
        "Match each connector term to the meaning that fits it.",
        "The pinned route version records which connector build owns a delivery, so a request for the 4.0.1 build keeps that number.",
        vec![],
    );
    let phantom_faults = engine::checks::authored_content_faults(&phantom, "");
    assert!(
        phantom_faults.iter().any(|f| f.contains("4.0.1")),
        "{phantom_faults:?}"
    );

    let trailing_dot_phantom = candidate_with(
        LearningItemFormat::SingleChoice,
        "Which header pins the route version?",
        "Routes pin directly to 4.0.1.",
        vec![],
    );
    let dot_faults = engine::checks::authored_content_faults(&trailing_dot_phantom, "");
    assert!(
        dot_faults.iter().any(|f| f.contains("4.0.1")),
        "{dot_faults:?}"
    );

    let valid = candidate_with(
        LearningItemFormat::ShortAnswer,
        "Connector 6.2.3. adds an optional field. What version should it carry?",
        "Adding an optional field raises minor from 2 to 3, giving 6.3.0 under SemVer 2.0.0, aligning with HL7 v2.5.1, FHIR R4 (4.0.1), and FHIR R4 4.0.1.",
        vec![],
    );
    let valid_faults = engine::checks::authored_content_faults(&valid, "");
    assert!(valid_faults.is_empty(), "{valid_faults:?}");
}

#[test]
fn author_and_review_prompts_forbid_meta_talk_phantom_details_and_version_digit_mismatches() {
    let author = system();
    assert!(author.contains("meta-talk"), "{author}");
    assert!(author.contains("digit by digit"), "{author}");
    assert!(
        author.contains("Never attribute invented routes"),
        "{author}"
    );

    let review = crate::assessment::item_review::ITEM_REVIEW_SYSTEM;
    assert!(
        !review.contains("mention of the lesson is not a mention of a source"),
        "{review}"
    );
    assert!(
        review.contains("disagrees with the question's prompt or key"),
        "{review}"
    );
    assert!(
        review.contains("build or version number absent"),
        "{review}"
    );
    assert!(review.contains("meta-talk"), "{review}");
}

fn candidate_with(
    format: LearningItemFormat,
    prompt: &str,
    explanation: &str,
    rationales: Vec<String>,
) -> LearningAssessmentCandidate {
    LearningAssessmentCandidate {
        id: uuid::Uuid::new_v4().to_string(),
        outcome_ids: vec![uuid::Uuid::new_v4().to_string()],
        format,
        difficulty: 2,
        prompt: prompt.into(),
        options: vec![
            "Alpha".into(),
            "Beta".into(),
            "Gamma".into(),
            "Delta".into(),
        ],
        left: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key: match format {
            LearningItemFormat::ShortAnswer => {
                LearningAssessmentKey::Text(crate::assessment_engine::LearningTextKey {
                    variants: vec!["6.3.0".into()],
                    numeric: None,
                })
            }
            LearningItemFormat::Matching => LearningAssessmentKey::Matching(vec![0, 1]),
            _ => LearningAssessmentKey::Choice(0),
        },
        explanation: explanation.into(),
        distractor_rationales: rationales,
    }
}

#[test]
fn right_choice_rationale_is_cleared_by_read_options_and_rejected_by_validation() {
    let need = single_choice();
    let held = passage("Firing", FIRING);
    let mut item_json = kiln_choice(ASKED, &[0], &["u1"]);
    item_json["options"][0]["reason"] = json!("Should never appear on the right answer.");
    let raw = one_item(&[&held], &item_json);
    let built = to_candidate(raw, &need, &[&held]).unwrap();
    let LearningAssessmentKey::Choice(right_idx) = built.candidate.key else {
        panic!("expected choice key");
    };
    assert_eq!(built.candidate.distractor_rationales[right_idx], "");

    let invalid_right_reason = candidate_with(
        LearningItemFormat::SingleChoice,
        "Which option matches?",
        "Alpha matches.",
        vec![
            "Non-empty right rationale.".into(),
            "Wrong because B.".into(),
            "Wrong because C.".into(),
            "Wrong because D.".into(),
        ],
    );
    let err = engine::validate_candidate(&invalid_right_reason)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("right choice's distractor rationale empty"),
        "{err}"
    );
}

#[test]
fn authored_content_faults_for_catches_ungrounded_companies_and_invalid_smart_scopes() {
    let company_faults = engine::checks::authored_content_faults_for(
        &candidate_with(
            LearningItemFormat::SingleChoice,
            "Which capability would system/Binary.w permit Documo to do?",
            "Documo\u{2019}s delivery service writes files.",
            vec![
                String::new(),
                "system/DocumentReference.r allows reading, while .w allows writing.".into(),
                "Wrong.".into(),
                "Wrong.".into(),
            ],
        ),
        "SMART v2 uses .c for create.",
        &["Documo".to_owned()],
    );
    assert!(
        company_faults.iter().any(|f| f.contains("Documo")),
        "{company_faults:?}"
    );
    assert!(
        company_faults.iter().any(|f| f.contains("system/Binary.w")),
        "{company_faults:?}"
    );

    let distractor_ok = engine::checks::authored_content_faults_for(
        &LearningAssessmentCandidate {
            id: "id".into(),
            outcome_ids: vec!["o".into()],
            format: LearningItemFormat::SingleChoice,
            difficulty: 2,
            prompt: "Which SMART v2 scope requests create access on Binary?".into(),
            options: vec![
                "system/Binary.c".into(),
                "system/Binary.w".into(),
                "system/Binary.r".into(),
                "system/Binary.d".into(),
            ],
            left: vec![],
            source_version_ids: vec![],
            rubric: vec![],
            key: LearningAssessmentKey::Choice(0),
            explanation: "SMART v2 uses .c for create; .w does not exist.".into(),
            distractor_rationales: vec![
                String::new(),
                "system/Binary.w does not exist in SMART v2.".into(),
                "system/Binary.r requests read access.".into(),
                "system/Binary.d requests delete access.".into(),
            ],
        },
        "SMART v2 uses .c for create.",
        &["Documo".to_owned()],
    );
    assert!(distractor_ok.is_empty(), "{distractor_ok:?}");

    let matching_faults = engine::checks::authored_content_faults_for(
        &LearningAssessmentCandidate {
            id: "match-id".into(),
            outcome_ids: vec!["o".into()],
            format: LearningItemFormat::Matching,
            difficulty: 2,
            prompt: "Match each SMART workflow with its scope.".into(),
            left: vec![
                "Read binary attachments".into(),
                "Search observations".into(),
                "Create documents".into(),
            ],
            options: vec![
                "patient/Binary.rw".into(),
                "patient/Observation.rs".into(),
                "system/DocumentReference.c".into(),
                "system/Binary.w".into(),
            ],
            source_version_ids: vec![],
            rubric: vec![],
            key: LearningAssessmentKey::Matching(vec![0, 1, 2]),
            explanation: "Use SMART v2 granular scopes.".into(),
            distractor_rationales: vec![],
        },
        "SMART v2 uses .r/.c/.u/.d/.s.",
        &[],
    );
    assert!(
        matching_faults
            .iter()
            .any(|f| f.contains("patient/Binary.rw")),
        "{matching_faults:?}"
    );
    assert!(
        !matching_faults
            .iter()
            .any(|f| f.contains("system/Binary.w")),
        "{matching_faults:?}"
    );
}

#[test]
fn single_passage_quote_picks_contiguous_verbatim_slice_and_rejects_disjoint_short_jumps() {
    let need = single_choice();
    let body = "First long sentence about pre-heating kiln trays safely.\n\n\nSecond short. Third sentence explains cooling delays before glazing.";
    let held = passage("Firing", body);
    // Citing u1 + u3 across the `\n\n\n` stripped-fence boundary picks the longest contiguous verbatim run.
    let raw = one_item(&[&held], &kiln_choice(ASKED, &[0], &["u1", "u3"]));
    let built = to_candidate(raw, &need, &[&held]).unwrap();
    assert_eq!(
        built.source_quote,
        "First long sentence about pre-heating kiln trays safely."
    );
    assert!(crate::grade_rationale::verbatim(body, &built.source_quote));

    // When all cited parts are short disjoint fragments separated by `\n\n\n`, refuse concatenation.
    let disjoint = passage("Firing", "Too short.\n\n\nAlso brief.\n\n\nThird tiny.");
    let bad_raw = one_item(&[&disjoint], &kiln_choice(ASKED, &[0], &["u1", "u3"]));
    let err = to_candidate(bad_raw, &need, &[&disjoint]).unwrap_err();
    assert!(
        err.faults
            .iter()
            .any(|f| f.contains("consecutive prose sentences")),
        "{:?}",
        err.faults
    );
}

#[test]
fn authored_content_faults_catches_passage_meta_talk_and_scope_401_contradiction() {
    let passage_ref = candidate_with(
        LearningItemFormat::SingleChoice,
        "What happens when Retry-After is missing?",
        "Based on the provided principles, the passage states that exponential backoff is required, while the passage does not mention 500 errors.",
        vec![],
    );
    let meta_faults = engine::checks::authored_content_faults(&passage_ref, "");
    assert!(
        meta_faults.iter().any(|f| f.contains("meta-talk")),
        "{meta_faults:?}"
    );

    let wrong_status = candidate_with(
        LearningItemFormat::SingleChoice,
        "Which HTTP status code occurs when scopes are missing?",
        "A 401 Unauthorized error stems from an invalid scope mismatch.",
        vec![],
    );
    let status_faults = engine::checks::authored_content_faults_for(
        &wrong_status,
        "Insufficient scopes return HTTP 403 Forbidden with insufficient_scope.",
        &[],
    );
    assert!(
        status_faults.iter().any(|f| f.contains("403 Forbidden")),
        "{status_faults:?}"
    );

    let ungrounded_scope_401 = engine::checks::authored_content_faults_for(
        &candidate_with(
            LearningItemFormat::SingleChoice,
            "Which status code indicates an unprovisioned SMART scope?",
            "A 401 Unauthorized response indicates an invalid scope mismatch.",
            vec![],
        ),
        "Unprovisioned SMART scopes fail authorization checks.",
        &[],
    );
    assert!(
        ungrounded_scope_401
            .iter()
            .any(|f| f.contains("403 Forbidden")),
        "{ungrounded_scope_401:?}"
    );

    let prompted_scope_401 = engine::checks::authored_content_faults_for(
        &candidate_with(
            LearningItemFormat::SingleChoice,
            "An upstream gateway returns HTTP 401 on an invalid scope mismatch; how should the caller log it?",
            "A 401 Unauthorized error occurred due to an invalid scope mismatch.",
            vec![],
        ),
        "Unprovisioned SMART scopes fail authorization checks.",
        &[],
    );
    assert!(prompted_scope_401.is_empty(), "{prompted_scope_401:?}");
}

#[test]
fn repeat_descriptors_and_overlap_guard_catch_duplicate_matching_and_cloze_decisions() {
    let first_match = LearningAssessmentCandidate {
        id: "m1".into(),
        outcome_ids: vec!["o1".into()],
        format: LearningItemFormat::Matching,
        difficulty: 3,
        prompt: "Match each rate-limiting scenario to the connector action.".into(),
        left: vec![
            "429 with Retry-After header".into(),
            "429 without Retry-After header".into(),
            "Repeated retries violating Retry-After".into(),
        ],
        options: vec![
            "Pause for the Retry-After duration".into(),
            "Apply exponential backoff with jitter".into(),
            "Risk API key suspension".into(),
        ],
        source_version_ids: vec![],
        rubric: vec![],
        key: LearningAssessmentKey::Matching(vec![0, 1, 2]),
        explanation: "Respect Retry-After, use jittered backoff as fallback, and avoid suspension."
            .into(),
        distractor_rationales: vec![],
    };
    let dup_match = LearningAssessmentCandidate {
        id: "m2".into(),
        prompt: "A healthcare data connector faces HTTP 429 conditions. Match each condition to its outcome.".into(),
        left: vec![
            "Server returns Retry-After: 60".into(),
            "Retry-After header omitted".into(),
            "Sustained violation of rate contract".into(),
        ],
        options: vec![
            "Pause for Retry-After duration specified".into(),
            "Compute exponential backoff with random jitter".into(),
            "API key suspension from sustained violation".into(),
        ],
        ..first_match.clone()
    };
    let held_desc = crate::assessment::item_review::item_repeat_descriptor(&first_match);
    assert!(held_desc.contains("[pairs: "), "{held_desc}");
    assert!(
        crate::assessment::item_review::repeat_overlap_reason(&dup_match, &[held_desc]).is_some()
    );

    let first_cloze = LearningAssessmentCandidate {
        id: "c1".into(),
        outcome_ids: vec!["o1".into()],
        format: LearningItemFormat::Cloze,
        difficulty: 3,
        prompt: "Parse the {{1}} header, or fall back to {{2}}.".into(),
        left: vec![],
        options: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key: LearningAssessmentKey::Cloze(vec![
            crate::assessment_engine::LearningTextKey {
                variants: vec!["Retry-After".into()],
                numeric: None,
            },
            crate::assessment_engine::LearningTextKey {
                variants: vec!["exponential backoff with jitter".into()],
                numeric: None,
            },
        ]),
        explanation: "Parse Retry-After or apply exponential backoff with jitter.".into(),
        distractor_rationales: vec![],
    };
    let dup_cloze = LearningAssessmentCandidate {
        id: "c2".into(),
        prompt: "When {{1}} is absent, the connector falls back to {{2}}.".into(),
        ..first_cloze.clone()
    };
    let held_cloze = crate::assessment::item_review::item_repeat_descriptor(&first_cloze);
    assert!(held_cloze.contains("[blanks: "), "{held_cloze}");
    assert!(crate::assessment::item_review::repeat_overlap_reason(
        &dup_cloze,
        std::slice::from_ref(&held_cloze)
    )
    .is_some());
    let overrepresented = crate::assessment::item_review::overrepresented_topics(&[
        held_cloze.clone(),
        held_cloze,
        crate::assessment::item_review::item_repeat_descriptor(&first_match),
        crate::assessment::item_review::item_repeat_descriptor(&dup_match),
    ]);
    assert!(!overrepresented.is_empty());
    for tag in ["pairs", "blanks", "order", "picks"] {
        assert!(
            !overrepresented.iter().any(|t| t == tag),
            "{overrepresented:?}"
        );
    }

    let two_word_right_a = LearningAssessmentCandidate {
        id: "tw1".into(),
        prompt: "Match each HTTP error handling policy.".into(),
        left: vec!["First rate condition".into(), "Second auth event".into()],
        options: vec!["Exponential backoff".into(), "Token refresh".into()],
        key: LearningAssessmentKey::Matching(vec![0, 1]),
        ..first_match.clone()
    };
    let two_word_right_b = LearningAssessmentCandidate {
        id: "tw2".into(),
        prompt: "Match each different connector strategy.".into(),
        left: vec![
            "Distinct telemetry trigger".into(),
            "Distinct billing quota".into(),
        ],
        options: vec!["Random backoff".into(), "Scope refresh".into()],
        key: LearningAssessmentKey::Matching(vec![0, 1]),
        ..first_match
    };
    assert!(crate::assessment::item_review::repeat_overlap_reason(
        &two_word_right_b,
        &[crate::assessment::item_review::item_repeat_descriptor(
            &two_word_right_a
        )],
    )
    .is_none());

    let one_blank = |id: &str, prompt: &str, blank: &str| LearningAssessmentCandidate {
        id: id.into(),
        prompt: prompt.into(),
        key: LearningAssessmentKey::Cloze(vec![crate::assessment_engine::LearningTextKey {
            variants: vec![blank.into()],
            numeric: None,
        }]),
        ..first_cloze.clone()
    };
    let cloze_scope = one_blank(
        "c3",
        "When client scopes fail validation, respond with HTTP {{1}} Forbidden.",
        "403",
    );
    let cloze_unrelated = one_blank(
        "c4",
        "Billing account suspension triggers status {{1}} on unauthorized admin calls.",
        "403",
    );
    assert!(crate::assessment::item_review::repeat_overlap_reason(
        &cloze_unrelated,
        &[crate::assessment::item_review::item_repeat_descriptor(
            &cloze_scope
        )],
    )
    .is_none());

    let cloze_wait_a = one_blank(
        "c5",
        "Respect the duration in the {{1}} header on rate limits.",
        "Retry-After",
    );
    let cloze_wait_b = one_blank(
        "c6",
        "Pause for the interval inside the {{1}} header on rate limits.",
        "Retry-After",
    );
    assert!(crate::assessment::item_review::repeat_overlap_reason(
        &cloze_wait_b,
        &[crate::assessment::item_review::item_repeat_descriptor(
            &cloze_wait_a
        )],
    )
    .is_some());
}
