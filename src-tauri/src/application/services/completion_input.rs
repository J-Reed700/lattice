//! Assemble role-bearing conversation context once for provider adapters.
use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, InferencePriority};
use crate::application::ports::LLMPort;
use crate::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

/// How a plain-text model call queues at the backend.
#[derive(Debug, Clone, Default)]
pub struct TextCall {
    pub priority: InferencePriority,
    pub cancel: Option<CancellationToken>,
    pub cache_key: Option<String>,
}

impl TextCall {
    pub fn at(priority: InferencePriority) -> Self {
        Self {
            priority,
            ..Default::default()
        }
    }
}

/// Plain text for `prompt` with "Role: content" `context`, queued as `call`
/// says: the typed request carries its priority, cancellation and cache key
/// to the backend's scheduler.
pub async fn complete_text(
    llm: &dyn LLMPort,
    prompt: &str,
    context: &[String],
    call: TextCall,
) -> Result<String> {
    let response = llm
        .complete(&CompletionRequest {
            input: from_context("", context, prompt),
            priority: call.priority,
            cancel: call.cancel,
            cache_key: call.cache_key,
            ..Default::default()
        })
        .await?;
    if !response.tool_calls.is_empty() {
        return Err(AppError::InvalidState(
            "Unexpected tool call in text generation".into(),
        ));
    }
    Ok(response.text)
}

/// Explicit context instructions already reflect conversation > space > global
/// precedence. Use the default only when context supplies no system instruction.
/// Keep history roles intact and combine supplemental instructions at the front.
pub fn from_context(
    default_system: &str,
    context: &[String],
    prompt: &str,
) -> Vec<CompletionInput> {
    let mut instructions = Vec::new();
    let mut supplemental = Vec::new();
    let mut messages = Vec::new();
    for entry in context {
        let parsed = entry.split_once(':').and_then(|(role, content)| {
            let role = role.trim().to_ascii_lowercase();
            matches!(role.as_str(), "system" | "user" | "assistant").then(|| (role, content.trim()))
        });
        match parsed {
            Some((role, content)) if role == "system" => {
                add_instruction(&mut instructions, content)
            }
            Some((role, content)) => messages.push(CompletionInput::Message {
                role,
                content: content.to_owned(),
            }),
            None => add_instruction(&mut supplemental, entry),
        }
    }
    if instructions.is_empty() {
        add_instruction(&mut instructions, default_system);
    }
    for content in supplemental {
        add_instruction(&mut instructions, &content);
    }
    if !instructions.is_empty() {
        messages.insert(
            0,
            CompletionInput::Message {
                role: "system".into(),
                content: instructions.join("\n\n"),
            },
        );
    }
    messages.push(CompletionInput::Message {
        role: "user".into(),
        content: prompt.to_owned(),
    });
    messages
}

fn add_instruction(instructions: &mut Vec<String>, content: &str) {
    let content = content.trim();
    if !content.is_empty() && !instructions.iter().any(|existing| existing == content) {
        instructions.push(content.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::llm_port::CompletionResponse;
    use async_trait::async_trait;

    /// Records the typed request it was sent.
    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Option<CompletionRequest>>);

    #[async_trait]
    impl LLMPort for Recorder {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            *self.0.lock().unwrap() = Some(request.clone());
            Ok(CompletionResponse::from_text("answer"))
        }
        fn model_name(&self) -> &str {
            "recorder"
        }
        fn max_context_tokens(&self) -> usize {
            4096
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn a_plain_text_call_reaches_the_backend_with_its_priority() {
        let llm = Recorder::default();
        let text = complete_text(
            &llm,
            "Label this",
            &["System: JSON only".into()],
            TextCall::at(InferencePriority::Maintenance),
        )
        .await
        .unwrap();
        assert_eq!(text, "answer");
        let sent = llm.0.lock().unwrap().clone().unwrap();
        assert_eq!(sent.priority, InferencePriority::Maintenance);
        assert_eq!(
            messages(sent.input),
            vec![
                ("system".into(), "JSON only".into()),
                ("user".into(), "Label this".into()),
            ]
        );
    }

    fn messages(input: Vec<CompletionInput>) -> Vec<(String, String)> {
        input
            .into_iter()
            .map(|item| match item {
                CompletionInput::Message { role, content } => (role, content),
                _ => panic!("unexpected native input"),
            })
            .collect()
    }

    #[test]
    fn selected_instructions_replace_defaults_and_retries_keep_history_roles() {
        let context = [
            "System: Conversation override",
            "User: First question",
            "Assistant: First answer",
            "System: Retry instruction",
            "System: Conversation override",
            "Supplemental context",
        ]
        .map(str::to_owned);
        assert_eq!(
            messages(from_context("Global default", &context, "Follow-up")),
            vec![
                (
                    "system".into(),
                    "Conversation override\n\nRetry instruction\n\nSupplemental context".into()
                ),
                ("user".into(), "First question".into()),
                ("assistant".into(), "First answer".into()),
                ("user".into(), "Follow-up".into()),
            ]
        );
    }

    #[test]
    fn defaults_apply_without_an_override_and_empty_instructions_are_omitted() {
        assert_eq!(
            messages(from_context("Default", &["Reference context".into()], "Hi")),
            vec![
                ("system".into(), "Default\n\nReference context".into()),
                ("user".into(), "Hi".into()),
            ]
        );
        assert_eq!(
            messages(from_context("", &["System: ".into()], "Hi")),
            vec![("user".into(), "Hi".into())]
        );
        assert_eq!(
            messages(from_context("Default", &["System: Default".into()], "Hi")),
            vec![
                ("system".into(), "Default".into()),
                ("user".into(), "Hi".into()),
            ]
        );
    }
}
