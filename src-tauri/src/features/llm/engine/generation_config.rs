//! Sampling defaults and output ceiling a model is loaded with.

use crate::application::contracts::settings::{
    LLMSettingsDto, SamplingProfileDto, ThinkingOverrideDto,
};
use crate::application::ports::llm_port::{ThinkingMode, ThinkingPolicy};
use serde::{Deserialize, Serialize};

/// Configuration for text generation.
///
/// The flat sampling fields are the answer profile, used when the model
/// answers directly; `thinking` is the profile for requests that think first.
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

    /// Flat penalty on any token already present in the recent window.
    #[serde(default = "default_presence_penalty")]
    pub presence_penalty: f32,

    /// Sampling for requests that think before answering.
    #[serde(default = "SamplingProfile::thinking")]
    pub thinking: SamplingProfile,

    /// The user's pin on thinking; `PerRequest` leaves it to the request
    /// and the model.
    #[serde(default)]
    pub thinking_override: ThinkingPolicy,
}

/// The sampling a model runs with in one mode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingProfile {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub presence_penalty: f32,
    pub repeat_penalty: f32,
}

impl SamplingProfile {
    /// The Qwen 3.5 model card's thinking-mode profile. A long thinking trace
    /// loops without the presence penalty, and the higher temperature is what
    /// the card measured its reasoning scores at.
    pub fn thinking() -> Self {
        Self {
            temperature: 1.0,
            top_p: 0.95,
            top_k: 20,
            presence_penalty: 1.5,
            repeat_penalty: 1.0,
        }
    }
}

impl From<SamplingProfileDto> for SamplingProfile {
    fn from(profile: SamplingProfileDto) -> Self {
        Self {
            temperature: profile.temperature,
            top_p: profile.top_p,
            top_k: profile.top_k,
            presence_penalty: profile.presence_penalty,
            repeat_penalty: profile.repeat_penalty,
        }
    }
}

impl GenerationConfig {
    /// The user's sampling for both modes and their output ceiling.
    pub fn from_settings(settings: &LLMSettingsDto) -> Self {
        Self {
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            max_tokens: settings.max_tokens as usize,
            repeat_penalty: settings.repeat_penalty,
            presence_penalty: settings.presence_penalty,
            thinking: settings.thinking_sampling.into(),
            thinking_override: match settings.thinking_override {
                ThinkingOverrideDto::Auto => ThinkingPolicy::PerRequest,
                ThinkingOverrideDto::On => ThinkingPolicy::AlwaysOn,
                ThinkingOverrideDto::Off => ThinkingPolicy::AlwaysOff,
            },
        }
    }

    /// The profile a request in `mode` is sampled with, before any override
    /// the request carries.
    pub fn profile(&self, mode: ThinkingMode) -> SamplingProfile {
        match mode {
            ThinkingMode::Off => SamplingProfile {
                temperature: self.temperature,
                top_p: self.top_p,
                top_k: self.top_k,
                presence_penalty: self.presence_penalty,
                repeat_penalty: self.repeat_penalty,
            },
            ThinkingMode::On => self.thinking,
        }
    }
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            top_p: default_top_p(),
            top_k: default_top_k(),
            max_tokens: default_max_tokens(),
            repeat_penalty: default_repeat_penalty(),
            presence_penalty: default_presence_penalty(),
            thinking: SamplingProfile::thinking(),
            thinking_override: ThinkingPolicy::PerRequest,
        }
    }
}

// The answer defaults are the Qwen 3.5 card's non-thinking profile.
fn default_temperature() -> f32 {
    0.7
}
fn default_top_p() -> f32 {
    0.8
}
fn default_top_k() -> i32 {
    20
}
fn default_max_tokens() -> usize {
    131072
}
fn default_repeat_penalty() -> f32 {
    1.0
}
fn default_presence_penalty() -> f32 {
    1.5
}
