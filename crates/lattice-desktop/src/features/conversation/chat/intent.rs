use lattice_core::application::ports::llm_port::{InferencePriority, SamplingOverride};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::completion_input::from_context;
use lattice_core::application::services::model_contract::legacy;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

// What a turn needs, defined once for both prompts that ask for it.
macro_rules! turn_needs {
    () => {
        "- needsKnowledgeBase: true only if answering requires searching or reading the user's own documents, notes, or files.
- needsWeb: true only if the answer depends on information that cannot be in the user's own files: current events, live data such as prices or weather, or public facts about the outside world that the user did not write about. False for questions about the user's own documents, notes, policies, projects or work, including anything asked about \"our\", \"my\" or \"the company's\" rules, plans or records. When unsure, false.
- isFollowup: true only if the message just continues the recent conversation and cannot stand alone.
- standaloneQuestion: the user's latest message as a question that can be understood with no conversation before it, naming in full what it is about.\n"
    };
}

pub const DEFAULT_INTENT_PROMPT_TEMPLATE: &str = concat!(
    "You classify one turn of a conversation with a \
personal knowledge-base assistant. Decide four things about the user's latest message:\n",
    turn_needs!(),
    "Also give a confidence between 0 and 1.\n\nRecent conversation:\n{context}\n\nUser message: {message}"
);

pub const DEFAULT_UNIFIED_TRIAGE_PROMPT_TEMPLATE: &str = concat!(
    "You classify one turn of a conversation \
with a personal knowledge-base assistant. Decide seven things about the user's latest message:\n",
    turn_needs!(),
    "- documentAction: choose \"use_last_document\" when continuing or asking about the recent document, \"new_search\" when asking about other documents or a new topic in the library, \"clarify\" only when genuinely ambiguous whether the user means the recent document or something else, or \"none\" when no document is needed at all.
- clarifyQuestion: a short question naming the recent document when documentAction is \"clarify\", else empty string.
- rationale: a short reason for documentAction.
Also give a confidence between 0 and 1.

Recent conversation:
{context}

Recent document title: {recent_document_title}
User message: {message}"
);

/// The answer's shape, sent as the request's schema rather than spelled out in
/// the prompt: a format demand in the prompt is what costs a small model its
/// accuracy, while the grammar holds the shape. No bound here comes near
/// llama.cpp's grammar repetition limit, past which it would drop the grammar.
pub(crate) fn intent_schema() -> serde_json::Value {
    let mut properties = turn_need_properties();
    // Range left to the clamp in `TurnIntent::settled`: not every provider's
    // structured output accepts numeric bounds.
    properties.insert(
        "confidence".to_string(),
        serde_json::json!({ "type": "number" }),
    );
    object_schema(properties)
}

pub(crate) fn unified_triage_schema() -> serde_json::Value {
    let mut properties = turn_need_properties();
    properties.insert(
        "documentAction".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": ["none", "use_last_document", "new_search", "clarify"]
        }),
    );
    properties.insert(
        "clarifyQuestion".to_string(),
        serde_json::json!({ "type": "string" }),
    );
    properties.insert(
        "rationale".to_string(),
        serde_json::json!({ "type": "string" }),
    );
    properties.insert(
        "confidence".to_string(),
        serde_json::json!({ "type": "number" }),
    );
    object_schema(properties)
}

fn turn_need_properties() -> serde_json::Map<String, serde_json::Value> {
    let mut properties = serde_json::Map::new();
    properties.insert(
        "needsKnowledgeBase".to_string(),
        serde_json::json!({ "type": "boolean" }),
    );
    properties.insert(
        "needsWeb".to_string(),
        serde_json::json!({ "type": "boolean" }),
    );
    properties.insert(
        "isFollowup".to_string(),
        serde_json::json!({ "type": "boolean" }),
    );
    properties.insert(
        "standaloneQuestion".to_string(),
        serde_json::json!({ "type": "string" }),
    );
    properties
}

fn object_schema(properties: serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let required: Vec<String> = properties.keys().cloned().collect();
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": required
    })
}

