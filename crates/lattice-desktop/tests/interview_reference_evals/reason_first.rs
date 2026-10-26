//! Experimental test-only mirror of Learning's private two-step protocol.
//! This does not enable the production ReasonFirst path or checkpoint handling.
use lattice_core::{
    application::ports::{
        llm_port::{CompletionInput, CompletionRequest, CompletionResponse, SamplingOverride},
        LLMPort,
    },
    shared::error::Result,
};

struct Candidate {
    inner: Box<dyn LLMPort>,
}

pub(super) fn candidate(inner: Box<dyn LLMPort>) -> Box<dyn LLMPort> {
    Box::new(Candidate { inner })
}

#[async_trait::async_trait]
impl LLMPort for Candidate {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let system = request
            .input
            .iter()
            .find_map(|item| match item {
                CompletionInput::Message { role, content } if role == "system" => {
                    Some(content.as_str())
                }
                _ => None,
            })
            .unwrap_or_default();
        let brief = if system.starts_with("Prepare a private interview answer standard") {
            "Solve the exact task from the actual code and supplied evidence. Check compilation, borrowing guarantees, unsupported premises, valid alternatives and the performance that should earn credit."
        } else if system.starts_with("Independently check an interview question") {
            "Independently analyze the actual code and assumptions before deciding. Check compilation, mutability and exclusive borrowing before runtime hypotheses; compare the proposed answer, citation meaning and fair credit against that analysis."
        } else {
            return self.inner.complete(request).await;
        };
        // A correction already carries an assistant answer; let the production
        // correction proceed rather than inventing a new draft for that step.
        if request.json_schema.is_none()
            || request.input.iter().any(
                |item| matches!(item, CompletionInput::Message { role, .. } if role == "assistant"),
            )
        {
            return self.inner.complete(request).await;
        }
        let mut draft_request = request.clone();
        draft_request.json_schema = None;
        for item in &mut draft_request.input {
            if let CompletionInput::Message { role, content } = item {
                if role == "system" {
                    content.push_str(&format!("\n\nAnswer in two steps. Step 1, now: work through the task in prose. {brief} Write out everything the final answer needs, in full. Step 2 is requested separately and turns this draft into the final answer."));
                }
            }
        }
        let draft = self.inner.complete(&draft_request).await?;
        if draft.text.trim().is_empty() {
            return self.inner.complete(request).await;
        }
        let mut answer = draft_request;
        answer.json_schema = request.json_schema.clone();
        answer.sampling = Some(SamplingOverride::deterministic());
        answer.input.push(CompletionInput::Message {
            role: "assistant".into(),
            content: draft.text,
        });
        answer.input.push(CompletionInput::Message {
            role:"user".into(),
            content:"Step 2: give the final answer from your draft. Keep its content, wording, numbers, citations and answers. Copy quotations and identifiers exactly as the task supplies them. Where the draft left something out or stopped early, complete it from the task.".into(),
        });
        self.inner.complete(&answer).await
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    fn chars_per_token(&self) -> f64 {
        self.inner.chars_per_token()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.inner.count_tokens_exact(text).await
    }
    fn counts_tokens_exactly(&self) -> bool {
        self.inner.counts_tokens_exactly()
    }
    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }
    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
}
