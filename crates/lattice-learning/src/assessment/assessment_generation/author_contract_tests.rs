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
    for at in 0..TEXTS.len() {
        let raw = one_item(&[&held], &kiln_choice(ASKED, &[at], &["u1"]));
        let item = to_candidate(raw, &need, &[&held]).unwrap();
        let LearningAssessmentKey::Choice(key) = item.candidate.key else {
            panic!("expected a single-choice key");
        };
        assert_eq!(item.candidate.options[key], TEXTS[at]);
        for (option, reason) in item
            .candidate
            .options
            .iter()
            .zip(&item.candidate.distractor_rationales)
        {
            let expected = if *option == TEXTS[at] {
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
fn a_quote_citing_two_passages_is_refused() {
    let need = single_choice();
    let first = passage("Firing", FIRING);
    let second = passage("Glazing", GLAZING);
    let raw = one_item(&[&first, &second], &kiln_choice(ASKED, &[0], &["u1", "u3"]));
    let unbuilt = to_candidate(raw, &need, &[&first, &second]).unwrap_err();
    assert!(
        unbuilt
            .faults
            .iter()
            .any(|fault| fault.contains("more than one passage")),
        "{:?}",
        unbuilt.faults
    );
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
}

fn queued(replies: Vec<(String, &'static str)>) -> Queued {
    Queued {
        replies: Mutex::new(replies.into()),
        seen: Mutex::new(Vec::new()),
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
    let second = kiln_choice("What happens to a tray left inside past noon?", &[1], &["u2"]);
    let llm = queued(vec![
        (format!("{{\"items\":[{first},{{\"prompt\":"), "length"),
        (json!({"items": [second]}).to_string(), "stop"),
    ]);
    let batch = Batch {
        need: &need,
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
async fn a_rewrite_keeps_the_difficulty_of_the_item_it_replaces_and_does_not_write_one() -> Result<()> {
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
        count: 1,
        notes: vec![],
        repair: None,
    };

    batch.author(&llm, &collection).await?;

    let seen = llm.seen.lock().unwrap();
    let schema = &seen[0].1;
    assert!(schema.contains("difficulty"), "{schema}");
    for gone in ["blockIndex", "answerIndex", "correctIndices", "distractorRationales"] {
        assert!(!schema.contains(gone), "{gone} in {schema}");
    }
    Ok(())
}
