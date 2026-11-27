//! A reply as the frontend draws it: text between references.

use crate::domain::citation::Relation;
use serde::{Deserialize, Serialize};

/// A line range a reference names inside what it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LinesDto {
    pub first: u32,
    pub last: u32,
}

/// How a sentence uses a source, as the frontend names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RelationDto {
    Quotes,
    Restates,
    Summarizes,
    Infers,
    Agrees,
    Backs,
    Cites,
    Contradicts,
}

impl From<Relation> for RelationDto {
    fn from(relation: Relation) -> Self {
        match relation {
            Relation::Quotes => Self::Quotes,
            Relation::Restates => Self::Restates,
            Relation::Summarizes => Self::Summarizes,
            Relation::Infers => Self::Infers,
            Relation::Agrees => Self::Agrees,
            Relation::Backs => Self::Backs,
            Relation::Cites => Self::Cites,
            Relation::Contradicts => Self::Contradicts,
        }
    }
}

/// A span of a source version's saved text, in UTF-8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ByteSpanDto {
    pub start: usize,
    pub end: usize,
}

/// The words a citation rests on, with where they stand when that is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AnchorDto {
    pub exact: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<ByteSpanDto>,
}

/// How a marker uses its source, and the words it rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SupportDto {
    pub relation: RelationDto,
    pub anchors: Vec<AnchorDto>,
}

impl Default for SupportDto {
    fn default() -> Self {
        Self {
            relation: RelationDto::Cites,
            anchors: Vec::new(),
        }
    }
}

impl SupportDto {
    /// Unchecked citation support carrying one anchor per non-empty excerpt.
    pub fn cites<I, S>(excerpts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let anchors = excerpts
            .into_iter()
            .filter_map(|excerpt| {
                let trimmed = excerpt.as_ref().trim();
                (!trimmed.is_empty()).then(|| AnchorDto {
                    exact: trimmed.to_string(),
                    span: None,
                })
            })
            .collect();
        Self {
            relation: RelationDto::Cites,
            anchors,
        }
    }

    /// Check `quote` against `full_text` with `same_text`: if found, emit
    /// `RelationDto::Quotes` with the exact UTF-8 byte span in `full_text`;
    /// otherwise emit `RelationDto::Cites`.
    pub fn verify_quote(full_text: Option<&str>, quote: &str) -> Self {
        let trimmed = quote.trim();
        if trimmed.is_empty() {
            return Self::default();
        }
        if let Some((text, (start, end))) = full_text
            .and_then(|text| crate::shared::text::same_text::find(text, trimmed).map(|s| (text, s)))
        {
            if let Some(exact) = text.get(start..end) {
                return Self {
                    relation: RelationDto::Quotes,
                    anchors: vec![AnchorDto {
                        exact: exact.to_string(),
                        span: Some(ByteSpanDto { start, end }),
                    }],
                };
            }
        }
        Self {
            relation: RelationDto::Cites,
            anchors: vec![AnchorDto {
                exact: trimmed.to_string(),
                span: None,
            }],
        }
    }
}

/// A marker of a reply that resolved, as the frontend draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReplyReferenceDto {
    /// The handle as written, `[s12]` or `[f2:10-20]`.
    pub written: String,
    /// `source` or `file`.
    pub kind: String,
    /// The handle's own number.
    pub number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<LinesDto>,
    /// What a kind drawn by name shows: a file's path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How the marked sentence uses its source; none when nothing says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<SupportDto>,
}

/// One piece of a reply, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ReplyPieceDto {
    Text { text: String },
    Reference { reference: ReplyReferenceDto },
    Quoted { text: String },
}

#[cfg(test)]
mod support_tests {
    use super::{ByteSpanDto, RelationDto, SupportDto};

    #[test]
    fn verify_quote_returns_quotes_with_utf8_byte_span_when_found() {
        let full = "Intro — résumé section.\nExact words matter here.";
        let support = SupportDto::verify_quote(Some(full), "Exact  words\nmatter");
        assert_eq!(support.relation, RelationDto::Quotes);
        assert_eq!(support.anchors.len(), 1);
        assert_eq!(support.anchors[0].exact, "Exact words matter");
        let start = full.find("Exact words").unwrap();
        let end = start + "Exact words matter".len();
        assert_eq!(support.anchors[0].span, Some(ByteSpanDto { start, end }));
    }

    #[test]
    fn verify_quote_falls_back_to_cites_when_unmatched_or_missing_text() {
        let missing = SupportDto::verify_quote(Some("Different text."), "Not present");
        assert_eq!(missing.relation, RelationDto::Cites);
        assert_eq!(missing.anchors.len(), 1);
        assert_eq!(missing.anchors[0].exact, "Not present");
        assert_eq!(missing.anchors[0].span, None);

        let no_text = SupportDto::verify_quote(None, "Cited wording");
        assert_eq!(no_text.relation, RelationDto::Cites);
        assert_eq!(no_text.anchors.len(), 1);
        assert_eq!(no_text.anchors[0].span, None);

        let blank = SupportDto::verify_quote(Some("Text"), "   ");
        assert_eq!(blank.relation, RelationDto::Cites);
        assert!(blank.anchors.is_empty());
    }
}
