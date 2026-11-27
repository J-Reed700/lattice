//! The evidence a lesson is written from, gathered before writing: plan the
//! topics the lesson teaches, retrieve passages for each from every saved
//! reference, let the model judge which passages cover each topic, research
//! the topics nothing covers, then pack the passages round-robin across
//! topics into the room the writer's window leaves.
//!
//! Code retrieves and packs; the model only plans and judges. An answer that
//! strays from its schema never stops the lesson: an unusable plan becomes
//! one topic, and a topic an unusable judgement leaves out is not judged.

use super::{complete_json, invalid, parse_json};
use crate::content_verification::research::research_topics;
use crate::dto::{LearningLessonDto, LearningProgramDto, LearningSourceDto};
use crate::lesson_drafts::{checkpoint, record_checkpoint};
use crate::lessons::trace;
use crate::model_call::ErrorClass;
use crate::reference_collection::{ReferenceCollection, ReferencePassage};
use lattice_core::application::ports::LLMPort;
use lattice_core::protocol;
use lattice_core::shared::error::Result;
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::protocol::Protocol;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod coverage;
mod pack;
mod plan;

pub(super) use coverage::judge;
#[cfg(test)]
use coverage::{EvidenceCoverage, Input, Passage, Subject};
pub(super) use pack::{assemble, pack};
use pack::{depth, retrieve};
#[cfg(test)]
use pack::passage_tokens;
pub(super) use plan::plan;
#[cfg(test)]
use plan::{parse_plan, plan_schema, EvidencePlan, PlannedTopic};

/// Changes when the plan, the coverage question or their answers change.
/// Distinct from content verification's `lesson-evidence` policy: these
/// keys hold the lesson's topic plan and coverage judgements, not claims.
pub(super) const POLICY: Protocol = protocol!("lesson-topic-evidence", 1);

/// One topic of a lesson's evidence plan, with the searches that find it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Topic {
    pub topic: String,
    pub queries: Vec<String>,
}

/// The passages retrieved for a lesson's topics and which cover each topic.
pub(super) struct Gathered<'a> {
    /// The collection after research, when research saved references.
    pub expanded: Option<ReferenceCollection<'a>>,
    pub(super) passages: Vec<ReferencePassage>,
    /// Per topic: its passages, best first.
    pub(super) candidates: Vec<Vec<usize>>,
    /// Per topic: the passages judged to cover it.
    pub(super) covering: Vec<Vec<usize>>,
}

impl Gathered<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.passages.is_empty()
    }
}

/// Retrieve every topic's passages, judge which cover each topic, and
/// research uncovered topics in rounds while a round covers a topic that was
/// not covered before.
pub(super) async fn gather<'a>(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'a>,
    topics: &[Topic],
    room: usize,
) -> Result<Gathered<'a>> {
    let depth = depth(llm, references, topics, room);
    let mut passages = Vec::new();
    let mut candidates = Vec::with_capacity(topics.len());
    for topic in topics {
        candidates.push(retrieve(references, topic, depth, &mut passages).await?);
    }
    let judged = judge(llm, topics, &passages, &candidates, references).await?;
    let unjudged: Vec<bool> = judged.iter().map(Option::is_none).collect();
    let mut covering: Vec<Vec<usize>> = judged.into_iter().map(Option::unwrap_or_default).collect();
    let mut expanded: Option<ReferenceCollection<'a>> = None;
    loop {
        let missing: Vec<(&str, &[String])> = topics
            .iter()
            .zip(&covering)
            .zip(&unjudged)
            .filter(|((_, covered), unjudged)| covered.is_empty() && !**unjudged)
            .map(|((topic, _), _)| (topic.topic.as_str(), topic.queries.as_slice()))
            .collect();
        tracing::info!(
            topics = topics.len(),
            uncovered = missing.len(),
            passages = passages.len(),
            depth,
            "Lesson topics checked against saved references"
        );
        for ((topic, covered), unjudged) in topics.iter().zip(&covering).zip(&unjudged) {
            let covering_passages: Vec<String> = covered
                .iter()
                .filter_map(|index| passages.get(*index))
                .map(|passage| {
                    format!(
                        "{}@{}-{}",
                        passage.source_id, passage.start_byte, passage.end_byte
                    )
                })
                .collect();
            trace::decided(
                trace::Stage::ReferenceGathering,
                &format!("topic {}", topic.topic),
                if *unjudged {
                    "not judged"
                } else if covered.is_empty() {
                    "not covered"
                } else {
                    "covered"
                },
                &format!("passages {covering_passages:?}"),
            );
        }
        if missing.is_empty() {
            break;
        }
        let Some(next) =
            research_topics(llm, expanded.as_ref().unwrap_or(references), &missing).await?
        else {
            break;
        };
        let active: &ReferenceCollection<'a> = expanded.insert(next);
        // Only passages a topic has not been judged against are judged again.
        let mut fresh = vec![Vec::new(); topics.len()];
        for ((((topic, list), fresh), covered), unjudged) in topics
            .iter()
            .zip(candidates.iter_mut())
            .zip(fresh.iter_mut())
            .zip(&covering)
            .zip(&unjudged)
        {
            if !covered.is_empty() || *unjudged {
                continue;
            }
            let mut found = retrieve(active, topic, depth, &mut passages).await?;
            *fresh = found
                .iter()
                .filter(|passage| !list.contains(passage))
                .copied()
                .collect();
            for passage in std::mem::take(list) {
                if !found.contains(&passage) {
                    found.push(passage);
                }
            }
            *list = found;
        }
        let judged = judge(llm, topics, &passages, &fresh, active).await?;
        let mut newly_covered = 0;
        for (covered, judged) in covering.iter_mut().zip(judged) {
            if let Some(judged) = judged.filter(|judged| covered.is_empty() && !judged.is_empty()) {
                newly_covered += 1;
                *covered = judged;
            }
        }
        if newly_covered == 0 {
            break;
        }
    }
    Ok(Gathered {
        expanded,
        passages,
        candidates,
        covering,
    })
}

#[cfg(test)]
#[path = "../evidence_tests.rs"]
mod tests;
