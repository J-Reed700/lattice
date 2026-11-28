//! Review and single-shot repair for one authored quiz-item batch.
use super::{AuthoredItem, AuthoringNeed, Batch, Repair, Written};
use crate::assessment::item_review::{self, Context, Entry, Findings, RepeatOf};
use crate::assessment_engine::LearningAssessmentCandidate;
use crate::model_call::ErrorClass;
use crate::reference_collection::ReferenceCollection;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;

/// An item that was written and dropped, and whether it was dropped as a
/// repeat.
pub(super) struct Dropped {
    pub(super) reason: String,
    pub(super) repeat: bool,
}

impl Dropped {
    fn flawed(reason: String) -> Self {
        Self {
            reason,
            repeat: false,
        }
    }

    /// `held[n]` is whether the new item at position `n` is one the bank holds.
    fn of(findings: &Findings, held: &[bool]) -> Self {
        Self {
            reason: findings.texts().join(" "),
            repeat: !findings.has_defect()
                && findings.repeats.iter().any(|r| match r.of {
                    RepeatOf::Held => true,
                    RepeatOf::New(n) => held.get(n).copied().unwrap_or(false),
                }),
        }
    }
}

/// An item to write again, and every fault it was refused for.
struct ToRewrite {
    prompt: String,
    options: Vec<String>,
    findings: Vec<String>,
    difficulty: u8,
}

/// What reviewing a set of items made of them: a finding list per item, or
/// the reason none could be reviewed.
async fn review_items(
    llm: &dyn LLMPort,
    need: &AuthoringNeed,
    items: &[AuthoredItem],
    learner_has: &[String],
) -> Result<std::result::Result<Vec<Findings>, String>> {
    let candidates: Vec<LearningAssessmentCandidate> =
        items.iter().map(|i| i.candidate.clone().into()).collect();
    let entries: Vec<Entry<'_>> = candidates
        .iter()
        .zip(items)
        .map(|(item, authored)| Entry {
            item,
            quote: &authored.source_quote,
        })
        .collect();
    let context = Context {
        outcome: &need.outcome_title,
        outcome_description: &need.outcome_description,
        learner_has,
    };
    match item_review::review(llm, &entries, &context).await {
        Ok(findings) => Ok(Ok(findings)),
        Err(error) if ErrorClass::of(&error) == ErrorClass::Unusable => {
            Ok(Err(format!("The question could not be reviewed: {error}")))
        }
        Err(error) => Err(error),
    }
}

/// Review one round's items: the sound ones are kept as written; each the
/// checks refused or review found findings on is rewritten once, concurrently,
/// and the rewrites are reviewed together. Returns the items kept and why the
/// others were dropped.
pub(super) async fn reviewed_round(
    llm: &dyn LLMPort,
    need: &AuthoringNeed,
    collection: &ReferenceCollection<'_>,
    written: Written,
    companies: &[String],
) -> Result<(Vec<AuthoredItem>, Vec<Dropped>)> {
    let mut fresh = Vec::new();
    let mut flawed = Vec::new();
    let mut known_with_batch = need.existing_prompts.clone();
    let mut dropped = Vec::new();
    for result in written {
        match result {
            Ok(item) => {
                let candidate: LearningAssessmentCandidate = item.candidate.clone().into();
                if let Some(reason) =
                    item_review::repeat_overlap_reason(&candidate, &known_with_batch)
                {
                    if need.pass == super::BankPass::Retakes {
                        dropped.push(Dropped {
                            reason,
                            repeat: true,
                        });
                    } else {
                        flawed.push(ToRewrite {
                            prompt: item.candidate.prompt,
                            options: item.candidate.options,
                            findings: vec![reason],
                            difficulty: item.candidate.difficulty,
                        });
                    }
                } else {
                    known_with_batch.push(item_review::item_repeat_descriptor(&candidate));
                    fresh.push(item);
                }
            }
            Err(unbuilt) => flawed.push(ToRewrite {
                prompt: unbuilt.prompt,
                options: unbuilt.options,
                findings: unbuilt.faults,
                difficulty: unbuilt.difficulty,
            }),
        }
    }
    let mut kept = Vec::new();
    if !fresh.is_empty() {
        match review_items(llm, need, &fresh, &need.existing_prompts).await? {
            Ok(findings) => {
                for (item, findings) in fresh.into_iter().zip(findings) {
                    if findings.unreviewed {
                        dropped.push(Dropped::flawed(item_review::UNREVIEWED.into()));
                    } else if findings.is_sound() {
                        kept.push(item);
                    } else {
                        flawed.push(ToRewrite {
                            prompt: item.candidate.prompt,
                            options: item.candidate.options,
                            findings: findings.texts(),
                            difficulty: item.candidate.difficulty,
                        });
                    }
                }
            }
            Err(reason) => dropped.extend(fresh.iter().map(|_| Dropped::flawed(reason.clone()))),
        }
    }
    if flawed.is_empty() {
        return Ok((kept, dropped));
    }
    let mut known = need.existing_prompts.clone();
    known.extend(kept.iter().map(|i| {
        let candidate: LearningAssessmentCandidate = i.candidate.clone().into();
        item_review::item_repeat_descriptor(&candidate)
    }));
    let mut own = need.clone();
    own.existing_prompts = known.clone();
    let repairs = flawed.iter().map(|flawed| {
        let batch = Batch {
            need: &own,
            count: 1,
            notes: vec![],
            repair: Some(Repair {
                question: flawed.prompt.clone(),
                options: flawed.options.clone(),
                findings: flawed.findings.clone(),
                difficulty: flawed.difficulty,
            }),
            companies,
        };
        async move {
            match batch.author(llm, collection).await {
                Ok(items) => Ok(items.and_then(|items| items.into_iter().next())),
                Err(error)
                    if matches!(
                        ErrorClass::of(&error),
                        ErrorClass::Unusable | ErrorClass::DoesNotFit
                    ) =>
                {
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        }
    });
    let repaired = futures::future::try_join_all(repairs).await?;
    let mut again = Vec::new();
    let mut second_known = known.clone();
    for (flawed, repair) in flawed.into_iter().zip(repaired) {
        match repair {
            Some(Ok(item)) => {
                let candidate: LearningAssessmentCandidate = item.candidate.clone().into();
                if let Some(reason) = item_review::repeat_overlap_reason(&candidate, &second_known)
                {
                    dropped.push(Dropped {
                        reason,
                        repeat: true,
                    });
                } else {
                    second_known.push(item_review::item_repeat_descriptor(&candidate));
                    again.push(item);
                }
            }
            Some(Err(unbuilt)) => dropped.push(Dropped::flawed(unbuilt.faults.join(" "))),
            None => dropped.push(Dropped::flawed(flawed.findings.join(" "))),
        }
    }
    if again.is_empty() {
        return Ok((kept, dropped));
    }
    match review_items(llm, need, &again, &known).await? {
        Ok(second) => {
            let mut held: Vec<bool> = Vec::new();
            for (item, findings) in again.into_iter().zip(second) {
                if findings.is_sound() {
                    kept.push(item);
                    held.push(true);
                } else {
                    let drop = Dropped::of(&findings, &held);
                    held.push(drop.repeat);
                    dropped.push(drop);
                }
            }
        }
        Err(reason) => dropped.extend(again.iter().map(|_| Dropped::flawed(reason.clone()))),
    }
    Ok((kept, dropped))
}
