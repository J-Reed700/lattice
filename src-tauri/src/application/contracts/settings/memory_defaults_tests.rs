//! Tests for the bounded conversation memory default.

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