pub const DEFAULT_INTENT_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// The utility model's guess at what a turn needs. It can add to the user-set
/// flags and never takes anything away: `needs_knowledge_base` and
/// `is_followup` are ORed in, and `needs_web` only allows a web fallback after
/// the vault came back empty or insufficient.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnIntent {
    pub needs_knowledge_base: bool,
    pub needs_web: bool,
    pub is_followup: bool,
    /// A follow-up as a question that stands alone. The search takes it beside
    /// the message's own words, which may not name what they are about.
    #[serde(default)]
    pub standalone_question: Option<String>,
    pub confidence: f32,
}

impl TurnIntent {
    /// The conservative default: behave as if classification never ran, so any
    /// failure reproduces verbatim user flags.
    pub fn fallback() -> Self {
        Self {
            needs_knowledge_base: false,
            needs_web: false,
            is_followup: false,
            standalone_question: None,
            confidence: 0.0,
        }
    }

    /// The intent the turn may act on: a guess the model itself does not
    /// believe must not steer the turn, and a standalone question stands only
    /// for a believed follow-up. The confidence stays on the record for logging.
    fn settled(mut self, confidence_threshold: f32) -> Self {
        self.confidence = self.confidence.clamp(0.0, 1.0);
        if self.confidence < confidence_threshold {
            self.needs_knowledge_base = false;
            self.needs_web = false;
            self.is_followup = false;
        }
        let is_followup = self.is_followup;
        self.standalone_question = self
            .standalone_question
            .map(|question| question.trim().to_string())
            .filter(|question| is_followup && !question.is_empty());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriageDocumentAction {
    #[default]
    None,
    UseLastDocument,
    NewSearch,
    Clarify,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedTurnTriage {
    pub intent: TurnIntent,
    pub document_action: TriageDocumentAction,
    pub clarify_question: Option<String>,
    pub rationale: Option<String>,
    #[serde(default)]
    pub used_fallback: bool,
}

#[derive(Debug, Clone)]
pub struct IntentInput {
    pub message: String,
    /// Most recent conversation turns, oldest first, formatted "Role: content".
    pub recent_context: Vec<String>,
}

/// Asks the utility model what a turn needs. No clock bounds the call: the
/// user's Stop cancels it, and the adapter's stall detection ends one whose
/// server has gone quiet. Any error falls back to the user's own flags.
pub struct IntentClassifier {
    llm: Arc<dyn LLMPort>,
    prompt_template: String,
    confidence_threshold: f32,
    cancel: Option<CancellationToken>,
}

impl IntentClassifier {
    pub fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self::with_config(
            llm,
            DEFAULT_INTENT_PROMPT_TEMPLATE.to_string(),
            DEFAULT_INTENT_CONFIDENCE_THRESHOLD,
        )
    }

    pub fn with_config(
        llm: Arc<dyn LLMPort>,
        prompt_template: String,
        confidence_threshold: f32,
    ) -> Self {
        Self {
            llm,
            prompt_template,
            confidence_threshold,
            cancel: None,
        }
    }

    /// Stop the classification when the turn is stopped.
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// The input for `message` with as much of `context`'s turns (oldest
    /// first; its system prompt is not one) as this classifier's window gives
    /// history: the newest turns, whole, and a
    /// line counting any earlier ones left out. A newest turn too long for the room on its
    /// own is not dropped: its start goes in, marked as continuing, since the
    /// turn a follow-up follows is the one thing the classifier must see.
    pub fn input_for(&self, message: &str, context: &[String]) -> IntentInput {
        let bare = render_intent_prompt(
            &self.prompt_template,
            &IntentInput {
                message: message.to_string(),
                recent_context: Vec::new(),
            },
        );
        self.input_for_bare(message, context, &bare)
    }

    pub fn input_for_recent_document(
        &self,
        message: &str,
        context: &[String],
        recent_document_title: &str,
    ) -> IntentInput {
        let bare = render_unified_triage_prompt(
            DEFAULT_UNIFIED_TRIAGE_PROMPT_TEMPLATE,
            &IntentInput {
                message: message.to_string(),
                recent_context: Vec::new(),
            },
            recent_document_title,
        );
        self.input_for_bare(message, context, &bare)
    }

    fn input_for_bare(&self, message: &str, context: &[String], bare: &str) -> IntentInput {
        use lattice_core::application::services::completion_input::is_system_entry;
        use lattice_core::application::services::history_share::{
            auxiliary_history_room, newest_that_fit, omitted_note,
        };
        let turns: Vec<String> = context
            .iter()
            .filter(|entry| !is_system_entry(entry))
            .cloned()
            .collect();
        let context = turns.as_slice();
        let room = auxiliary_history_room(self.llm.as_ref(), bare);
        let turn_cost = |turn: &String| self.llm.count_tokens(turn).saturating_add(1);
        let (recent, omitted) = newest_that_fit(context, room, turn_cost);
        let (recent, omitted) = match (recent.is_empty(), context.last()) {
            (true, Some(newest)) => match self.start_of(newest, room) {
                Some(partial) => (vec![partial], omitted.saturating_sub(1)),
                None => (Vec::new(), omitted),
            },
            _ => (recent.to_vec(), omitted),
        };
        IntentInput {
            message: message.to_string(),
            recent_context: omitted_note(omitted).into_iter().chain(recent).collect(),
        }
    }

    /// As much of the start of `turn` as fits `room` tokens, with a note of
    /// how much more there is; `None` when not even a little fits.
    fn start_of(&self, turn: &str, room: usize) -> Option<String> {
        use lattice_core::application::ports::llm_port::chars_within_tokens;
        let note =
            |rest: usize| format!(" … (message continues; {rest} more characters not shown)");
        let room = room.saturating_sub(self.llm.count_tokens(&note(turn.len())) + 1);
        let chars = chars_within_tokens(room, self.llm.chars_per_token());
        if chars == 0 {
            return None;
        }
        let end = turn
            .char_indices()
            .nth(chars)
            .map_or(turn.len(), |(index, _)| index);
        let mut head = turn[..end].to_string();
        // A word-counting tokenizer can still find the head too long; shed
        // words until it fits rather than send past the room.
        while self.llm.count_tokens(&head) > room {
            match head.trim_end().rfind(char::is_whitespace) {
                Some(cut) if cut > 0 => head.truncate(cut),
                _ => return None,
            }
        }
        let rest = turn[head.len()..].chars().count();
        Some(format!("{head}{}", note(rest)))
    }

    pub async fn classify(&self, input: &IntentInput) -> TurnIntent {
        let prompt = render_intent_prompt(&self.prompt_template, input);

        match self.complete_with_schema(prompt, intent_schema()).await {
            Ok(text) => {
                let intent = parse_turn_intent(&text)
                    .unwrap_or_else(|| {
                        warn!("Intent classifier returned unparsable output, falling back");
                        TurnIntent::fallback()
                    })
                    .settled(self.confidence_threshold);

                info!(
                    needs_knowledge_base = intent.needs_knowledge_base,
                    needs_web = intent.needs_web,
                    is_followup = intent.is_followup,
                    confidence = intent.confidence,
                    "Turn intent classification"
                );

                intent
            }
            Err(e) => {
                warn!(error = %e, "Intent classifier LLM failed, falling back");
                TurnIntent::fallback()
            }
        }
    }

    pub async fn triage_with_recent_document(
        &self,
        input: &IntentInput,
        recent_document_title: &str,
    ) -> UnifiedTurnTriage {
        let prompt = render_unified_triage_prompt(
            DEFAULT_UNIFIED_TRIAGE_PROMPT_TEMPLATE,
            input,
            recent_document_title,
        );
        match self
            .complete_with_schema(prompt, unified_triage_schema())
            .await
        {
            Ok(text) => match parse_unified_triage(&text, self.confidence_threshold) {
                Some(triage) => {
                    info!(
                        needs_knowledge_base = triage.intent.needs_knowledge_base,
                        needs_web = triage.intent.needs_web,
                        is_followup = triage.intent.is_followup,
                        document_action = ?triage.document_action,
                        confidence = triage.intent.confidence,
                        "Unified turn triage"
                    );
                    triage
                }
                None => {
                    warn!("Unified triage returned unparsable output, falling back");
                    unified_triage_fallback()
                }
            },
            Err(e) => {
                warn!(error = %e, "Unified triage LLM failed, falling back");
                unified_triage_fallback()
            }
        }
    }
}

impl IntentClassifier {
    /// One schema-constrained request. A provider that cannot hold the answer
    /// to the schema still gets the prompt, and the lenient parser reads what
    /// comes back.
    async fn complete_with_schema(
        &self,
        prompt: String,
        schema: serde_json::Value,
    ) -> Result<String> {
        let response = self
            .llm
            .complete(&legacy::completion(legacy::Completion {
                input: from_context("", &[], &prompt),
                json_schema: Some(schema),
                sampling: Some(SamplingOverride::deterministic()),
                // Part of the turn the user is waiting on: Stop cancels it.
                priority: InferencePriority::Interactive,
                cancel: self.cancel.clone(),
                no_time_limit: true,
                ..Default::default()
            }))
            .await?;
        if !response.tool_calls.is_empty() {
            return Err(AppError::InvalidState(
                "Unexpected tool call in intent classification".into(),
            ));
        }
        Ok(response.text)
    }
}

fn render_intent_prompt(template: &str, input: &IntentInput) -> String {
    let context = if input.recent_context.is_empty() {
        "(no prior turns)".to_string()
    } else {
        input.recent_context.join("\n")
    };
    template
        .replace("{message}", input.message.trim())
        .replace("{context}", &context)
}

fn render_unified_triage_prompt(
    template: &str,
    input: &IntentInput,
    recent_document_title: &str,
) -> String {
    render_intent_prompt(template, input)
        .replace("{recent_document_title}", recent_document_title.trim())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUnifiedTriage {
    needs_knowledge_base: bool,
    needs_web: bool,
    is_followup: bool,
    #[serde(default)]
    standalone_question: Option<String>,
    #[serde(default, alias = "document_action")]
    document_action: Option<TriageDocumentAction>,
    #[serde(default, alias = "clarify_question")]
    clarify_question: Option<String>,
    #[serde(default)]
    rationale: Option<String>,
    confidence: f32,
}

fn unified_triage_fallback() -> UnifiedTurnTriage {
    UnifiedTurnTriage {
        intent: TurnIntent::fallback(),
        document_action: TriageDocumentAction::NewSearch,
        clarify_question: None,
        rationale: Some("fallback".to_string()),
        used_fallback: true,
    }
}

fn parse_unified_triage(text: &str, confidence_threshold: f32) -> Option<UnifiedTurnTriage> {
    let json = extract_json_object(text)?;
    let raw: RawUnifiedTriage = serde_json::from_str(&json).ok()?;
    let intent = TurnIntent {
        needs_knowledge_base: raw.needs_knowledge_base,
        needs_web: raw.needs_web,
        is_followup: raw.is_followup,
        standalone_question: raw.standalone_question,
        confidence: raw.confidence,
    }
    .settled(confidence_threshold);
    let document_action =
        raw.document_action
            .unwrap_or(if intent.needs_knowledge_base || intent.is_followup {
                TriageDocumentAction::NewSearch
            } else {
                TriageDocumentAction::None
            });
    Some(UnifiedTurnTriage {
        intent,
        document_action,
        clarify_question: non_blank(raw.clarify_question),
        rationale: non_blank(raw.rationale),
        used_fallback: false,
    })
}

fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn parse_turn_intent(text: &str) -> Option<TurnIntent> {
    let json = extract_json_object(text)?;
    serde_json::from_str(&json).ok()
}

fn extract_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    text.get(start..=end).map(str::to_string)
}

#[cfg(test)]
#[path = "intent_tests.rs"]
mod tests;
