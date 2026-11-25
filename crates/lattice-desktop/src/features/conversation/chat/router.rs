use lattice_core::application::contracts::settings::RouterSettingsDto;
use lattice_core::application::ports::llm_port::InferencePriority;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::completion_input::{complete_text, TextCall};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

const CLARIFY_NO_RECENT_DOCUMENT_PROMPT: &str =
    "I don't see a recent linked document yet. Do you want me to search your documents, or answer this generally?";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouterAction {
    UseLastDocument,
    NewSearch,
    Clarify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterDecision {
    pub action: RouterAction,
    pub confidence: f32,
    /// The routing prompt asks for `clarify_question`.
    #[serde(default, alias = "clarify_question")]
    pub clarify_question: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
}

impl RouterDecision {
    pub fn fallback(_input: &RouterInput, _settings: &RouterSettingsDto) -> Self {
        Self {
            action: RouterAction::NewSearch,
            confidence: 0.0,
            clarify_question: None,
            rationale: Some("fallback".to_string()),
        }
    }
}

/// What an answer the router is unsure of does. Going back to the last
/// document on a guess is asked about first; a fresh search loses nothing, so
/// it goes ahead.
pub(crate) fn settle_unsure(
    action: RouterAction,
    confidence: f32,
    ambiguity_threshold: f32,
) -> RouterAction {
    if action == RouterAction::UseLastDocument && confidence < ambiguity_threshold {
        RouterAction::Clarify
    } else {
        action
    }
}

#[derive(Debug, Clone)]
pub struct RouterInput {
    pub query: String,
    pub has_recent_document: bool,
    pub recent_document_title: Option<String>,
}

pub struct RouterService {
    llm: Arc<dyn LLMPort>,
    settings: RouterSettingsDto,
    cancel: Option<CancellationToken>,
}

impl RouterService {
    pub fn new(llm: Arc<dyn LLMPort>, settings: RouterSettingsDto) -> Self {
        Self {
            llm,
            settings,
            cancel: None,
        }
    }

    /// Stop the routing call when the turn is stopped.
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub async fn route(&self, input: RouterInput) -> RouterDecision {
        if !self.settings.enabled {
            return RouterDecision::fallback(&input, &self.settings);
        }

        let prompt = render_router_prompt(&self.settings.prompt_template, &input);
        let timeout_ms = self.settings.timeout_ms.max(1);

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
                    ..Default::default()
                },
            ),
        )
        .await;

        match result {
            Ok(Ok(text)) => {
                let mut decision = parse_router_decision(&text).unwrap_or_else(|| {
                    warn!("Router returned unparsable output, falling back");
                    RouterDecision::fallback(&input, &self.settings)
                });

                decision.confidence = decision.confidence.clamp(0.0, 1.0);

                if decision.action == RouterAction::UseLastDocument && !input.has_recent_document {
                    decision.action = RouterAction::NewSearch;
                }

                decision.action = settle_unsure(
                    decision.action,
                    decision.confidence,
                    self.settings.ambiguity_threshold,
                );

                if decision.action == RouterAction::Clarify && !input.has_recent_document {
                    decision.action = RouterAction::NewSearch;
                    decision.clarify_question = None;
                }

                if decision.action == RouterAction::Clarify {
                    let clarify = decision
                        .clarify_question
                        .clone()
                        .filter(|q| !q.trim().is_empty())
                        .unwrap_or_else(|| render_clarify_prompt(&self.settings, &input));
                    decision.clarify_question = Some(clarify);
                }

                info!(
                    action = ?decision.action,
                    confidence = decision.confidence,
                    "Router decision"
                );

                decision
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Router LLM failed, falling back");
                RouterDecision::fallback(&input, &self.settings)
            }
            Err(_) => {
                warn!("Router timed out, falling back");
                RouterDecision::fallback(&input, &self.settings)
            }
        }
    }
}

fn render_router_prompt(template: &str, input: &RouterInput) -> String {
    let title = input.recent_document_title.as_deref().unwrap_or("");
    template
        .replace("{query}", input.query.trim())
        .replace(
            "{has_recent_document}",
            if input.has_recent_document {
                "true"
            } else {
                "false"
            },
        )
        .replace("{recent_document_title}", title)
        .replace("{recent_document_id}", title)
}

fn render_clarify_prompt(settings: &RouterSettingsDto, input: &RouterInput) -> String {
    if !input.has_recent_document {
        return CLARIFY_NO_RECENT_DOCUMENT_PROMPT.to_string();
    }

    settings.clarify_prompt_template.replace(
        "{recent_document_title}",
        input
            .recent_document_title
            .as_deref()
            .unwrap_or("the recent document"),
    )
}

fn parse_router_decision(text: &str) -> Option<RouterDecision> {
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

    #[test]
    fn clarify_prompt_without_recent_document_is_generic() {
        let settings = RouterSettingsDto::default();
        let input = RouterInput {
            query: "help".to_string(),
            has_recent_document: false,
            recent_document_title: None,
        };

        let prompt = render_clarify_prompt(&settings, &input);
        assert!(prompt.contains("I don't see a recent linked document"));
        assert!(!prompt.contains("previous document"));
    }

    #[test]
    fn clarify_prompt_with_recent_document_uses_template() {
        let settings = RouterSettingsDto::default();
        let input = RouterInput {
            query: "help".to_string(),
            has_recent_document: true,
            recent_document_title: Some("NDA.pdf".to_string()),
        };

        let prompt = render_clarify_prompt(&settings, &input);
        assert!(prompt.contains("NDA.pdf"));
    }

    #[test]
    fn the_models_own_clarifying_question_is_kept_under_the_key_the_prompt_asks_for() {
        let decision = parse_router_decision(
            r#"{"action":"clarify","confidence":0.4,"clarify_question":"Do you mean the NDA you opened?","rationale":"ambiguous"}"#,
        )
        .expect("a decision");
        assert_eq!(
            decision.clarify_question.as_deref(),
            Some("Do you mean the NDA you opened?")
        );
    }

    fn tide_input() -> RouterInput {
        RouterInput {
            query: "what next".to_string(),
            has_recent_document: true,
            recent_document_title: Some("Tide tables".to_string()),
        }
    }

    #[test]
    fn the_default_template_does_not_show_a_document_id() {
        let template = RouterSettingsDto::default().prompt_template;
        let prompt = render_router_prompt(&template, &tide_input());
        assert!(prompt.contains("Recent document title: Tide tables"));
        assert!(!prompt.contains("Recent document id"));
    }

    #[test]
    fn a_saved_template_with_the_id_placeholder_shows_the_title() {
        let prompt = render_router_prompt("Document: {recent_document_id}", &tide_input());
        assert_eq!(prompt, "Document: Tide tables");
    }
}
