use lattice_core::application::ports::llm_port::{CompletionRequest, InferencePriority};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::completion_input::from_context;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub const DEFAULT_INTENT_PROMPT_TEMPLATE: &str = "You classify one turn of a conversation with a \
personal knowledge-base assistant. Decide three things about the user's latest message:
- needsKnowledgeBase: true only if answering requires searching the user's own documents, notes, or files.
- needsWeb: true only if the answer depends on information that cannot be in the user's own files: current events, live data such as prices or weather, or public facts about the outside world that the user did not write about. False for questions about the user's own documents, notes, policies, projects or work, including anything asked about \"our\", \"my\" or \"the company's\" rules, plans or records. When unsure, false.
- isFollowup: true only if the message just continues the recent conversation and cannot stand alone.
Also give a confidence between 0 and 1.

Recent conversation:
{context}

User message: {message}";

/// The answer's shape, sent as the request's schema rather than spelled out in
/// the prompt: a format demand in the prompt is what costs a small model its
/// accuracy, while the grammar holds the shape. No bound here comes near
/// llama.cpp's grammar repetition limit, past which it would drop the grammar.
pub(crate) fn intent_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "needsKnowledgeBase": { "type": "boolean" },
            "needsWeb": { "type": "boolean" },
            "isFollowup": { "type": "boolean" },
            // Range left to the clamp in `classify`: not every provider's
            // structured output accepts numeric bounds.
            "confidence": { "type": "number" }
        },
        "required": ["needsKnowledgeBase", "needsWeb", "isFollowup", "confidence"]
    })
}

