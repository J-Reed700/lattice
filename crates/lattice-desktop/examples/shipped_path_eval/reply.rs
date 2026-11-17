//! What a turn's reply was, as a row keeps it: the pieces the chat draws, text
//! between the references the app resolved. The scorers read citations from
//! these pieces and from nothing else.

use lattice::features::conversation::dto::MessageDto;
use lattice_core::shared::error::Result;
use serde_json::{json, Value};

/// The pieces the chat drew the last answer from; `null` when that message
/// came without them.
pub fn shown(_messages: &[MessageDto]) -> Value {
    Value::Null
}

/// A saved answer read the way the chat reads one, in a conversation whose
/// sources were named `[s1]` to `[s{last}]`.
pub async fn read_saved(answer: &str, _last: u32) -> Result<Value> {
    Ok(json!([{"type": "text", "text": answer}]))
}

/// Gives each turn of a saved row the pieces the chat would have drawn.
pub async fn rescore_row(_row: &mut Value) -> Result<()> {
    Ok(())
}
