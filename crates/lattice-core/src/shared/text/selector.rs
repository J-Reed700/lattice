//! A byte-addressed text quote inside one field, shared with exact source citations.

use serde::{Deserialize, Serialize};

/// A W3C-shaped quote selector refined by UTF-8 byte offsets into a field's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TextSelector {
    pub exact: String,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub start_byte: u32,
    pub end_byte: u32,
}

impl TextSelector {
    /// Builds a selector for `text[start_byte..end_byte]`, returning `None` off a character boundary or out of bounds.
    pub fn of(_text: &str, _start_byte: usize, _end_byte: usize) -> Option<Self> {
        None
    }

    /// Returns true when the bytes at this selector's offsets in `text` match `exact`.
    pub fn holds(&self, _text: &str) -> bool {
        false
    }
}
