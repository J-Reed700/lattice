//! Test helpers for inference tests
use lattice::infrastructure::llm::traits::GenerationConfig;

pub struct GenerationConfigFixture;

impl GenerationConfigFixture {
    pub fn default() -> GenerationConfig {
        GenerationConfig::default()
    }

    pub fn small() -> GenerationConfig {
        GenerationConfig {
            max_tokens: 512,
            ..Default::default()
        }
    }

    pub fn large() -> GenerationConfig {
        GenerationConfig {
            max_tokens: 4096,
            ..Default::default()
        }
    }

    pub fn custom(max_tokens: usize) -> GenerationConfig {
        GenerationConfig {
            max_tokens,
            ..Default::default()
        }
    }
}
