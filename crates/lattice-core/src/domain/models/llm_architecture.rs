//! Which GGUF `general.architecture` values the pinned bundled `llama-server` loads as a standalone language model.

/// `llama.cpp`'s `LLM_ARCH_NAMES` at the pinned engine tag: a comment line,
/// the tag, then one architecture per line.
pub const PINNED_ENGINE_ARCHITECTURES: &str =
    include_str!("../../../../lattice-desktop/scripts/llama-architectures.txt");

/// Speculative-decoding sidecars and audio vocoders live in `LLM_ARCH_NAMES`
/// because `llama.cpp` parses their headers, but `llama-server -m` cannot load
/// them as a standalone chat or utility language model.
const NON_STANDALONE_LLM_ARCHITECTURES: &[&str] = &[
    "dflash",
    "dspark",
    "eagle3",
    "pockettts",
    "qwen3tts",
    "talkie",
    "wavtokenizer-dec",
];

/// How the pinned engine treats one exact architecture name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmArchitectureSupport {
    /// Loads as a standalone chat or utility model.
    Standalone,
    /// Parsed by the engine, but only usable as a sidecar or codec.
    SidecarOnly,
    /// Not in the pinned engine's architecture list.
    Unsupported,
}

/// Classifies an exact `general.architecture` value.
pub fn classify_llm_architecture(architecture: &str) -> LlmArchitectureSupport {
    if NON_STANDALONE_LLM_ARCHITECTURES.contains(&architecture) {
        return LlmArchitectureSupport::SidecarOnly;
    }
    if PINNED_ENGINE_ARCHITECTURES
        .lines()
        .skip(2)
        .any(|entry| entry == architecture)
    {
        return LlmArchitectureSupport::Standalone;
    }
    LlmArchitectureSupport::Unsupported
}

/// True when `architecture`, ignoring case and surrounding whitespace, loads
/// as a standalone model on the pinned engine.
pub fn is_supported_llm_architecture(architecture: &str) -> bool {
    let lower = architecture.trim().to_ascii_lowercase();
    classify_llm_architecture(&lower) == LlmArchitectureSupport::Standalone
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_standalone_sidecar_and_unknown_architectures() {
        assert_eq!(
            classify_llm_architecture("llama"),
            LlmArchitectureSupport::Standalone
        );
        assert_eq!(
            classify_llm_architecture("eagle3"),
            LlmArchitectureSupport::SidecarOnly
        );
        assert_eq!(
            classify_llm_architecture("future-unknown-family"),
            LlmArchitectureSupport::Unsupported
        );
        assert!(is_supported_llm_architecture(" Llama "));
        assert!(!is_supported_llm_architecture("DFlash"));
    }
}
