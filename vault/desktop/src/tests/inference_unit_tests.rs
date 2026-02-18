#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Unit tests for GenerationConfig.
//!
//! Tests verify GenerationConfig behavior and default values.
//!
//! Note: RequestBuilder tests are not included because mistral.rs RequestBuilder
//! does not expose methods to inspect internal state (max_tokens, temperature, etc.).
//! The builder pattern is opaque, making verification infeasible without actual
//! inference execution.
use vault::infrastructure::llm::traits::GenerationConfig;

mod inference_helpers;
use inference_helpers::GenerationConfigFixture;

#[cfg(test)]
mod generation_config_tests {
    use super::*;

    #[test]
    fn test_generation_config_defaults() {
        let config = GenerationConfig::default();

        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.top_p, 0.9);
        assert_eq!(config.top_k, 40);
        assert_eq!(config.max_tokens, 2048);
        assert_eq!(config.repeat_penalty, 1.1);
    }

    #[test]
    fn test_generation_config_custom_max_tokens() {
        let config = GenerationConfig {
            max_tokens: 1024,
            ..Default::default()
        };

        assert_eq!(config.max_tokens, 1024);
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.top_p, 0.9);
    }

    #[test]
    fn test_generation_config_zero_max_tokens() {
        let config = GenerationConfig {
            max_tokens: 0,
            ..Default::default()
        };

        assert_eq!(config.max_tokens, 0);
    }

    #[test]
    fn test_generation_config_all_fields() {
        let config = GenerationConfig {
            temperature: 0.8,
            top_p: 0.95,
            top_k: 50,
            max_tokens: 512,
            repeat_penalty: 1.2,
        };

        assert_eq!(config.temperature, 0.8);
        assert_eq!(config.top_p, 0.95);
        assert_eq!(config.top_k, 50);
        assert_eq!(config.max_tokens, 512);
        assert_eq!(config.repeat_penalty, 1.2);
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;

    #[test]
    fn test_fixture_default() {
        let config = GenerationConfigFixture::default();
        let default = GenerationConfig::default();

        assert_eq!(config.temperature, default.temperature);
        assert_eq!(config.top_p, default.top_p);
        assert_eq!(config.top_k, default.top_k);
        assert_eq!(config.max_tokens, default.max_tokens);
        assert_eq!(config.repeat_penalty, default.repeat_penalty);
    }

    #[test]
    fn test_fixture_small() {
        let config = GenerationConfigFixture::small();
        assert_eq!(config.max_tokens, 512);
    }

    #[test]
    fn test_fixture_large() {
        let config = GenerationConfigFixture::large();
        assert_eq!(config.max_tokens, 4096);
    }

    #[test]
    fn test_fixture_custom() {
        let config = GenerationConfigFixture::custom(1024);
        assert_eq!(config.max_tokens, 1024);
    }
}
