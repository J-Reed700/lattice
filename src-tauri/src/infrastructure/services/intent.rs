use crate::application::ports::llm_port::InferencePriority;
use crate::application::ports::LLMPort;
use crate::application::services::completion_input::{complete_text, TextCall};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub const DEFAULT_INTENT_PROMPT_TEMPLATE: &str = "You classify one turn of a conversation with a \
personal knowledge-base assistant. Decide three things about the user's latest message:
- needsKnowledgeBase: true only if answering requires searching the user's own documents, notes, or files.
- needsWeb: true only if answering requires current or public information from the web.
- isFollowup: true only if the message just continues the recent conversation and cannot stand alone.
Also give a confidence between 0 and 1.

Recent conversation:
{context}

User message: {message}

Respond with strict JSON only, no other text:
{\"needsKnowledgeBase\":bool,\"needsWeb\":bool,\"isFollowup\":bool,\"confidence\":number}";

pub const DEFAULT_INTENT_CONFIDENCE_THRESHOLD: f32 = 0.5;
const DEFAULT_INTENT_TIMEOUT_MS: u64 = 15_000;

/// The utility model's guess at what a turn needs. Enable-only: the caller ORs
/// these into the user-set flags, so a wrong `false` costs nothing and a wrong
/// `true` costs one unneeded search — never a missing one.
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

pub struct IntentClassifier {
    llm: Arc<dyn LLMPort>,
    prompt_template: String,
    confidence_threshold: f32,
    timeout_ms: u64,
    cancel: Option<CancellationToken>,
}

impl IntentClassifier {
    pub fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self::with_config(
            llm,
            DEFAULT_INTENT_PROMPT_TEMPLATE.to_string(),
            DEFAULT_INTENT_CONFIDENCE_THRESHOLD,
            DEFAULT_INTENT_TIMEOUT_MS,
        )
    }

    pub fn with_config(
        llm: Arc<dyn LLMPort>,
        prompt_template: String,
        confidence_threshold: f32,
        timeout_ms: u64,
    ) -> Self {
        Self {
            llm,
            prompt_template,
            confidence_threshold,
            timeout_ms,
            cancel: None,
        }
    }

    /// Stop the classification when the turn is stopped.
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub async fn classify(&self, input: &IntentInput) -> TurnIntent {
        let prompt = render_intent_prompt(&self.prompt_template, input);
        let timeout_ms = self.timeout_ms.max(1);

        // Part of the turn the user is waiting on.
        let result = timeout(
            Duration::from_millis(timeout_ms),
            complete_text(
                self.llm.as_ref(),
                &prompt,
                &[],
                TextCall {
                    priority: InferencePriority::Interactive,
                    cancel: self.cancel.clone(),
                    cache_key: None,
                },
            ),
        )
        .await;

        match result {
            Ok(Ok(text)) => {
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
            Ok(Err(e)) => {
                warn!(error = %e, "Intent classifier LLM failed, falling back");
                TurnIntent::fallback()
            }
            Err(_) => {
                warn!("Intent classifier timed out, falling back");
                TurnIntent::fallback()
            }
        }
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
    use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use async_trait::async_trait;

    struct MockLLM {
        response: String,
    }

    impl MockLLM {
        fn new(response: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                response: response.into(),
            })
        }
    }

    #[async_trait]
    impl LLMPort for MockLLM {
        async fn complete(
            &self,
            request: &CompletionRequest,
        ) -> crate::shared::error::Result<CompletionResponse> {
            Ok(CompletionResponse::from_text(
                self.respond(request.user_text()).await?,
            ))
        }
        fn model_name(&self) -> &str {
            "mock-llm"
        }

        fn max_context_tokens(&self) -> usize {
            4096
        }

        fn count_tokens(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }

        async fn is_ready(&self) -> crate::shared::error::Result<bool> {
            Ok(true)
        }
    }
    impl MockLLM {
        async fn respond(&self, _prompt: &str) -> crate::shared::error::Result<String> {
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
}
