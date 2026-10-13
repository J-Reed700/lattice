//! Tests for LLM settings that fill in when a saved file lacks them: bounded memory and generation.

#![cfg(test)]

use super::*;
#[test]
fn memory_defaults_on_without_overriding_an_explicit_opt_out() {
    assert!(LLMSettingsDto::default().bounded_conversation_memory);
    let mut value = serde_json::to_value(LLMSettingsDto::default()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("boundedConversationMemory");
    let missing: LLMSettingsDto = serde_json::from_value(value.clone()).unwrap();
    assert!(missing.bounded_conversation_memory);
    value["boundedConversationMemory"] = serde_json::json!(false);
    let disabled: LLMSettingsDto = serde_json::from_value(value).unwrap();
    assert!(!disabled.bounded_conversation_memory);
}

#[test]
fn generation_settings_fill_in_when_missing_and_keep_their_wire_shape() {
    let mut value = serde_json::to_value(LLMSettingsDto::default()).unwrap();
    assert_eq!(
        value["speculativeDecoding"],
        serde_json::json!({"mode": "off"})
    );
    assert_eq!(value["thinkingOverride"], "auto");
    let object = value.as_object_mut().unwrap();
    for key in [
        "presencePenalty",
        "answerThinking",
        "thinkingOverride",
        "thinkingSampling",
        "speculativeDecoding",
    ] {
        object.remove(key);
    }
    let missing: LLMSettingsDto = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(missing.presence_penalty, 1.5);
    assert!(!missing.answer_thinking);
    assert_eq!(missing.thinking_override, ThinkingOverrideDto::Auto);
    assert_eq!(missing.thinking_sampling, SamplingProfileDto::thinking());

    value["speculativeDecoding"] = serde_json::json!({"mode": "mtpDraft", "nMax": 4});
    let mtp: LLMSettingsDto = serde_json::from_value(value).unwrap();
    assert_eq!(
        mtp.speculative_decoding,
        SpeculativeDecodingDto::MtpDraft { n_max: 4 }
    );
    assert!(mtp.speculative_decoding.problem().is_none());
    assert!(SpeculativeDecodingDto::MtpDraft { n_max: 0 }
        .problem()
        .is_some());
    assert!(SamplingProfileDto::thinking()
        .problems("thinking")
        .is_empty());
    assert!(presence_penalty_problem("llm", 2.5).is_some());
}
