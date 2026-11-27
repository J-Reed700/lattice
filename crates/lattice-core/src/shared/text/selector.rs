//! A byte-addressed text quote inside one field, shared with exact source citations.

use serde::{Deserialize, Serialize};

const CONTEXT_CHARS: usize = 64;

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

fn context_before(text: &str, byte: usize) -> Option<String> {
    let before: String = text
        .get(..byte)?
        .chars()
        .rev()
        .take(CONTEXT_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (!before.is_empty()).then_some(before)
}

fn context_after(text: &str, byte: usize) -> Option<String> {
    let after: String = text.get(byte..)?.chars().take(CONTEXT_CHARS).collect();
    (!after.is_empty()).then_some(after)
}

impl TextSelector {
    /// Builds a selector for `text[start_byte..end_byte]`, returning `None` off a character boundary, empty, or out of bounds.
    pub fn of(text: &str, start_byte: usize, end_byte: usize) -> Option<Self> {
        if start_byte >= end_byte
            || end_byte > text.len()
            || !text.is_char_boundary(start_byte)
            || !text.is_char_boundary(end_byte)
        {
            return None;
        }
        let exact = text.get(start_byte..end_byte)?.to_owned();
        let start = u32::try_from(start_byte).ok()?;
        let end = u32::try_from(end_byte).ok()?;
        Some(Self {
            exact,
            prefix: context_before(text, start_byte),
            suffix: context_after(text, end_byte),
            start_byte: start,
            end_byte: end,
        })
    }

    /// Returns true when the bytes at this selector's offsets in `text` match `exact`.
    pub fn holds(&self, text: &str) -> bool {
        let Ok(start) = usize::try_from(self.start_byte) else {
            return false;
        };
        let Ok(end) = usize::try_from(self.end_byte) else {
            return false;
        };
        start < end && text.get(start..end) == Some(self.exact.as_str())
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
