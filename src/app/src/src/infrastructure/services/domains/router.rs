use crate::application::ports::LLMPort;
use crate::features::settings::dto::RouterSettingsDto;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
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
    #[serde(default)]
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

#[derive(Debug, Clone)]
pub struct RouterInput {
    pub query: String,
    pub has_recent_document: bool,
    pub recent_document_title: Option<String>,
    pub recent_document_id: Option<String>,
}

pub struct RouterService {
    llm: Arc<dyn LLMPort>,
    settings: RouterSettingsDto,
}

impl RouterService {
    pub fn new(llm: Arc<dyn LLMPort>, settings: RouterSettingsDto) -> Self {
        Self { llm, settings }
    }

    pub async fn route(&self, input: RouterInput) -> RouterDecision {
        if !self.settings.enabled {
            return RouterDecision::fallback(&input, &self.settings);
        }

        let prompt = render_router_prompt(&self.settings.prompt_template, &input);
        let timeout_ms = self.settings.timeout_ms.max(1);

        let result = timeout(Duration::from_millis(timeout_ms), async {
            self.llm.generate(&prompt, &[], None).await
        })
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

                if decision.confidence < self.settings.ambiguity_threshold {
                    decision.action = RouterAction::Clarify;
                }

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
        .replace(
            "{recent_document_title}",
            input.recent_document_title.as_deref().unwrap_or(""),
        )
        .replace(
            "{recent_document_id}",
            input.recent_document_id.as_deref().unwrap_or(""),
        )
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
            recent_document_id: None,
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
            recent_document_id: Some("doc-1".to_string()),
        };

        let prompt = render_clarify_prompt(&settings, &input);
        assert!(prompt.contains("NDA.pdf"));
    }
}
