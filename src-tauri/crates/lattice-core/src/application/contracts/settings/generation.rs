//! How the model samples, thinks and decodes, as the user tunes it.

use serde::{Deserialize, Serialize};

/// The sampling a model runs with in one mode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SamplingProfileDto {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    /// Flat penalty on any token already in the recent window. Zero is off.
    pub presence_penalty: f32,
    /// Multiplicative penalty on repeated tokens. One is off.
    pub repeat_penalty: f32,
}

impl SamplingProfileDto {
    /// The Qwen 3.5 model card's thinking-mode profile.
    pub fn thinking() -> Self {
        Self {
            temperature: 1.0,
            top_p: 0.95,
            top_k: 20,
            presence_penalty: 1.5,
            repeat_penalty: 1.0,
        }
    }

    /// Problems with this profile, each a sentence for the settings check.
    pub fn problems(&self, name: &str) -> Vec<String> {
        let mut problems = Vec::new();
        if !(0.0..=2.0).contains(&self.temperature) {
            problems.push(format!("{name} temperature must be between 0.0 and 2.0"));
        }
        if !(0.0..=1.0).contains(&self.top_p) {
            problems.push(format!("{name} top_p must be between 0.0 and 1.0"));
        }
        if self.top_k < 0 {
            problems.push(format!("{name} top_k must be greater than or equal to 0"));
        }
        problems.extend(presence_penalty_problem(name, self.presence_penalty));
        if self.repeat_penalty <= 0.0 {
            problems.push(format!("{name} repeat_penalty must be greater than 0.0"));
        }
        problems
    }
}

impl Default for SamplingProfileDto {
    fn default() -> Self {
        Self::thinking()
    }
}

/// The OpenAI range llama-server and Ollama accept for a presence penalty.
pub fn presence_penalty_problem(name: &str, value: f32) -> Option<String> {
    (!(-2.0..=2.0).contains(&value))
        .then(|| format!("{name} presence_penalty must be between -2.0 and 2.0"))
}

/// Whether the model thinks, overriding the rule for its model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ThinkingOverrideDto {
    /// Answers think when the user asked and no tool can be called; a model
    /// that breaks with thinking off (Laguna) always thinks.
    #[default]
    Auto,
    /// Every request thinks, tool rounds included.
    On,
    /// No request thinks, whatever the model.
    Off,
}

/// Tokens a multi-token-prediction head drafts per step when the user turns
/// it on: llama-server's own default, and the fastest measured on Qwen 3.5 9B
/// (1.55x decode, 1.83x while thinking; N=2, 4 and 6 were slower, track G).
pub const DEFAULT_MTP_DRAFT_N_MAX: u32 = 3;

/// More drafted tokens than any published MTP head predicts well; past this a
/// draft is mostly rejected and costs more than it saves.
pub const MAX_MTP_DRAFT_N_MAX: u32 = 8;

/// Speculative decoding for the bundled llama-server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SpeculativeDecodingDto {
    #[default]
    Off,
    /// Draft with the model's own multi-token-prediction head. Only GGUFs
    /// that carry one can use it; the server then runs a single slot.
    MtpDraft {
        #[serde(rename = "nMax")]
        n_max: u32,
    },
}

impl SpeculativeDecodingDto {
    /// Problems with this choice, each a sentence for the settings check.
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::MtpDraft { n_max } if !(1..=MAX_MTP_DRAFT_N_MAX).contains(n_max) => Some(
                format!("speculative_decoding n_max must be between 1 and {MAX_MTP_DRAFT_N_MAX}"),
            ),
            _ => None,
        }
    }
}

/// Per-model launch configuration for a bundled local GGUF model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelConfigDto {
    /// Context length in tokens. `None` means Auto (dynamically sized to the
    /// model's trained maximum and the machine's available memory).
    #[serde(default)]
    pub context_length: Option<u32>,
    /// Max concurrent predictions (`-np` parallel slots on `llama-server`).
    /// `None` uses the server default (4).
    #[serde(default)]
    pub max_concurrent_predictions: Option<u32>,
    /// Fixed RNG seed (`-s` on `llama-server`). `None` uses a random seed.
    #[serde(default)]
    pub seed: Option<u32>,
    /// Quantize the KV cache (`-ctk q8_0 -ctv q8_0`) to roughly halve KV cache
    /// memory usage and allow a larger context window to fit in memory.
    #[serde(default)]
    pub kv_cache_quantization: bool,
}
