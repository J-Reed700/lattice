use super::Edit;
use crate::lessons::review_findings::{index_segment, leaves, FindingSite};
use lattice_core::shared::text::same_text;
use serde::Deserialize;
#[cfg(any(test, feature = "test-support"))]
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(Serialize, Debug, PartialEq)
)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    #[serde(default)]
    pub(crate) edits: Vec<Edit>,
    #[serde(default)]
    pub(crate) citations: Vec<Cite>,
    #[serde(default)]
    pub(crate) uncite: Vec<Uncite>,
}

impl Patch {
    /// Whether the patch asks for no change at all.
    pub(crate) fn asks_nothing(&self) -> bool {
        self.edits.is_empty() && self.citations.is_empty() && self.uncite.is_empty()
    }

    /// The findings its changes name, each once, in the order first named:
    /// edits, then citations, then uncites.
    pub(crate) fn findings(&self) -> Vec<String> {
        let named = self
            .edits
            .iter()
            .filter_map(|edit| edit.finding.as_deref())
            .chain(
                self.citations
                    .iter()
                    .filter_map(|cite| cite.finding.as_deref()),
            )
            .chain(self.uncite.iter().filter_map(Uncite::finding));
        let mut ids: Vec<String> = Vec::new();
        for id in named {
            if !ids.iter().any(|known| known == id) {
                ids.push(id.to_owned());
            }
        }
        ids
    }

    /// The patch with only the changes that name one of `ids`: unnamed edits
    /// and citations and path-only uncites are left out.
    pub(crate) fn only(&self, ids: &[String]) -> Self {
        let wanted = |id: Option<&str>| id.is_some_and(|id| ids.iter().any(|known| known == id));
        Self {
            edits: self
                .edits
                .iter()
                .filter(|edit| wanted(edit.finding.as_deref()))
                .cloned()
                .collect(),
            citations: self
                .citations
                .iter()
                .filter(|cite| wanted(cite.finding.as_deref()))
                .cloned()
                .collect(),
            uncite: self
                .uncite
                .iter()
                .filter(|uncite| wanted(uncite.finding()))
                .cloned()
                .collect(),
        }
    }
}

/// A citation to take back, as a path alone or with the finding it is for.
#[derive(Clone, Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(Serialize, Debug, PartialEq)
)]
#[serde(untagged)]
pub(crate) enum Uncite {
    Path(String),
    Named(NamedUncite),
}

#[derive(Clone, Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(Serialize, Debug, PartialEq)
)]
#[serde(deny_unknown_fields)]
pub(crate) struct NamedUncite {
    pub(crate) finding: String,
    pub(crate) path: String,
}

impl Uncite {
    pub(super) fn path(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Named(named) => &named.path,
        }
    }

    pub(super) fn finding(&self) -> Option<&str> {
        match self {
            Self::Path(_) => None,
            Self::Named(named) => Some(&named.finding),
        }
    }
}

/// A citation to append to a block's or question's `citations`: the sentence
/// of its body or explanation it is for, the excerpt that states it and an
/// exact span of that excerpt. The quote is checked after the edits, as any
/// other quotation is.
#[derive(Clone, Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(Serialize, Debug, PartialEq)
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Cite {
    #[serde(default)]
    pub(crate) finding: Option<String>,
    pub(crate) path: String,
    pub(crate) sentence: String,
    pub(crate) source_index: Option<u64>,
    pub(crate) quote: String,
}

/// The fields that teach, of a block and of a question: where a cited
/// sentence may stand and where a written sentence is held to the lesson's
/// own words.
const TAUGHT: (&[&str], &[&str]) = (&["body"], &["explanation", "rationales"]);

/// The texts of a block or question a cited sentence may stand in: a block's
/// body, a question's explanation or any of its choice reasons.
pub(in crate::lessons) fn citable_texts(part: &Value, question: bool) -> Vec<&str> {
    let fields = if question { TAUGHT.1 } else { TAUGHT.0 };
    fields
        .iter()
        .filter_map(|field| part.get(*field))
        .flat_map(|value| leaves(value).into_iter().map(|(_, text)| text))
        .collect()
}

/// Whether `path` names a text that teaches in a block or question.
pub(super) fn taught_at(path: &str) -> bool {
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
    if sentence.trim().is_empty() {
        return None;
    }
    let exact = citable_texts(part, question).into_iter().find_map(|held| {
        let span = same_text::find(held, sentence)?;
        same_text::exact_sentences(held, span)
    })?;
    Some(json!({"sentence":exact,"sourceIndex":source_index,"quote":quote}))
}

/// Appends `cite` to the citations of the block or question it names. Its
/// sentence must stand in that part's body, or in a question's explanation
/// or a choice's reason, and the same citation is not listed twice.
pub(super) fn append(lesson: &mut Value, cite: &Cite) -> std::result::Result<(), String> {
    let site = FindingSite::of_pointer(&cite.path);
    let text = match site {
        FindingSite::Section(_) => "body",
        FindingSite::Question(_) | FindingSite::Lesson => "explanation",
    };
    if site.pointer().as_deref() != Some(cite.path.as_str()) {
        return Err(format!(
            "names {}, which is not a block or a question.",
            cite.path
        ));
    }
    let target = lesson
        .pointer(&cite.path)
        .filter(|part| part.is_object())
        .ok_or_else(|| format!("names {}, which is not in the draft.", cite.path))?;
    let entry = citation_entry(
        target,
        text == "explanation",
        &cite.sentence,
        json!(cite.source_index),
        &cite.quote,
    );
    let Some(entry) = entry else {
        let place = if text == "explanation" {
            "explanation or choice reasons"
        } else {
            text
        };
        return Err(format!(
            "names a sentence that is not in the {place} of {}; copy it exactly.",
            cite.path
        ));
    };
    let part = lesson
        .pointer_mut(&cite.path)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("names {}, which is not in the draft.", cite.path))?;
    let listed = part
        .entry("citations")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| format!("names {}, which holds no list of citations.", cite.path))?;
    if listed.contains(&entry) {
        return Err(format!("is listed in {} already.", cite.path));
    }
    listed.push(entry);
    Ok(())
}
