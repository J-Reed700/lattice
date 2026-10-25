//! The author selects captured evidence; it never has to reproduce a quote.
use super::{invalid, AnswerStandard};
use crate::features::learning::{
    dto::{LearningPracticeCitationDto, LearningSourceVersionDto},
    practice_generation::validate_citations,
};
use lattice_core::{
    application::ports::LLMPort,
    shared::{
        error::Result,
        text::{sentences, split_into_parts},
    },
};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(super) const MAX_EVIDENCE_CHOICES: usize = 32;

struct Choice {
    id: String,
    citation: LearningPracticeCitationDto,
    view: Value,
}

#[derive(Default)]
pub(super) struct EvidenceBank {
    choices: Vec<Choice>,
}

fn same_citation(left: &LearningPracticeCitationDto, right: &LearningPracticeCitationDto) -> bool {
    left.source_id == right.source_id
        && left.version_id == right.version_id
        && left.quote == right.quote
}

impl EvidenceBank {
    pub(super) async fn gather(
        llm: &dyn LLMPort,
        sources: &[LearningSourceVersionDto],
        query: &str,
        token_room: usize,
    ) -> Result<Self> {
        if sources.is_empty() || token_room == 0 {
            return Ok(Self::default());
        }
        let passages = crate::features::learning::reference_collection::snapshot_passages(
            llm, sources, query, token_room,
        )
        .await?;
        Self::from_passages(llm, sources, &passages, token_room)
    }

