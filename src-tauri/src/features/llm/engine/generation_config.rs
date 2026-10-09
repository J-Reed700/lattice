//! Sampling defaults and output ceiling a model is loaded with.

use serde::{Deserialize, Serialize};

/// Configuration for text generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Temperature for sampling (0.0 = deterministic, 2.0 = very random).
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p (nucleus) sampling.
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Top-k sampling.
    #[serde(default = "default_top_k")]
    pub top_k: i32,

    /// Maximum number of tokens to generate.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Penalty for repeating tokens.
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            top_p: default_top_p(),
            top_k: default_top_k(),
            max_tokens: default_max_tokens(),
            repeat_penalty: default_repeat_penalty(),
        }
    }
}

fn default_temperature() -> f32 {
    0.7
}
fn default_top_p() -> f32 {
    0.9
}
fn default_top_k() -> i32 {
    40
}
fn default_max_tokens() -> usize {
    131072
}
fn default_repeat_penalty() -> f32 {
    1.1
}
