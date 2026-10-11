//! A token estimate that learns the backend's tokenizer from its own reports.
//!
//! A fixed four characters a token reads low on code, numbers and most
//! non-English text, so budgets built on it overran the window exactly where
//! the content was densest. Every backend reports how many prompt tokens a
//! request really took; this keeps a moving average of characters per token
//! against those reports.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN;

/// No tokenizer averages outside this range on real text; a measurement that
/// does is a mis-attributed prompt (images, opaque provider state), not a
/// tokenizer.
const MIN_CHARS_PER_TOKEN: f64 = 2.0;
const MAX_CHARS_PER_TOKEN: f64 = 6.0;

/// Weight of one new measurement in the moving average.
const SMOOTHING: f64 = 0.2;

/// Prompts below this many reported tokens are mostly chat-template framing,
/// which would drag the ratio down for every long prompt after them.
const MIN_OBSERVED_TOKENS: u64 = 64;

/// Characters per token for one backend, calibrated as usage comes back.
pub(crate) struct TokenEstimator {
    /// `f64` bits; an atomic so counting never waits on a lock.
    chars_per_token: AtomicU64,
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self {
            chars_per_token: AtomicU64::new(DEFAULT_CHARS_PER_TOKEN.to_bits()),
        }
    }
}

impl TokenEstimator {
    pub fn chars_per_token(&self) -> f64 {
        f64::from_bits(self.chars_per_token.load(Ordering::Relaxed))
    }

    /// Fold in one request: `chars` characters that the backend reported as
    /// `tokens` prompt tokens.
    pub fn observe(&self, chars: usize, tokens: u64) {
        if tokens < MIN_OBSERVED_TOKENS || chars == 0 {
            return;
        }
        let measured =
            (chars as f64 / tokens as f64).clamp(MIN_CHARS_PER_TOKEN, MAX_CHARS_PER_TOKEN);
        // Concurrent observations may overwrite each other; either result is
        // a valid average, and counting must not wait on a lock.
        let current = self.chars_per_token();
        let next = (current + SMOOTHING * (measured - current))
            .clamp(MIN_CHARS_PER_TOKEN, MAX_CHARS_PER_TOKEN);
        self.chars_per_token
            .store(next.to_bits(), Ordering::Relaxed);
    }
}
