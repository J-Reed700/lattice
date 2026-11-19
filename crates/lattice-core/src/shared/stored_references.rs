//! The places of the references a message was saved with, and its text without them.
//!
//! A reply is saved as its text, handles written in, and the byte range of each
//! reference under `metadata.references`. This is the one reader of those
//! ranges: whatever shows a stored message to a model outside the prompt that
//! writes an answer in its own conversation reads the text through [`cut`].

use std::borrow::Cow;
use std::ops::Range;

use serde_json::Value;

/// One saved reference: where it sits in the message text, and the entry it was saved as.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPlace<'a> {
    pub range: Range<usize>,
    pub entry: &'a Value,
}

/// The saved references of a message, in order; `None` for a message saved
/// before references. An entry whose place does not fit the text, or overlaps
/// the one before it, is left out.
pub fn stored_places<'a>(content: &str, metadata: &'a Value) -> Option<Vec<StoredPlace<'a>>> {
    let list = metadata.get("references")?.as_array()?;
    let mut places: Vec<StoredPlace<'a>> = Vec::new();
    for entry in list {
        let Some(range) = place_of(content, entry) else {
            continue;
        };
        if places
            .last()
            .is_some_and(|last| range.start < last.range.end)
        {
            continue;
        }
        places.push(StoredPlace { range, entry });
    }
    Some(places)
}

/// The byte ranges of a message's saved references, from its `metadata` column;
/// empty for a message saved before references.
pub fn reference_ranges(content: &str, metadata: Option<&str>) -> Vec<Range<usize>> {
    let Some(metadata) = metadata.and_then(|metadata| serde_json::from_str::<Value>(metadata).ok())
    else {
        return Vec::new();
    };
    stored_places(content, &metadata)
        .unwrap_or_default()
        .into_iter()
        .map(|place| place.range)
        .collect()
}

/// A stored message's text without its references: the content with the
/// ranges saved in its `metadata` taken out, and the content as it is for a
/// message saved before them (its numbered markers are another reader's).
pub fn text_without_references<'a>(content: &'a str, metadata: Option<&str>) -> Cow<'a, str> {
    let ranges = reference_ranges(content, metadata);
    cut(content, 0..content.len(), &ranges)
}

/// `metadata` without its `references`, for a message whose text has been cut
/// by them and is read as that text from here on.
pub fn metadata_without_references(metadata: Option<&str>) -> Option<String> {
    let mut parsed = serde_json::from_str::<Value>(metadata?).ok()?;
    parsed.as_object_mut()?.remove("references");
    serde_json::to_string(&parsed).ok()
}

/// `window` of `text` with each of `ranges` that lies inside it, and the spaces
/// and tabs before it, taken out. Borrowed when nothing is taken out.
pub fn cut<'a>(text: &'a str, window: Range<usize>, ranges: &[Range<usize>]) -> Cow<'a, str> {
    let mut inside = ranges
        .iter()
        .filter(|range| range.start >= window.start && range.end <= window.end)
        .peekable();
    if inside.peek().is_none() {
        return Cow::Borrowed(text.get(window).unwrap_or_default());
    }
    let mut out = String::new();
    let mut at = window.start;
    for range in inside {
        let before = text.get(at..range.start).unwrap_or_default();
        out.push_str(before.trim_end_matches([' ', '\t']));
        at = range.end;
    }
    out.push_str(text.get(at..window.end).unwrap_or_default());
    Cow::Owned(out)
}

fn place_of(content: &str, entry: &Value) -> Option<Range<usize>> {
    let start = usize::try_from(entry.get("start")?.as_u64()?).ok()?;
    let end = usize::try_from(entry.get("end")?.as_u64()?).ok()?;
    (start < end && content.is_char_boundary(start) && content.is_char_boundary(end))
        .then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_message_saved_before_references_has_no_places() {
        assert_eq!(
            reference_ranges("Plain [1].", None),
            Vec::<Range<usize>>::new()
        );
        assert_eq!(
            reference_ranges("Plain [1].", Some("{}")),
            Vec::<Range<usize>>::new()
        );
        assert!(stored_places("Plain", &json!({})).is_none());
    }

    #[test]
    fn saved_places_that_do_not_fit_or_overlap_are_left_out() {
        let metadata = json!({"references": [
            {"start": 4, "end": 8},
            {"start": 6, "end": 9},
            {"start": 50, "end": 60},
            {"start": 9, "end": 9},
            {"start": 10, "end": 14},
        ]});
        let content = "Ab. [s1] [s2] end";
        let places = stored_places(content, &metadata).unwrap_or_default();
        let ranges: Vec<_> = places.into_iter().map(|place| place.range).collect();
        assert_eq!(ranges, vec![4..8, 10..14]);
    }

    #[test]
    fn a_cut_takes_out_each_reference_and_the_spaces_before_it() {
        let text = "Fact [s1] and more [s2].";
        let ranges = [5..9, 19..23];
        assert_eq!(cut(text, 0..text.len(), &ranges), "Fact and more.");
        assert_eq!(cut(text, 0..9, &ranges), "Fact");
        assert!(matches!(cut(text, 0..4, &ranges), Cow::Borrowed("Fact")));
    }
}