    pub(super) fn from_passages(
        llm: &dyn LLMPort,
        sources: &[LearningSourceVersionDto],
        passages: &Value,
        token_room: usize,
    ) -> Result<Self> {
        let mut bank = Self::default();
        let empty_cost = llm.count_tokens("[]");
        let documents = passages.as_array().ok_or_else(|| {
            invalid("Captured interview evidence must be a list of source passages.")
        })?;
        for document in documents {
            let source = sources
                .iter()
                .find(|source| {
                    document.get("sourceId").and_then(Value::as_str)
                        == Some(source.source_id.as_str())
                        && document.get("versionId").and_then(Value::as_str)
                            == Some(source.version.id.as_str())
                })
                .ok_or_else(|| {
                    invalid("Captured interview evidence is outside the supplied sources.")
                })?;
            let texts = document
                .get("passages")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("Captured interview evidence has no passage list."))?;
            for passage in texts {
                let text = passage
                    .as_str()
                    .ok_or_else(|| invalid("A captured interview passage must be text."))?;
                // Keep facts in separate choices. A fact in a paragraph with a
                // hostile instruction need not quote that unrelated instruction.
                for (start, end) in sentences(text) {
                    let sentence = text.get(start..end).ok_or_else(|| {
                        invalid("A captured interview sentence has invalid boundaries.")
                    })?;
                    for part in split_into_parts(sentence, 700) {
                        let quote = part.trim();
                        if quote.is_empty() {
                            continue;
                        }
                        let citation = LearningPracticeCitationDto {
                            source_id: source.source_id.clone(),
                            version_id: source.version.id.clone(),
                            quote: quote.into(),
                        };
                        validate_citations(std::slice::from_ref(&citation), sources)?;
                        if bank
                            .choices
                            .iter()
                            .any(|choice| same_citation(&choice.citation, &citation))
                        {
                            continue;
                        }
                        let id = format!("e{}", bank.choices.len() + 1);
                        let view = json!({
                            "id":id,"sourceId":citation.source_id,"versionId":citation.version_id,"quote":citation.quote,
                            "title":source.version.title,"url":source.version.resolved_url,"publisher":source.version.publisher,
                            "learnerProvidedNotes":source.version.extraction_version == crate::features::learning::source_library::PASTED_TEXT_EXTRACTION,
                        });
                        let next: Vec<&Value> = bank
                            .choices
                            .iter()
                            .map(|choice| &choice.view)
                            .chain(std::iter::once(&view))
                            .collect();
                        if llm
                            .count_tokens(&json!(next).to_string())
                            .saturating_sub(empty_cost)
                            > token_room
                        {
                            continue;
                        }
                        bank.choices.push(Choice { id, citation, view });
                        if bank.choices.len() == MAX_EVIDENCE_CHOICES {
                            return Ok(bank);
                        }
                    }
                }
            }
        }
        Ok(bank)
    }

    pub(super) fn ids(&self) -> Vec<String> {
        self.choices
            .iter()
            .map(|choice| choice.id.clone())
            .collect()
    }

    pub(super) fn view(&self) -> Value {
        json!(self
            .choices
            .iter()
            .map(|choice| &choice.view)
            .collect::<Vec<_>>())
    }

    /// Expand only IDs supplied in this authoring call. Stored standards keep
    /// the existing owned source/version/exact-quote representation.
    pub(super) fn read_standard(&self, raw: &str) -> Result<AnswerStandard> {
        let mut value: Value = serde_json::from_str(raw)
            .map_err(|_| invalid("The interview answer standard was malformed."))?;
        let points = value
            .get_mut("keyPoints")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid("The answer standard needs key points."))?;
        for point in points {
            let references = point
                .get_mut("citations")
                .ok_or_else(|| invalid("Each answer point needs a citation ID list."))?;
            let ids = references.as_array().ok_or_else(|| {
                invalid("Answer citations must be a list of supplied evidence IDs.")
            })?;
            let mut seen = HashSet::new();
            let mut citations = Vec::new();
            for id in ids {
                let id = id.as_str().ok_or_else(|| invalid("Select evidence IDs from sourceEvidence; do not write or paraphrase citation objects."))?;
                if !seen.insert(id.to_owned()) {
                    return Err(invalid("An answer point repeated an evidence ID."));
                }
                let choice = self
                    .choices
                    .iter()
                    .find(|choice| choice.id == id)
                    .ok_or_else(|| {
                        invalid(format!("Evidence ID {id} was not supplied in this call."))
                    })?;
                citations.push(choice.citation.clone());
            }
            *references = json!(citations);
        }
        let standard: AnswerStandard = serde_json::from_value(value)
            .map_err(|_| invalid("The interview answer standard was malformed."))?;
        let mut inline_tokens = Vec::new();
        for text in std::iter::once(standard.reference_answer.as_str())
            .chain(standard.key_points.iter().map(|point| point.point.as_str()))
            .chain(standard.acceptable_alternatives.iter().map(String::as_str))
            .chain(standard.common_mistakes.iter().map(String::as_str))
            .chain(standard.evidence_gaps.iter().map(String::as_str))
            .chain(standard.scoring_criteria.iter().flat_map(|criterion| {
                [
                    criterion.criterion.as_str(),
                    criterion.full_credit.as_str(),
                    criterion.partial_credit.as_str(),
                ]
            }))
        {
            for choice in &self.choices {
                let id = &choice.id;
                for pat in [
                    format!("[{id}]"),
                    format!("({id})"),
                    format!("({id},"),
                    format!(", {id})"),
                ] {
                    if text.contains(&pat) && !inline_tokens.contains(&pat) {
                        inline_tokens.push(pat);
                    }
                }
            }
        }
        if !inline_tokens.is_empty() {
            let mut msg = format!(
                "Evidence choice IDs belong only in keyPoints citations arrays. Remove inline {} labels from referenceAnswer, commonMistakes, scoringCriteria and all prose.",
                inline_tokens.join(", ")
            );
            if standard.reference_answer.trim().chars().count() > 4_000 {
                msg.push_str(" Also shorten referenceAnswer to under 3500 characters (maximum 4000 characters).");
            }
            return Err(invalid(msg));
        }
        Ok(standard)
    }

    /// A semantic repair must use the same author-facing representation, so
    /// an expanded review example cannot invite copying citation objects back.
    pub(super) fn author_view(&self, standard: &AnswerStandard) -> Result<Value> {
        let mut value =
            serde_json::to_value(standard).map_err(|error| invalid(error.to_string()))?;
        let points = value
            .get_mut("keyPoints")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid("The answer standard needs key points."))?;
        for (point, saved) in points.iter_mut().zip(&standard.key_points) {
            let mut ids = Vec::new();
            for citation in &saved.citations {
                let choice = self
                    .choices
                    .iter()
                    .find(|choice| same_citation(&choice.citation, citation))
                    .ok_or_else(|| {
                        invalid("The repaired standard cites evidence outside this call.")
                    })?;
                ids.push(choice.id.clone());
            }
            let citations = point
                .get_mut("citations")
                .ok_or_else(|| invalid("The repaired standard has no citation list."))?;
            *citations = json!(ids);
        }
        Ok(value)
    }
}
