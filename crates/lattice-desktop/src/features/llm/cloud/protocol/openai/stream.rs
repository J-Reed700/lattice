//! OpenAI's Responses stream events.

use lattice_core::shared::error::Result;
use serde_json::Value;

use super::super::EventDecoder;
use crate::features::llm::cloud::field;
use crate::features::llm::cloud::streaming::{invalid, Callback, ReasoningParts};

#[derive(Default)]
pub(super) struct Events {
    completed: Option<Value>,
    reasoning: ReasoningParts,
    begun: bool,
}

impl EventDecoder for Events {
    fn event(
        &mut self,
        event: &Value,
        kind: &str,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        match kind {
            "response.output_item.added" => self.begun = true,
            "response.output_text.delta" | "response.refusal.delta" => {
                if let Some(text) = field(event, "delta")
                    .as_str()
                    .filter(|text| !text.is_empty())
                {
                    self.begun = true;
                    on_text(text.to_owned())?;
                }
            }
            "response.reasoning_summary_text.delta" => {
                if let Some(text) = field(event, "delta")
                    .as_str()
                    .filter(|text| !text.is_empty())
                {
                    self.begun = true;
                    let part = (
                        field(event, "item_id").as_str().unwrap_or("").to_owned(),
                        field(event, "summary_index").as_u64().unwrap_or(0),
                    );
                    self.reasoning.push(part, text, on_reasoning)?;
                }
            }
            "response.completed" | "response.incomplete" => {
                let response = field(event, "response");
                if !response.is_object() || !field(response, "output").is_array() {
                    return Err(invalid("missing terminal response"));
                }
                // The terminal response contains complete tool arguments,
                // encrypted reasoning, annotations and final token usage.
                self.completed = Some(response.clone());
            }
            _ => {}
        }
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.completed.is_some()
    }

    fn is_failure(&self, kind: &str) -> bool {
        matches!(kind, "error" | "response.failed")
    }

    fn has_tool_output(&self) -> bool {
        false
    }

    fn answer_begun(&self) -> bool {
        self.begun
    }

    fn finish(self: Box<Self>) -> Result<Option<Value>> {
        Ok(self.completed)
    }
}
