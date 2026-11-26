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

#[cfg(test)]
mod tests {
    use super::TextSelector;

    #[test]
    fn a_selector_holds_only_where_its_bytes_are_its_quote() {
        let text = "Guide: Quillet — edition №1 works.";
        let target = "Quillet";
        let start = text.find(target).unwrap();
        let end = start + target.len();

        let selector = TextSelector::of(text, start, end);
        assert!(
            selector.is_some(),
            "expected selector for valid char bounds"
        );
        let selector = selector.unwrap();
        assert_eq!(selector.exact, target);
        assert_eq!(selector.start_byte, u32::try_from(start).unwrap());
        assert_eq!(selector.end_byte, u32::try_from(end).unwrap());
        assert!(selector.holds(text));

        let shifted = format!("X{text}");
        assert!(!selector.holds(&shifted));

        let multi = "Café crème";
        assert!(TextSelector::of(multi, 3, 4).is_none());
        assert!(TextSelector::of(multi, 4, multi.len() + 1).is_none());
        assert!(TextSelector::of(multi, 5, 4).is_none());
    }
}
