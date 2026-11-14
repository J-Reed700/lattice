use super::*;
use crate::assessment_engine::LearningBlueprintRequirement;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Counts exactly at two characters a token where the estimate assumes
/// four, as on reaction notation, and refuses, as the scheduler's gate does,
/// a prompt its exact count says fills the window.
struct Notation {
    prompts: Mutex<Vec<Value>>,
    refusals: AtomicUsize,
}

fn notation() -> Notation {
    Notation {
        prompts: Mutex::new(Vec::new()),
        refusals: AtomicUsize::new(0),
    }
}

const WINDOW: usize = 8_192;

fn exact(text: &str) -> usize {
    text.chars().count() / 2
}

#[async_trait::async_trait]
impl LLMPort for Notation {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let rendered = request
            .input
            .iter()
            .flat_map(|item| match item {
                CompletionInput::Message { role, content } => vec![role.clone(), content.clone()],
                _ => Vec::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        if exact(&rendered) >= WINDOW {
            self.refusals.fetch_add(1, Ordering::SeqCst);
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens: exact(&rendered),
                window: WINDOW,
                counted_exactly: true,
                message: "over the window".into(),
            });
        }
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        if let Ok(mut prompts) = self.prompts.lock() {
            prompts.push(serde_json::from_str(user)?);
        }
        Ok(CompletionResponse::from_text("{\"items\":[]}"))
    }
    fn model_name(&self) -> &str {
        "notation"
    }
    fn max_context_tokens(&self) -> usize {
        WINDOW
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        Ok(exact(text))
    }
    fn counts_tokens_exactly(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// About 1,100 characters of reaction notation.
fn reaction_notes(i: usize) -> String {
    (0..11)
        .map(|k| format!("In trial {i}-{k}, iron oxide reduction Fe₂O₃ + 3CO → 2Fe + 3CO₂ reaches equilibrium when ΔG° falls to zero."))
        .collect::<Vec<_>>()
        .join(" ")
}

fn need(description: String, blocks: usize) -> AuthoringNeed {
    AuthoringNeed {
        requirement: LearningBlueprintRequirement {
            outcome_id: "reduction".into(),
            format: LearningItemFormat::Explanation,
            count: 1,
            difficulty_min: 2,
            difficulty_max: 3,
        },
        outcome_title: "Iron oxide reduction".into(),
        outcome_description: description,
        lesson_id: Some("lesson".into()),
        count: 1,
        pass: BankPass::Retakes,
        material: (0..blocks)
            .map(|i| MaterialPassage {
                lesson_title: "Reduction".into(),
                title: format!("Run {i}"),
                text: reaction_notes(i),
                source_version_ids: vec![],
            })
            .collect(),
        existing_prompts: vec![],
    }
}

/// The lesson blocks as the collection `author_items` searches.
fn documents(need: &AuthoringNeed) -> Vec<LearningSourceDto> {
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
async fn passages_twice_as_dense_as_estimated_are_packed_until_the_request_fits() -> Result<()> {
    let need = need(
        "Explain iron oxide reduction and its equilibrium.".into(),
        16,
    );
    let documents = documents(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let batch = Batch {
        need: &need,
        count: 1,
        notes: vec![],
        repair: None,
    };
    let llm = notation();

    let sent = batch.send(&llm, &collection).await?;

    let Some((_, carried)) = sent else {
        panic!("expected the request to be sent with the passages that fit");
    };
    assert_eq!(llm.refusals.load(Ordering::SeqCst), 0);
    let prompts = llm
        .prompts
        .lock()
        .map_err(|_| AppError::InternalError("lock".into()))?;
    assert_eq!(prompts.len(), 1);
    let shown = prompts
        .first()
        .and_then(|prompt| prompt["passages"].as_array())
        .map_or(0, Vec::len);
    assert!(shown > 0 && shown < need.material.len(), "{shown} passages");
    // Quotes are checked against exactly the passages the model saw.
    assert_eq!(carried.len(), shown);
    Ok(())
}

#[tokio::test]
async fn instructions_the_window_cannot_hold_by_exact_count_name_the_window() {
    // Within the window by the estimate, twice over it by the exact count.
    let description = format!(
        "Explain iron oxide reduction. {}",
        "Fe₂O₃ ⇌ 2Fe³⁺ + 3O²⁻ ".repeat(800)
    );
    let llm = notation();
    let error = author_items(&llm, &need(description, 4)).await.err();
    let Some(AppError::WindowTooSmall(message)) = error else {
        panic!("expected the window to be named, got {error:?}");
    };
    assert!(
        message.contains("notation's 8192-token context window"),
        "{message}"
    );
    assert!(llm.prompts.lock().is_ok_and(|prompts| prompts.is_empty()));
    assert_eq!(llm.refusals.load(Ordering::SeqCst), 0);
}

/// Judges short answers from a script: a request holding more than `fits`
/// items, or the item `cut`, has its reply cut off at the output limit;
/// any other is judged equivalent, item by item.
struct Grader {
    fits: usize,
    cut: &'static str,
    sizes: Mutex<Vec<usize>>,
}

#[async_trait::async_trait]
impl LLMPort for Grader {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        let prompt: Value = serde_json::from_str(user)?;
        let ids: Vec<String> = prompt["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["itemId"].as_str().map(str::to_owned))
            .collect();
        self.sizes.lock().unwrap().push(ids.len());
        if ids.len() > self.fits || ids.iter().any(|id| id == self.cut) {
            return Ok(CompletionResponse {
                finish_reason: "length".into(),
                ..CompletionResponse::from_text("{\"items\":[{\"itemId\":")
            });
        }
        let items: Vec<Value> = ids
            .iter()
            .map(
                |id| serde_json::json!({"itemId": id, "verdict": "equivalent", "summary": "Same."}),
            )
            .collect();
        Ok(CompletionResponse::from_text(
            serde_json::json!({ "items": items }).to_string(),
        ))
    }
    fn model_name(&self) -> &str {
        "grader"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn short(id: &str) -> ShortAnswer {
    ShortAnswer {
        item_id: id.into(),
        prompt: "When did the mill close?".into(),
        accepted: vec!["1887".into()],
        answer: "In 1887.".into(),
        reference: "The mill closed in 1887.".into(),
        learner_note: None,
    }
}

#[tokio::test]
async fn a_grading_reply_cut_off_is_asked_again_in_halves_and_grades_every_item() {
    let grader = Grader {
        fits: 2,
        cut: "",
        sizes: Mutex::new(Vec::new()),
    };
    let answers: Vec<ShortAnswer> = ["a", "b", "c", "d", "e"].map(short).to_vec();
    let graded = judge_short_answers(&grader, &answers).await.unwrap();
    let mut ids: Vec<_> = graded.iter().map(|grade| grade.item_id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["a", "b", "c", "d", "e"],
        "nothing was lost to the cut-off replies"
    );
    let sizes = grader.sizes.lock().unwrap().clone();
    assert_eq!(
        sizes,
        [5, 2, 2, 1],
        "five cut off, then the rest at the smaller size"
    );
}

#[tokio::test]
async fn an_item_whose_reply_is_cut_off_alone_stays_pending_and_the_rest_are_graded() {
    let grader = Grader {
        fits: usize::MAX,
        cut: "b",
        sizes: Mutex::new(Vec::new()),
    };
    let answers: Vec<ShortAnswer> = ["a", "b", "c"].map(short).to_vec();
    let graded = judge_short_answers(&grader, &answers)
        .await
        .expect("one overlong reply no longer fails the grading");
    let mut ids: Vec<_> = graded.iter().map(|grade| grade.item_id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["a", "c"], "[b] stays pending");
}

/// Declines every request that holds the item `b`; grades any other.
struct Decliner(Mutex<Vec<usize>>);

#[async_trait::async_trait]
impl LLMPort for Decliner {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        let prompt: Value = serde_json::from_str(user)?;
        let ids: Vec<String> = prompt["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["itemId"].as_str().map(str::to_owned))
            .collect();
        self.0.lock().unwrap().push(ids.len());
        if ids.iter().any(|id| id == "b") {
            return Ok(CompletionResponse {
                finish_reason: "refusal".into(),
                ..CompletionResponse::from_text("")
            });
        }
        let items: Vec<Value> = ids
            .iter()
            .map(
                |id| serde_json::json!({"itemId": id, "verdict": "equivalent", "summary": "Same."}),
            )
            .collect();
        Ok(CompletionResponse::from_text(
            serde_json::json!({ "items": items }).to_string(),
        ))
    }
    fn model_name(&self) -> &str {
        "decliner"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn an_item_the_model_declines_stays_pending_and_the_rest_are_graded() {
    let llm = Decliner(Mutex::new(Vec::new()));
    let answers: Vec<ShortAnswer> = ["a", "b", "c"].map(short).to_vec();
    let graded = judge_short_answers(&llm, &answers).await.unwrap();
    let mut ids: Vec<_> = graded.iter().map(|grade| grade.item_id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["a", "c"], "[b] stays pending");
    assert_eq!(
        llm.0.lock().unwrap().clone(),
        [3, 1, 1, 1],
        "the batch is declined and split; a and c are graded; b is declined alone"
    );
}

/// Every reply cut off at the output limit.
struct CutOff;

#[async_trait::async_trait]
impl LLMPort for CutOff {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse {
            finish_reason: "length".into(),
            ..CompletionResponse::from_text("{\"items\":[{\"prompt\":")
        })
    }
    fn model_name(&self) -> &str {
        "cut-off"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_batch_cut_off_asks_for_fewer_and_a_single_item_cut_off_is_refused() -> Result<()> {
    let need = need("Explain iron oxide reduction.".into(), 2);
    let documents = documents(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let batch = |count| Batch {
        need: &need,
        count,
        notes: vec![],
        repair: None,
    };
    assert!(batch(2).author(&CutOff, &collection).await?.is_none());
    let single = batch(1).author(&CutOff, &collection).await;
    assert!(matches!(single, Err(AppError::AnswerCutOff(_))));
    Ok(())
}

/// Cuts its first answer off at the output limit and completes the second;
/// records each request's output reservation.
struct CutOnce(Mutex<Vec<Option<u32>>>);

#[async_trait::async_trait]
impl LLMPort for CutOnce {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        let prompt: Value = serde_json::from_str(user).unwrap_or(Value::Null);
        let mut budgets = self.0.lock().unwrap();
        budgets.push(request.max_output_tokens);
        if budgets.len() == 1 {
            return Ok(CompletionResponse {
                finish_reason: "length".into(),
                ..CompletionResponse::from_text("{\"items\":[{\"itemId\":")
            });
        }
        let items: Vec<Value> = prompt["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["itemId"].as_str())
            .map(
                |id| serde_json::json!({"itemId": id, "verdict": "equivalent", "summary": "Same."}),
            )
            .collect();
        Ok(CompletionResponse::from_text(
            serde_json::json!({ "items": items }).to_string(),
        ))
    }
    fn model_name(&self) -> &str {
        "cut-once"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn doubled(budgets: &Mutex<Vec<Option<u32>>>) -> bool {
    let budgets = budgets.lock().unwrap().clone();
    let first = budgets.first().copied().flatten();
    first.is_some() && budgets.get(1).copied().flatten() == first.map(|b| b * 2)
}

#[tokio::test]
async fn a_single_grading_reply_cut_off_is_asked_again_with_twice_the_room() {
    let grader = CutOnce(Mutex::new(Vec::new()));
    let graded = judge_short_answers(&grader, &[short("a")]).await.unwrap();
    assert_eq!(
        graded.len(),
        1,
        "the learner's answer is graded, not left pending"
    );
    assert!(doubled(&grader.0));
}

#[tokio::test]
async fn a_single_item_cut_off_is_asked_again_with_twice_the_room() -> Result<()> {
    let need = need("Explain iron oxide reduction.".into(), 1);
    let documents = documents(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let batch = Batch {
        need: &need,
        count: 1,
        notes: vec![],
        repair: None,
    };
    let llm = CutOnce(Mutex::new(Vec::new()));
    let single = batch.author(&llm, &collection).await?;
    assert!(single.is_some(), "the answer given twice the room is read");
    assert!(doubled(&llm.0));
    Ok(())
}

/// Records the schema of each request and grades nothing.
struct SchemaRecorder(Mutex<Vec<Value>>);

#[async_trait::async_trait]
impl LLMPort for SchemaRecorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.0.lock().unwrap().extend(request.json_schema.clone());
        Ok(CompletionResponse::from_text("{\"items\":[]}"))
    }
    fn model_name(&self) -> &str {
        "schema-recorder"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_grader_may_name_only_the_rubric_criteria() {
    let criterion = |id: &str| LearningRubricCriterion {
        id: id.into(),
        title: id.into(),
        description: "Explains it.".into(),
        max_points: 2,
    };
    let answer = WrittenAnswer {
        item_id: "q1".into(),
        prompt: "Why does the ore reduce?".into(),
        rubric: vec![criterion("mechanism"), criterion("evidence")],
        answer: "Carbon monoxide takes the oxygen.".into(),
        reference: "CO reduces Fe2O3.".into(),
        learner_note: None,
    };
    let llm = SchemaRecorder(Mutex::new(Vec::new()));
    grade_written(&llm, &[answer]).await.unwrap();
    let schemas = llm.0.lock().unwrap();
    assert_eq!(
        schemas[0]["properties"]["items"]["items"]["properties"]["criteria"]["items"]["properties"]
            ["criterionId"]["enum"],
        serde_json::json!(["evidence", "mechanism"])
    );
}

#[test]
fn the_item_schemas_hold_no_text_length_cap() {
    use crate::practice::practice_tests::text_length_rules;
    for format in [
        LearningItemFormat::SingleChoice,
        LearningItemFormat::TrueFalse,
        LearningItemFormat::MultiSelect,
        LearningItemFormat::ShortAnswer,
        LearningItemFormat::Cloze,
        LearningItemFormat::Matching,
        LearningItemFormat::Ordering,
        LearningItemFormat::Explanation,
        LearningItemFormat::Artifact,
    ] {
        // A written item's criteria are shaped by the lessons' rubric schema,
        // and a quote must run to a clause to show the item rests on its passage.
        let rules: Vec<String> = text_length_rules(&item_schema(format, 3, 2, 3))
            .into_iter()
            .filter(|rule| {
                !rule.starts_with("/properties/rubric/")
                    && rule.as_str() != "/properties/quote/minLength = 25"
            })
            .collect();
        assert!(rules.is_empty(), "{format:?}: {rules:?}");
    }
}

#[tokio::test]
async fn the_grading_schemas_ask_for_no_text_length() {
    use crate::practice::practice_tests::text_length_rules;
    let criterion = |id: &str| LearningRubricCriterion {
        id: id.into(),
        title: id.into(),
        description: "Explains it.".into(),
        max_points: 2,
    };
    let written = |note: Option<&str>| WrittenAnswer {
        item_id: "q1".into(),
        prompt: "Why does the ore reduce?".into(),
        rubric: vec![criterion("mechanism"), criterion("evidence")],
        answer: "Carbon monoxide takes the oxygen.".into(),
        reference: "CO reduces Fe2O3.".into(),
        learner_note: note.map(str::to_owned),
    };
    let llm = SchemaRecorder(Mutex::new(Vec::new()));
    grade_written(&llm, &[written(None)]).await.unwrap();
    grade_written(&llm, &[written(Some("I named the mechanism."))])
        .await
        .unwrap();
    judge_short_answers(&llm, &[short("a")]).await.unwrap();
    let mut noted = short("b");
    noted.learner_note = Some("I meant the same thing.".into());
    judge_short_answers(&llm, &[noted]).await.unwrap();
    let schemas = llm.0.lock().unwrap();
    assert!(schemas.len() >= 4, "{} schemas were sent", schemas.len());
    for schema in schemas.iter() {
        let rules = text_length_rules(schema);
        assert!(rules.is_empty(), "{rules:?}");
    }
}

#[test]
fn a_long_distractor_rationale_is_kept_whole() {
    let mut authored = need("Reduce iron oxide.".into(), 1);
    authored.requirement.format = LearningItemFormat::SingleChoice;
    authored.requirement.outcome_id = uuid::Uuid::new_v4().to_string();
    let passage = authored.material[0].clone();
    let rationale = "This option names a gas that does not take the oxygen away. ".repeat(12);
    let raw: RawItem = serde_json::from_value(serde_json::json!({
        "prompt": "Which gas takes the oxygen from the ore in a blast furnace?",
        "options": ["Carbon monoxide", "Nitrogen dioxide", "Sulfur dioxide", "Carbon dioxide"],
        "answerIndex": 0,
        "distractorRationales": ["", rationale, "Not a reducing gas here.", "Already fully oxidised."],
        "explanation": "Carbon monoxide takes the oxygen from the oxide.",
        "blockIndex": 0,
        "quote": "iron oxide reduction Fe₂O₃ + 3CO",
        "difficulty": 2
    }))
    .unwrap();
    let item = to_candidate(raw, &authored, &[&passage]).unwrap();
    assert!(item
        .candidate
        .distractor_rationales
        .contains(&rationale.trim().to_string()));
}

/// Keeps the instructions of each request and writes nothing.
struct InstructionRecorder(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl LLMPort for InstructionRecorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, _) = crate::lessons::structured_call::task_messages(request);
        self.0.lock().unwrap().push(system.to_owned());
        Ok(CompletionResponse::from_text("{\"items\":[]}"))
    }
    fn model_name(&self) -> &str {
        "instruction-recorder"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

async fn writer_instructions() -> Result<String> {
    let need = need("Explain iron oxide reduction.".into(), 2);
    let documents = documents(&need);
    let collection = ReferenceCollection::lexical(&documents)?;
    let batch = Batch {
        need: &need,
        count: 1,
        notes: vec![],
        repair: None,
    };
    let llm = InstructionRecorder(Mutex::new(Vec::new()));
    batch.send(&llm, &collection).await?;
    let sent = llm.0.lock().unwrap().clone();
    Ok(sent.into_iter().next().unwrap_or_default())
}

#[tokio::test]
async fn the_writer_is_told_what_the_learner_sees_and_never_to_cite_passages() -> Result<()> {
    let system = writer_instructions().await?;
    for sentence in [
        "The learner sees each question with its options, explanation and rationales beside the lesson, never these passages as excerpts or the sources behind them.",
        "mentions a passage, an excerpt, a tutorial, a document or a source",
        "never give what a passage says or does not say as the reason an answer is right or wrong",
    ] {
        assert!(system.contains(sentence), "missing \"{sentence}\" in {system}");
    }
    Ok(())
}

#[tokio::test]
async fn the_writer_is_told_an_explanation_is_about_its_own_question() -> Result<()> {
    let system = writer_instructions().await?;
    let sentence = "An explanation and its rationales are about the question's own case: they use the names and values its prompt gives, never another example's.";
    assert!(
        system.contains(sentence),
        "missing \"{sentence}\" in {system}"
    );
    Ok(())
}

#[tokio::test]
async fn the_writer_is_told_a_prompt_states_every_value_its_answer_depends_on() -> Result<()> {
    let system = writer_instructions().await?;
    let sentence = "the question depends on a value its prompt never gives";
    assert!(
        system.contains(sentence),
        "missing \"{sentence}\" in {system}"
    );
    Ok(())
}

#[tokio::test]
async fn the_bank_writer_names_no_subjects_values() -> Result<()> {
    let system = writer_instructions().await?;
    let start = system
        .find("An explanation and its rationales are about the question's own case")
        .unwrap_or_default();
    let end = system.find("doNotRepeat lists").unwrap_or_default();
    assert!(start < end, "the two rules are missing from {system}");
    let rules = &system[start..end];
    assert!(!rules.contains("path"), "{rules}");
    assert!(!rules.contains("such as"), "{rules}");
    assert!(
        rules.contains("they use the names and values its prompt gives"),
        "{rules}"
    );
    assert!(
        rules.contains("the question depends on a value its prompt never gives"),
        "{rules}"
    );
    Ok(())
}

#[test]
fn the_writer_is_told_options_are_shuffled_so_they_are_named_by_their_words() {
    assert!(
        system().contains("shuffled order"),
        "SYSTEM must say options are shown in a shuffled order"
    );
    assert!(
        system().contains("names an option by its position, number or letter"),
        "SYSTEM must forbid naming an option by position, number or letter"
    );
}

#[test]
fn the_instructions_say_what_do_not_repeat_requires() {
    assert!(
        system().contains("doNotRepeat"),
        "SYSTEM never names doNotRepeat"
    );
    assert!(
        system().contains("never asks one of them again"),
        "SYSTEM never says a new item does not ask one of its questions again"
    );
}

#[test]
fn do_not_repeat_holds_every_existing_prompt_in_full() {
    let mut need = need("Explain iron oxide reduction.".into(), 1);
    need.existing_prompts = (0..45)
        .map(|i| format!("Question {i}: {}", "which case decides it? ".repeat(12)))
        .collect();
    assert!(need
        .existing_prompts
        .iter()
        .all(|p| p.chars().count() > 160));
    let batch = Batch {
        need: &need,
        count: 1,
        notes: vec![],
        repair: None,
    };

    let skeleton = batch.skeleton();

    let listed: Vec<&str> = skeleton["doNotRepeat"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(listed.len(), need.existing_prompts.len());
    for prompt in &need.existing_prompts {
        assert!(
            listed.contains(&prompt.as_str()),
            "cut or missing: {prompt}"
        );
    }
}

#[test]
fn the_quiz_bank_writer_is_asked_for_choices_alike_in_form() {
    assert!(system().contains("Write all the choices of a question alike in form: close in length and detail, the same grammatical shape and opening, none alone hedged or absolute, none alone carrying a reason or an example, and none repeating the question's distinctive words, so that the right one cannot be told from the others without knowing the material."));
    assert!(!system().contains("distractors of similar length"));
}

const STAMPS: &str =
    "A stamp goes into the box that matches its colour, and a torn stamp goes into the repair tray.";
const STAMP_QUOTE: &str = "goes into the box that matches its colour";
const TRAY: &str = "The repair tray";
const LONG_TRAY: &str =
    "The repair tray, where every torn stamp is set aside before anything else happens to it";
const SORTED: &str = "A torn blue stamp arrives at the desk. Which place takes it first?";
const SORTED_AGAIN: &str =
    "A torn red stamp arrives at the desk. Which place takes it first, and why?";
const GAP: &str = "The question depends on a value it never gives.";

fn stamp_item(prompt: &str, right: &str) -> Value {
    json!({
        "prompt": prompt,
        "options": [right, "The green box", "The red box", "The blue box"],
        "distractorRationales": ["", "A green box takes green stamps.", "The red box takes red stamps.", "The blue box takes blue stamps."],
        "answerIndex": 0,
        "explanation": "A torn stamp is set aside whatever its colour.",
        "blockIndex": 0,
        "quote": STAMP_QUOTE,
        "difficulty": 3
    })
}

/// Writes from a queue, solves by the repair tray, and flags the prompt `flag`.
struct Rewriter {
    writes: Mutex<std::collections::VecDeque<Value>>,
    requests: Mutex<Vec<Value>>,
    flag: &'static str,
}

#[async_trait::async_trait]
impl LLMPort for Rewriter {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let context: Value = serde_json::from_str(user).unwrap_or(Value::Null);
        let reply = if system.starts_with("Write quiz questions") {
            self.requests.lock().unwrap().push(context);
            self.writes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| json!({"items": []}))
        } else if system.starts_with("Solve quiz questions") {
            let answers: Vec<Value> = context["questions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|question| {
                    let right = question["options"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .position(|o| o.as_str().is_some_and(|o| o.starts_with(TRAY)));
                    json!({"index": question["index"], "reason": "Torn stamps are set aside.", "correctIndices": right.into_iter().collect::<Vec<_>>(), "answer": "", "blankAnswers": [], "matches": [], "orderedValues": []})
                })
                .collect();
            json!({"answers": answers})
        } else {
            let checks: Vec<Value> = context["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| {
                    let flagged = item["prompt"] == self.flag;
                    json!({"id": item["id"], "hasDefect": flagged, "finding": if flagged { GAP } else { "" }, "repeats": []})
                })
                .collect();
            json!({"checks": checks})
        };
        Ok(CompletionResponse::from_text(reply.to_string()))
    }
    fn model_name(&self) -> &str {
        "rewriter"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn rewriter(writes: Vec<Value>, flag: &'static str) -> Rewriter {
    Rewriter {
        writes: Mutex::new(writes.into()),
        requests: Mutex::new(Vec::new()),
        flag,
    }
}

fn stamp_need() -> AuthoringNeed {
    AuthoringNeed {
        requirement: LearningBlueprintRequirement {
            outcome_id: uuid::Uuid::new_v4().to_string(),
            format: LearningItemFormat::SingleChoice,
            count: 1,
            difficulty_min: 2,
            difficulty_max: 3,
        },
        outcome_title: "Sorting stamps".into(),
        outcome_description: "Say where a stamp goes by its colour and condition.".into(),
        lesson_id: Some("lesson".into()),
        count: 1,
        pass: BankPass::Retakes,
        material: vec![MaterialPassage {
            lesson_title: "Stamps".into(),
            title: "The boxes".into(),
            text: STAMPS.into(),
            source_version_ids: vec![],
        }],
        existing_prompts: vec![],
    }
}

#[tokio::test]
async fn a_quiz_item_whose_right_choice_is_much_the_longest_is_rewritten_not_dropped() -> Result<()>
{
    let llm = rewriter(
        vec![
            json!({"items": [stamp_item(SORTED, LONG_TRAY)]}),
            json!({"items": [stamp_item(SORTED_AGAIN, TRAY)]}),
        ],
        "",
    );
    let authored = author_items(&llm, &stamp_need()).await?;
    let requests = llm.requests.lock().unwrap();
    let rewrites: Vec<&Value> = requests.iter().filter(|r| !r["repair"].is_null()).collect();
    assert_eq!(rewrites.len(), 1, "rewrite requests: {}", rewrites.len());
    assert!(rewrites[0]["repair"]["findings"]
        .to_string()
        .contains("longest"));
    assert_eq!(authored.items.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_rewrite_that_fails_the_checks_is_dropped_with_its_own_fault() -> Result<()> {
    let llm = rewriter(
        vec![
            json!({"items": [stamp_item(SORTED, TRAY)]}),
            json!({"items": [stamp_item(SORTED_AGAIN, LONG_TRAY)]}),
        ],
        SORTED,
    );
    let authored = author_items(&llm, &stamp_need()).await?;
    assert!(authored.items.is_empty(), "{:?}", authored.items);
    assert!(
        authored
            .dropped
            .iter()
            .any(|reason| reason.contains("longest")),
        "dropped: {:?}",
        authored.dropped
    );
    Ok(())
}

/// Answers a blind solve, and a judge whose first reply misses the item; keeps each judge request whole.
struct Incomplete {
    judged: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl LLMPort for Incomplete {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let rendered = request
            .input
            .iter()
            .flat_map(|item| match item {
                CompletionInput::Message { role, content } => vec![role.clone(), content.clone()],
                _ => Vec::new(),
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let (system, _) = crate::lessons::structured_call::task_messages(request);
        let reply = if system.starts_with("Solve quiz questions") {
            json!({"answers": [{"index": 0, "reason": "It follows.", "correctIndices": [0], "answer": "", "blankAnswers": [], "matches": [], "orderedValues": []}]})
        } else {
            let mut judged = self.judged.lock().unwrap();
            judged.push(rendered);
            if judged.len() == 1 {
                json!({"checks": []})
            } else {
                json!({"checks": [{"id": "new-0", "finding": "", "hasDefect": false, "repeats": []}]})
            }
        };
        Ok(CompletionResponse::from_text(reply.to_string()))
    }
    fn model_name(&self) -> &str {
        "incomplete"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn an_incomplete_item_review_is_corrected_with_why() -> Result<()> {
    let item = crate::assessment_engine::LearningAssessmentCandidate {
        id: uuid::Uuid::new_v4().to_string(),
        outcome_ids: vec![uuid::Uuid::new_v4().to_string()],
        format: LearningItemFormat::SingleChoice,
        difficulty: 3,
        prompt: SORTED.into(),
        options: vec![
            TRAY.into(),
            "The green box".into(),
            "The red box".into(),
            "The blue box".into(),
        ],
        left: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key: LearningAssessmentKey::Choice(0),
        explanation: "A torn stamp is set aside.".into(),
        distractor_rationales: vec![],
    };
    let llm = Incomplete {
        judged: Mutex::new(Vec::new()),
    };
    let entries = [item_review::Entry {
        item: &item,
        quote: STAMP_QUOTE,
    }];
    let context = item_review::Context {
        outcome: "Sorting stamps",
        outcome_description: "Where stamps go.",
        learner_has: &[],
    };
    let findings = item_review::review(&llm, &entries, &context).await?;
    assert!(findings.iter().all(item_review::Findings::is_sound));
    let judged = llm.judged.lock().unwrap();
    assert_eq!(judged.len(), 2);
    assert!(
        judged[1].contains("That answer could not be used"),
        "second request: {}",
        judged[1]
    );
    Ok(())
}

#[test]
fn the_grading_contracts_read_what_they_render() {
    let criterion = |verdict: &str| {
        json!({"criterionId":"c1","yourWords":["a phrase"],"whatWasMissing":"","fullCreditIncludes":"",
            "sourceQuote":"","verdict":verdict,"points":1.0})
    };
    let graded: Vec<grading::RawGradeItem> = serde_json::from_value(json!([
        {"itemId":"i1","criteria":[criterion("met"),criterion("partly_met"),criterion("missed")],"summary":"Fair.","responseToNote":"Noted."}
    ]))
    .unwrap();
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        grading::GradeWritten,
    >(&graded);
    for verdict in ["equivalent", "not_equivalent"] {
        let judged: Vec<grading::RawJudgeItem> = serde_json::from_value(json!([{
            "itemId":"i1","yourWords":[],"whatWasMissing":"","fullCreditIncludes":"",
            "verdict":verdict,"summary":"Judged."
        }]))
        .unwrap();
        lattice_core::application::services::model_contract::test_support::assert_round_trip::<
            grading::JudgeShort,
        >(&judged);
    }
}

#[test]
fn the_quiz_writing_contracts_read_what_they_render() {
    let items: Vec<formats::RawItem> = serde_json::from_value(json!([{
        "prompt":"Which one?","options":["A","B","C","D"],"answerIndex":1,
        "distractorRationales":["No.","","No.","No."],"explanation":"B follows.",
        "blockIndex":0,"quote":"A passage.","difficulty":2,
        "blanks":[{"acceptedAnswers":["x"]}],"pairs":[{"left":"l","right":"r"}],
        "rubric":[
            {"title":"T","description":"D","dimension":"recall"},
            {"title":"T","description":"D","dimension":"explanation"},
            {"title":"T","description":"D","dimension":"application"},
            {"title":"T","description":"D","dimension":"transfer"}
        ]
    }]))
    .unwrap();
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        QuizAuthor,
    >(&items);
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        QuizRewrite,
    >(&items);
}
