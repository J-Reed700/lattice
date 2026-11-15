//! The reason a lesson draft gives its right choice.
use serde_json::Value;

/// The draft with every question's right choice given no reason: its
/// explanation says why it is right. A draft that gave none comes back as
/// the writer sent it.
pub(crate) fn clear_right_choice_reasons(raw: String) -> String {
    let Ok(mut draft) = serde_json::from_str::<Value>(&raw) else {
        return raw;
    };
    let mut cleared = 0usize;
    if let Some(questions) = draft.get_mut("questions").and_then(Value::as_array_mut) {
        for question in questions {
            let Some(right) = question
                .get("correctIndex")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
            else {
                continue;
            };
            let Some(slot) = question
                .get_mut("rationales")
                .and_then(Value::as_array_mut)
                .and_then(|rationales| rationales.get_mut(right))
            else {
                continue;
            };
            if slot.as_str().is_some_and(|reason| !reason.is_empty()) {
                *slot = Value::String(String::new());
                cleared += 1;
            }
        }
    }
    if cleared == 0 {
        return raw;
    }
    tracing::info!(
        questions = cleared,
        "cleared the reason a lesson draft gave its right choice"
    );
    serde_json::to_string(&draft).unwrap_or(raw)
}
