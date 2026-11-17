//! What a turn's reply was, as a row keeps it: the pieces the chat draws, text
//! between the references the app resolved. The scorers read citations from
//! these pieces and from nothing else.

use std::sync::Arc;

use lattice::features::conversation::chat::reply_text::{markers, ReplyText};
use lattice::features::conversation::dto::MessageDto;
use lattice_core::application::ports::conversation_names::ConversationNames;
use lattice_core::application::services::model_contract::kept::{KeptNames, MemoryNames};
use lattice_core::application::services::model_contract::shown::{Kind, Record};
use lattice_core::application::services::tool_handles::Source;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

/// The conversation a saved answer is read in; each answer gets its own store.
const SAVED: &str = "saved";

/// The pieces the chat drew the last answer from; `null` when that message
/// came without them.
pub fn shown(messages: &[MessageDto]) -> Value {
    messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant")
        .and_then(|message| message.reply.as_ref())
        .and_then(|pieces| serde_json::to_value(pieces).ok())
        .unwrap_or(Value::Null)
}

/// A saved answer read the way the chat reads one, in a conversation whose
/// sources were named `[s1]` to `[s{last}]`.
pub async fn read_saved(answer: &str, last: u32) -> Result<Value> {
    let store = Arc::new(MemoryNames::new());
    let names: Vec<Record> = (1..=last)
        .map(|ordinal| Record {
            prefix: Source::PREFIX.to_owned(),
            ordinal,
            under: None,
            reference: json!([SAVED, ordinal.to_string()]),
        })
        .collect();
    store.keep(SAVED, 0, &names).await?;
    let names = KeptNames::new(store).open(SAVED).await?;
    let pieces = ReplyText::read(&names, markers(None), answer).pieces();
    serde_json::to_value(pieces).map_err(|error| AppError::InternalError(error.to_string()))
}

/// Gives each turn of a saved row the pieces the chat would have drawn. By a
/// turn, its conversation had named sources up to the highest citation number
/// in its own and earlier turns' evidence, so the names are rebuilt that far.
pub async fn rescore_row(row: &mut Value) -> Result<()> {
    let mut last = 0;
    for turn in row
        .get_mut("turns")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        last = turn
            .get("evidence")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| u32::try_from(entry.get("citation_id")?.as_u64()?).ok())
            .fold(last, u32::max);
        let pieces = match turn.get("answer").and_then(Value::as_str) {
            Some(answer) => read_saved(answer, last).await?,
            None => json!([]),
        };
        if let Some(turn) = turn.as_object_mut() {
            turn.insert("reply".to_owned(), pieces);
        }
    }
    Ok(())
}