pub const DEFAULT_INTENT_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// The utility model's guess at what a turn needs. It can add to the user-set
/// flags and never takes anything away: `needs_knowledge_base` and
/// `is_followup` are ORed in, and `needs_web` only allows a web fallback after
/// the vault came back empty or insufficient.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnIntent {
    pub needs_knowledge_base: bool,
    pub needs_web: bool,
    pub is_followup: bool,
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
            confidence: 0.0,
        }
    }
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
        let bare = render_intent_prompt(
            &self.prompt_template,
            &IntentInput {
                message: message.to_string(),
                recent_context: Vec::new(),
            },
        );
        let room = auxiliary_history_room(self.llm.as_ref(), &bare);
        let turn_cost = |turn: &String| self.llm.count_tokens(turn).saturating_add(1);
        let (recent, omitted) = newest_that_fit(context, room, turn_cost);
        let (recent, omitted) = match (recent.is_empty(), context.last()) {
            (true, Some(newest)) => match self.start_of(newest, room) {
                Some(partial) => (vec![partial], omitted - 1),
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

        match self.complete(prompt).await {
            Ok(text) => {
                let mut intent = parse_turn_intent(&text).unwrap_or_else(|| {
                    warn!("Intent classifier returned unparsable output, falling back");
                    TurnIntent::fallback()
                });

                intent.confidence = intent.confidence.clamp(0.0, 1.0);

                // A guess the model itself does not believe must not steer the
                // turn; the confidence stays on the record for logging.
                if intent.confidence < self.confidence_threshold {
                    intent.needs_knowledge_base = false;
                    intent.needs_web = false;
                    intent.is_followup = false;
                }

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
}

impl IntentClassifier {
    /// One schema-constrained request. A provider that cannot hold the answer
    /// to the schema still gets the prompt, and the lenient parser reads what
    /// comes back.
    async fn complete(&self, prompt: String) -> Result<String> {
        let response = self
            .llm
            .complete(&CompletionRequest {
                input: from_context("", &[], &prompt),
                json_schema: Some(intent_schema()),
                // Part of the turn the user is waiting on: Stop cancels it.
                priority: InferencePriority::Interactive,
                cancel: self.cancel.clone(),
                no_time_limit: true,
                ..Default::default()
            })
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
    Some(text[start..=end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};

    struct MockLLM {
        response: String,
        window: usize,
    }

    impl MockLLM {
        fn new(response: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                response: response.into(),
                window: 4096,
            })
        }

        /// A 32K window, whose history share holds a long chat whole.
        fn wide(response: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                response: response.into(),
                window: 32_768,
            })
        }
    }

    #[async_trait]
    impl LLMPort for MockLLM {
        async fn complete(
            &self,
            request: &CompletionRequest,
        ) -> lattice_core::shared::error::Result<CompletionResponse> {
            Ok(CompletionResponse::from_text(
                self.respond(request.user_text()).await?,
            ))
        }
        fn model_name(&self) -> &str {
            "mock-llm"
        }

        fn max_context_tokens(&self) -> usize {
            self.window
        }

        fn count_tokens(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }

        async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
            Ok(true)
        }
    }
    impl MockLLM {
        async fn respond(&self, _prompt: &str) -> lattice_core::shared::error::Result<String> {
            Ok(self.response.clone())
        }
    }

    fn input() -> IntentInput {
        IntentInput {
            message: "What did my notes say about the launch?".to_string(),
            recent_context: vec!["User: hello".to_string(), "Assistant: hi".to_string()],
        }
    }

    #[test]
    fn prompt_carries_the_message_and_recent_context() {
        let prompt = render_intent_prompt(DEFAULT_INTENT_PROMPT_TEMPLATE, &input());
        assert!(prompt.contains("What did my notes say about the launch?"));
        assert!(prompt.contains("User: hello"));
    }

    /// A question the company's own documents answer is not read as a public one.
    #[test]
    fn needs_web_is_for_what_the_users_files_cannot_hold() {
        let prompt = render_intent_prompt(DEFAULT_INTENT_PROMPT_TEMPLATE, &input());
        let definition = prompt
            .lines()
            .find(|line| line.starts_with("- needsWeb:"))
            .expect("the prompt defines needsWeb");

        for wanted in [
            "cannot be in the user's own files",
            "current events",
            "live data",
            "public facts about the outside world",
            "False for questions about the user's own documents, notes, policies, projects",
            "\"our\", \"my\" or \"the company's\"",
            "When unsure, false.",
        ] {
            assert!(definition.contains(wanted), "needsWeb lost: {wanted}");
        }
        assert!(
            !definition.contains("public information from the web"),
            "the old definition sent any public-sounding question out"
        );
    }

    #[test]
    fn prompt_without_context_says_so() {
        let bare = IntentInput {
            message: "hi".to_string(),
            recent_context: Vec::new(),
        };
        let prompt = render_intent_prompt(DEFAULT_INTENT_PROMPT_TEMPLATE, &bare);
        assert!(prompt.contains("(no prior turns)"));
    }

    #[tokio::test]
    async fn valid_json_parses_into_the_intent() {
        let classifier = IntentClassifier::new(MockLLM::new(
            r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"confidence":0.9}"#,
        ));

        let intent = classifier.classify(&input()).await;

        assert!(intent.needs_knowledge_base);
        assert!(!intent.needs_web);
        assert!(intent.is_followup);
        assert!((intent.confidence - 0.9).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn prose_around_the_json_is_ignored() {
        let classifier = IntentClassifier::new(MockLLM::new(
            "Sure! {\"needsKnowledgeBase\":false,\"needsWeb\":true,\"isFollowup\":false,\"confidence\":0.8} done",
        ));

        let intent = classifier.classify(&input()).await;

        assert!(intent.needs_web);
        assert!(!intent.needs_knowledge_base);
    }

    #[derive(Default)]
    struct RecordingLLM {
        sent: std::sync::Mutex<Vec<CompletionRequest>>,
    }

    #[async_trait]
    impl LLMPort for RecordingLLM {
        async fn complete(
            &self,
            request: &CompletionRequest,
        ) -> lattice_core::shared::error::Result<CompletionResponse> {
            self.sent.lock().unwrap().push(request.clone());
            Ok(CompletionResponse::from_text(
                r#"{"confidence":0.9,"isFollowup":false,"needsKnowledgeBase":true,"needsWeb":false}"#,
            ))
        }
        fn model_name(&self) -> &str {
            "recording-llm"
        }
        fn max_context_tokens(&self) -> usize {
            4096
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }
        async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn the_schema_travels_with_the_request_and_the_prompt_demands_no_format() {
        let llm = Arc::new(RecordingLLM::default());
        let intent = IntentClassifier::new(llm.clone()).classify(&input()).await;

        assert!(intent.needs_knowledge_base);
        let sent = llm.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let request = sent.first().unwrap();
        assert_eq!(request.json_schema, Some(intent_schema()));
        assert!(request.no_time_limit, "no clock cuts the classifier off");
        let prompt = request.user_text();
        for demand in ["JSON", "strict", "Respond with", "no other text", "bool"] {
            assert!(!prompt.contains(demand), "prompt still asks for: {demand}");
        }
        for field in ["needsKnowledgeBase", "needsWeb", "isFollowup", "confidence"] {
            assert!(prompt.contains(field), "prompt no longer explains {field}");
        }
    }

    #[test]
    fn the_schema_requires_every_field_the_parser_reads() {
        let schema = intent_schema();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let parsed: TurnIntent = serde_json::from_value(serde_json::json!({
            "needsKnowledgeBase": true, "needsWeb": false, "isFollowup": false, "confidence": 0.5
        }))
        .unwrap();
        assert!(parsed.needs_knowledge_base);
        assert_eq!(
            required,
            ["needsKnowledgeBase", "needsWeb", "isFollowup", "confidence"]
        );
        assert_eq!(schema["additionalProperties"], false);
    }

    #[tokio::test]
    async fn garbage_falls_back_to_all_false() {
        let classifier = IntentClassifier::new(MockLLM::new("I have no idea."));

        assert_eq!(classifier.classify(&input()).await, TurnIntent::fallback());
    }

    #[tokio::test]
    async fn missing_fields_fall_back_to_all_false() {
        let classifier = IntentClassifier::new(MockLLM::new(
            r#"{"needsKnowledgeBase":true,"confidence":0.9}"#,
        ));

        assert_eq!(classifier.classify(&input()).await, TurnIntent::fallback());
    }

    #[tokio::test]
    async fn low_confidence_clears_the_booleans_but_keeps_the_confidence() {
        let classifier = IntentClassifier::new(MockLLM::new(
            r#"{"needsKnowledgeBase":true,"needsWeb":true,"isFollowup":true,"confidence":0.2}"#,
        ));

        let intent = classifier.classify(&input()).await;

        assert!(!intent.needs_knowledge_base);
        assert!(!intent.needs_web);
        assert!(!intent.is_followup);
        assert!((intent.confidence - 0.2).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn out_of_range_confidence_is_clamped() {
        let classifier = IntentClassifier::new(MockLLM::new(
            r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":false,"confidence":7}"#,
        ));

        let intent = classifier.classify(&input()).await;

        assert_eq!(intent.confidence, 1.0);
        assert!(intent.needs_knowledge_base);
    }

    /// The history is what the classifier's window gives it: newest turns whole
    /// and the rest counted.
    #[test]
    fn history_is_sized_from_the_classifiers_window() {
        use lattice_core::application::services::history_share::auxiliary_history_room;
        let context: Vec<String> = (0..60)
            .map(|turn| format!("turn {turn} {}", "word ".repeat(20)))
            .collect();

        let wide = IntentClassifier::new(MockLLM::wide("{}")).input_for("and then?", &context);
        assert_eq!(wide.recent_context, context, "a 32K window holds it all");

        let narrow = IntentClassifier::new(MockLLM::new("{}"));
        let input = narrow.input_for("and then?", &context);
        assert!(input.recent_context.len() > 1 && input.recent_context.len() < context.len());
        assert!(
            input.recent_context[0].contains("earlier messages not shown"),
            "{:?}",
            input.recent_context
        );
        assert!(input.recent_context.last().unwrap().starts_with("turn 59 "));
        let room = auxiliary_history_room(
            narrow.llm.as_ref(),
            &render_intent_prompt(
                DEFAULT_INTENT_PROMPT_TEMPLATE,
                &IntentInput {
                    message: "and then?".into(),
                    recent_context: Vec::new(),
                },
            ),
        );
        let carried: usize = input.recent_context[1..]
            .iter()
            .map(|turn| narrow.llm.count_tokens(turn) + 1)
            .sum();
        assert!(carried <= room, "{carried} of {room}");
    }

    #[test]
    fn the_conversations_system_prompt_is_not_shown_as_a_turn() {
        let context = [
            "System: You are Lattice, a precise research assistant.".to_string(),
            "User: When does the agreement renew?".to_string(),
            "Assistant: On 2025-10-14 [1].".to_string(),
        ];
        let input =
            IntentClassifier::new(MockLLM::wide("{}")).input_for("And the notice?", &context);
        assert_eq!(input.recent_context, context[1..]);
        let alone = IntentClassifier::new(MockLLM::wide("{}")).input_for("Hi there", &context[..1]);
        assert!(
            alone.recent_context.is_empty(),
            "{:?}",
            alone.recent_context
        );
    }

    /// Answers after `delay`, as a slow local model does.
    struct SlowLLM {
        delay: std::time::Duration,
    }

    #[async_trait]
    impl LLMPort for SlowLLM {
        async fn complete(
            &self,
            _request: &CompletionRequest,
        ) -> lattice_core::shared::error::Result<CompletionResponse> {
            tokio::time::sleep(self.delay).await;
            Ok(CompletionResponse::from_text(
                r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"confidence":0.9}"#,
            ))
        }
        fn model_name(&self) -> &str {
            "slow-llm"
        }
        fn max_context_tokens(&self) -> usize {
            4096
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }
        async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
            Ok(true)
        }
    }

    /// A model that answers after 20 s is slow, not dead: its answer is used.
    #[tokio::test(start_paused = true)]
    async fn a_slow_classifier_is_waited_for() {
        let classifier = IntentClassifier::new(Arc::new(SlowLLM {
            delay: std::time::Duration::from_secs(20),
        }));
        let started = tokio::time::Instant::now();
        let intent = classifier.classify(&input()).await;
        assert!(started.elapsed() >= std::time::Duration::from_secs(20));
        assert!(intent.needs_knowledge_base);
        assert!(intent.is_followup);
        assert!((intent.confidence - 0.9).abs() < f32::EPSILON);
    }

    /// A newest turn longer than the whole room goes in by its start, marked as
    /// continuing, and nothing older.
    #[test]
    fn a_newest_turn_too_long_for_the_room_goes_in_partially_and_marked() {
        use lattice_core::application::services::history_share::auxiliary_history_room;
        let classifier = IntentClassifier::new(MockLLM::new("{}"));
        let room = auxiliary_history_room(
            classifier.llm.as_ref(),
            &render_intent_prompt(
                DEFAULT_INTENT_PROMPT_TEMPLATE,
                &IntentInput {
                    message: "and then?".into(),
                    recent_context: Vec::new(),
                },
            ),
        );
        let context = vec![
            "Assistant: an earlier answer".to_string(),
            format!("Assistant: {}", "word ".repeat(room * 3)),
        ];
        let input = classifier.input_for("and then?", &context);
        assert_eq!(input.recent_context.len(), 2, "{:?}", input.recent_context);
        assert!(input.recent_context[0].contains("1 earlier message not shown"));
        let partial = &input.recent_context[1];
        assert!(partial.starts_with("Assistant: word word"));
        assert!(partial.contains("more characters not shown"), "{partial}");
        assert!(classifier.llm.count_tokens(partial) < room);
    }
}
