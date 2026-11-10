//! A request's size against its context window, and the refusal of a prompt that fills it.

use crate::shared::error::AppError;

/// How a prompt measured against the window, and the room it left for the
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFit {
    /// The prompt's tokens, pictures included.
    pub prompt_tokens: usize,
    /// Whether the backend's tokenizer counted the prompt's text.
    pub counted_exactly: bool,
    /// Tokens the window has left after the prompt.
    pub room: u32,
}

/// What one request asks of the context window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestSize {
    /// The prompt's tokens, pictures included.
    pub prompt_tokens: usize,
    /// Whether the backend's tokenizer counted the prompt's text. Otherwise
    /// the text is the estimate. Pictures are never counted by a tokenizer.
    pub counted_exactly: bool,
    /// Tokens reserved for the answer.
    pub output_tokens: u32,
    /// What the prompt's pictures cost by the model's declared geometry,
    /// already inside `prompt_tokens`.
    pub image_tokens: usize,
    /// How many pictures the prompt carries.
    pub image_count: usize,
}

impl RequestSize {
    /// Prompt and answer together: what the request holds in the window.
    pub fn total(&self) -> usize {
        self.prompt_tokens
            .saturating_add(usize::try_from(self.output_tokens).unwrap_or(usize::MAX))
    }

    /// How the prompt was measured, naming the pictures when it has any.
    fn counted(&self) -> String {
        let text = if self.counted_exactly {
            "counted exactly"
        } else {
            "estimated"
        };
        if self.image_count == 0 {
            return text.into();
        }
        let pictures = if self.image_count == 1 {
            "1 picture".to_string()
        } else {
            format!("{} pictures", self.image_count)
        };
        format!(
            "text {text}; {} tokens for {pictures} estimated from the model's declared geometry",
            self.image_tokens
        )
    }

    pub fn describe(&self) -> String {
        format!(
            "{} tokens: a prompt of {} ({}) and {} for the answer",
            self.total(),
            self.prompt_tokens,
            self.counted(),
            self.output_tokens
        )
    }
}

/// The refusal of a request whose prompt alone fills the configured `window`,
/// before it is sent. Nothing else is refused for its size: an answer always
/// shrinks to the room its prompt leaves, so only a prompt that leaves none
/// cannot be sent. It carries the numbers, so a caller that can send less
/// (fewer passages, a smaller group) knows by how much and asks again.
pub fn window_refusal(model: &str, size: RequestSize, window: usize) -> AppError {
    tracing::warn!(
        model,
        prompt_tokens = size.prompt_tokens,
        counted_exactly = size.counted_exactly,
        window,
        "llm.request_refused"
    );
    AppError::PromptExceedsWindow {
        prompt_tokens: size.prompt_tokens,
        window,
        counted_exactly: size.counted_exactly,
        message: format!(
            "This request's prompt is {} tokens ({}), which leaves no room for an answer; the \
             configured context window is {window}. It was not sent. Shorten the request, or \
             raise the context window in Settings if the model's server holds more.",
            size.prompt_tokens,
            size.counted()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_that_fills_the_window_is_refused_with_its_numbers() {
        let size = RequestSize {
            prompt_tokens: 140_000,
            counted_exactly: false,
            ..RequestSize::default()
        };
        let refused = window_refusal("m", size, 131_072);
        assert!(matches!(
            refused,
            AppError::PromptExceedsWindow {
                prompt_tokens: 140_000,
                window: 131_072,
                counted_exactly: false,
                ..
            }
        ));
        let told = refused.to_string();
        for expected in [
            "prompt is 140000 tokens (estimated), which leaves no room for an answer",
            "configured context window is 131072",
            "not sent",
        ] {
            assert!(told.contains(expected), "{told}");
        }
    }

    #[test]
    fn a_refusal_names_the_pictures_and_how_they_were_charged() {
        let size = RequestSize {
            prompt_tokens: 8_300,
            counted_exactly: true,
            output_tokens: 0,
            image_tokens: 2_052,
            image_count: 2,
        };
        let told = window_refusal("m", size, 8_192).to_string();
        assert!(
            told.contains(
                "prompt is 8300 tokens (text counted exactly; 2052 tokens for 2 pictures estimated \
                 from the model's declared geometry)"
            ),
            "{told}"
        );
    }
}
