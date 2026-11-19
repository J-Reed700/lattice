use crate::lessons::review_findings::{index_segment, leaves};
use lattice_core::shared::text::same_text;
use serde_json::{json, Value};

/// The fields that teach, of a block and of a question: where a cited
/// sentence may stand and where a written sentence is held to the lesson's
/// own words.
const TAUGHT: (&[&str], &[&str]) = (&["body"], &["explanation", "rationales"]);

/// The texts of a block or question a cited sentence may stand in: a block's
/// body, a question's explanation or any of its choice reasons.
pub(in crate::lessons) fn citable_texts(part: &Value, question: bool) -> Vec<&str> {
    citable_leaves(part, question)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

/// [`citable_texts`] with the path of each text relative to `part` (`body`, `rationales/0`).
fn citable_leaves(part: &Value, question: bool) -> Vec<(String, &str)> {
    let fields = if question { TAUGHT.1 } else { TAUGHT.0 };
    fields
        .iter()
        .filter_map(|field| part.get(*field).map(|value| (*field, value)))
        .flat_map(|(field, value)| {
            leaves(value).into_iter().map(move |(rest, text)| {
                let path = if rest.is_empty() {
                    field.to_owned()
                } else {
                    format!("{field}/{rest}")
                };
                (path, text)
            })
        })
        .collect()
}

/// Where `sentence` stands in the citable texts of `part`: the path of the text relative to `part`, the
/// text, and the bytes of its whole sentence(s) that `sentence` stands for. None for a blank sentence.
pub(in crate::lessons) fn cited_sentence<'a>(
    part: &'a Value,
    question: bool,
    sentence: &str,
) -> Option<(String, &'a str, (usize, usize))> {
    if sentence.trim().is_empty() {
        return None;
    }
    citable_leaves(part, question)
        .into_iter()
        .find_map(|(path, held)| {
            let span = same_text::find(held, sentence)?;
            let lying = same_text::sentences_over(held, span);
            let start = lying.first()?.0;
            let end = lying.last()?.1;
            Some((path, held, (start, end)))
        })
}

/// Whether `path` names a text that teaches in a block or question.
pub(in crate::lessons) fn taught_at(path: &str) -> bool {
    let mut segments = path.split('/').skip(1);
    let (Some(list), Some(index), Some(field)) =
        (segments.next(), segments.next(), segments.next())
    else {
        return false;
    };
    let fields = match list {
        "blocks" => TAUGHT.0,
        "questions" => TAUGHT.1,
        _ => return false,
    };
    index_segment(index).is_some() && fields.contains(&field)
}

/// The entry that cites `sentence` in `part`, a question or a block: it holds
/// the part's own exact sentence(s) that `sentence` stands for, and there is
/// none for a blank sentence or one that stands in no citable text.
pub(in crate::lessons) fn citation_entry(
    part: &Value,
    question: bool,
    sentence: &str,
    source_index: Value,
    quote: &str,
) -> Option<Value> {
    let (_, held, (from, to)) = cited_sentence(part, question, sentence)?;
    let exact = held.get(from..to)?;
    Some(json!({"sentence":exact,"sourceIndex":source_index,"quote":quote}))
}
