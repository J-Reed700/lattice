use super::*;
use crate::features::conversation::chat::reply_text::markers;
use crate::features::conversation::chat::ChatEventSink;
use crate::features::conversation::chat::ChatStreamEventDto;
use lattice_core::application::services::model_contract::kept::TurnNames;
use std::sync::Arc;

/// A model whose first attempt stops inside what could be a written call
/// marker, and whose retry answers in full.
struct RetriesOnce;

#[async_trait::async_trait]
impl LLMPort for RetriesOnce {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse::from_text("unused"))
    }

    async fn complete_with_reasoning_progress(
        &self,
        _request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        _on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        on_text("Hello <tool_c".to_owned())?;
        on_retry(1)?;
        on_text("all it takes.".to_owned())?;
        Ok(CompletionResponse::from_text("all it takes."))
    }

    fn model_name(&self) -> &str {
        "retrying"
    }

    fn max_context_tokens(&self) -> usize {
        8192
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_retry_after_a_held_partial_call_marker_does_not_prepend_it_to_the_retried_text() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event: ChatStreamEventDto| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    let emitter = StreamEmitter::new(
        &sink,
        "conversation",
        "request",
        Arc::new(TurnNames::unstored()),
        markers(false),
    );
    let recorder = TurnRecorder::silent("conversation", "request");
    let mut request = CompletionRequest::probe();
    generate(
        &RetriesOnce,
        &mut request,
        &emitter,
        &recorder,
        &CancellationToken::new(),
        None,
        "Thinking",
        WrittenCalls::Cut,
    )
    .await
    .unwrap();
    let shown: String = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| event.content.clone())
        .collect();
    assert_eq!(shown, "Hello all it takes.");
}
