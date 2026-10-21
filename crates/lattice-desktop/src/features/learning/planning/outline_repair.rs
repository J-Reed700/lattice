//! Repair by finding kind. A factual finding gets targeted research and a
//! strict entailment judgment: what the evidence supports is closed by code,
//! what it contradicts or leaves unsupported is rewritten. A plan finding is
//! rewritten. Repair never adds a citation of its own: a rewrite keeps or
//! drops what it was given, and evidence linking then runs on that module.
use crate::features::learning::lessons::content_verification::{
    judge_for_publication, ranking, shown_of,
};
use crate::features::learning::outline_linking::{self, CitationReceipt, ItemKind};
use crate::features::learning::outline_references::{
    self, GatherPurpose, PlannedSubtopic, ReferencePlan,
};
use crate::features::learning::outline_review::{item_text, kind_name};
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::features::learning::references::web_research;
use crate::features::learning::{
    dto::*, generation, outline_draft::*, outline_progress::OutlineProgress,
    outline_progress::OutlineStage, persistence::hash_text, repository::LearningRepository,
};
use lattice_core::application::ports::WebServiceTrait;
use lattice_core::application::services::claim_verification::{
    ClaimJudgment, ClaimVerdict, JudgeOutcome,
};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

const CLAIM_POLICY: &str = "outline-claim-v1";

/// Per-kind, per-attempt repair instructions (design section 6.1).
pub(in crate::features::learning) const CORRECT: &str =
    "Correct the assertion to what the contradicting passage says.";
pub(in crate::features::learning) const NARROW: &str =
    "Narrow or remove the unsupported assertion; do not add citations.";
pub(in crate::features::learning) const REMOVE: &str =
    "Remove the assertion or restate it as a learning activity rather than a fact.";
pub(in crate::features::learning) const PLAN_FIX: &str =
    "Fix the plan defect described; do not add or change citations.";
pub(in crate::features::learning) const PLAN_SMALLEST: &str =
    "This is the last attempt and the previous rewording did not remove the defect. Rewrite the text at this finding's path so the reason no longer applies: delete or replace every phrase the reason objects to instead of rewording it. If a sentence cannot be fixed, remove it; if the item cannot be fixed, drop it.";

#[derive(Debug, Default, Clone)]
pub(in crate::features::learning) struct RepairOutcome {
    /// A module rewrite was saved.
    pub changed: bool,
    /// A factual finding was closed by evidence.
    pub evidence_added: bool,
    /// Why each module whose correction could not be used kept its saved
    /// text, as "Module N: reason". Their findings stay open.
    pub refused: Vec<String>,
}

enum Resolution {
    Closed,
    Rewrite(&'static str),
}

fn issue_mut<'a>(draft: &'a mut OutlineDraft, id: &str) -> Result<&'a mut LearningOutlineIssueDto> {
    draft
        .review
        .issues
        .iter_mut()
        .find(|issue| issue.id == id)
        .ok_or_else(|| AppError::InternalError("A repaired finding is no longer open.".into()))
}

fn item_kind(path: &str) -> Option<ItemKind> {
    if path.ends_with("/summary") {
        Some(ItemKind::Summary)
    } else if path.contains("/outcomes/") {
        Some(ItemKind::Outcome)
    } else if path.contains("/lessons/") {
        Some(ItemKind::Lesson)
    } else {
        None
    }
}

fn short(text: &str) -> String {
    if text.chars().count() <= 120 {
        return text.into();
    }
    format!("{}…", text.chars().take(119).collect::<String>().trim_end())
}

fn claim_key(model: &str, claim: &str, passages: &[ReferencePassage]) -> String {
    hash_text(
        &json!({"policy":CLAIM_POLICY,"model":model,"claim":claim.trim(),
            "passages":passages.iter().map(|p| json!({"source":p.source_id,"sha256":hash_text(&p.text)})).collect::<Vec<_>>()})
        .to_string(),
    )
}

/// The judge's outcome for `claim`, and the positions of the `passages` it
/// was shown (see `judge_for_publication`).
async fn judge(
    llm: &dyn LLMPort,
    claim: &str,
    passages: &[String],
    ranking: &[usize],
) -> Result<(JudgeOutcome, Vec<usize>)> {
    let judged = judge_for_publication(llm, claim, passages, ranking).await;
    match judged.judgment {
        ClaimJudgment::Judged(outcome) => Ok((outcome, judged.shown)),
        ClaimJudgment::Failed(error) => Err(error),
        ClaimJudgment::Unusable => Err(AppError::Other(
            "The claim check returned no usable verdict. The finding is kept; Repair can try again."
                .into(),
        )),
    }
}

/// Attach verified evidence to the item at `path` and record both receipts:
/// the citation receipt keeps the citation through later checks and
/// rewrites. A module-level finding has no item to carry a citation.
fn attach(
    draft: &mut OutlineDraft,
    sources: &[LearningSourceDto],
    model: &str,
    path: &str,
    source_id: &str,
    quote: &str,
) -> Result<()> {
    let Some(kind) = item_kind(path) else {
        return Ok(());
    };
    let index = sources
        .iter()
        .position(|source| source.id == source_id)
        .ok_or_else(|| AppError::InternalError("Evidence names an unknown source.".into()))?;
    generation::optional_citation(sources, Some(index), quote)?;
    let text = item_text(&draft.candidate, path)
        .ok_or_else(|| AppError::InternalError("Evidence names an unavailable item.".into()))?;
    let field = draft
        .candidate
        .pointer_mut(path)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::InternalError("Evidence names an unavailable item.".into()))?;
    field.insert("sourceIndex".into(), json!(index));
    field.insert("quote".into(), json!(quote));
    let hash = outline_linking::source_hashes(sources)
        .into_iter()
        .nth(index)
        .unwrap_or_default();
    draft.citation_receipts.insert(
        outline_linking::receipt_key(model, kind, &text, &hash, quote),
        CitationReceipt {
            covers: true,
            reason: "The passage entails the assertion (strict claim check during repair).".into(),
            checked_at: chrono::Utc::now().timestamp_millis(),
        },
    );
    Ok(())
}

/// The finding a targeted research pass serves, and its place in the run.
struct ResearchFinding<'a> {
    issue: &'a LearningOutlineIssueDto,
    topic: &'a str,
    position: usize,
    total: usize,
}

/// Targeted research for one factual finding: plan public searches from the
/// claim and the reviewer's reason, run them, select once, capture the chosen
/// pages and persist them with the draft. Everything saved is available to
/// the judge and to every later pass.
async fn research(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    web: &dyn WebServiceTrait,
    program: &mut LearningProgramDto,
    draft: &mut OutlineDraft,
    sources: &mut Vec<LearningSourceDto>,
    finding: ResearchFinding<'_>,
    progress: &OutlineProgress,
) -> Result<()> {
    let ResearchFinding {
        issue,
        topic,
        position,
        total,
    } = finding;
    let gaps = [json!({"claim":issue.claim,"missingEvidence":issue.message})];
    let already: Vec<String> = draft
        .research_log
        .get(&issue.id)
        .cloned()
        .unwrap_or_default();
    let catalog = ReferenceCollection::lexical(sources)?.catalog(llm);
    progress.detail(format!(
        "Finding {position} of {total}: planning searches for \"{}\"",
        short(&issue.claim)
    ));
    let planned = web_research::plan_needs(llm, topic, &gaps, catalog, &already).await?;
    let private = [
        draft.request.goal.as_str(),
        draft.request.prior_knowledge.as_str(),
    ];
    let needs: Vec<_> = planned
        .into_iter()
        .map(|served| served.need)
        .filter(|need| {
            if outline_references::copies_private_text(&need.query, &private) {
                tracing::warn!(finding = %issue.id, "Dropped a research query that copied the learner's goal or prior knowledge");
                return false;
            }
            !already.contains(&need.query)
        })
        .collect();
    if needs.is_empty() {
        tracing::info!(finding = %issue.id, "No new public searches were planned for this finding");
        return Ok(());
    }
    let log = draft.research_log.entry(issue.id.clone()).or_default();
    log.extend(needs.iter().map(|need| need.query.clone()));
    let searched = log.clone();
    issue_mut(draft, &issue.id)?.searched = searched;
    let plan = ReferencePlan {
        subtopics: vec![PlannedSubtopic {
            title: issue.claim.clone(),
            needs,
        }],
    };
    let found = outline_references::gather_for(
        llm,
        web,
        &plan,
        sources,
        progress,
        GatherPurpose::Finding {
            topic,
            reason: &issue.message,
            position,
            total,
        },
    )
    .await?;
    for (source, reference) in found.sources.iter().zip(&found.references) {
        draft.source_ids.push(source.id.clone());
        sources.push(outline_references::authoring_copy(source, reference));
    }
    draft.review.references_gathered += found.sources.len() as u32;
    tracing::info!(finding = %issue.id, searches = found.searches, saved = found.sources.len(), unavailable = found.unavailable, "Targeted research for an outline finding");
    repo.checkpoint_outline(program, draft, &found.sources, &found.references)
        .await?;
    Ok(())
}

/// First attempt on a factual finding: research, then judge the claim against
/// every saved source. Supported closes the finding by code; contradicted or
/// unsupported schedules a rewrite with the matching instruction.
async fn resolve_factual(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    program: &mut LearningProgramDto,
    draft: &mut OutlineDraft,
    sources: &mut Vec<LearningSourceDto>,
    web: Option<&dyn WebServiceTrait>,
    id: &str,
    position: usize,
    total: usize,
    progress: &OutlineProgress,
) -> Result<Resolution> {
    let issue = issue_mut(draft, id)?.clone();
    progress.stage(OutlineStage::Researching);
    progress.detail(format!(
        "Finding {position} of {total}: checking \"{}\"",
        short(&issue.claim)
    ));
    let topic = module_index(&issue.path)
        .and_then(|index| {
            draft
                .candidate
                .get("modules")?
                .get(index)?
                .get("title")?
                .as_str()
        })
        .unwrap_or_default()
        .to_owned();
    if let Some(web) = web {
        let finding = ResearchFinding {
            issue: &issue,
            topic: &topic,
            position,
            total,
        };
        research(repo, llm, web, program, draft, sources, finding, progress).await?;
    }
    let claim = issue.claim.clone();
    let collection = ReferenceCollection::lexical(sources)?;
    // A finding is checked alone, so its first page may fill the checker's
    // window. An unsupported verdict walks on to the next page, passages not
    // yet shown, while that page shares a content term with the claim.
    let share = crate::features::learning::content_verification::claim_share(llm, &claim, 1);
    let fits = |passages: &[ReferencePassage]| {
        crate::features::learning::content_verification::within_share(llm, share, passages.iter())
    };
    let mut shown: Vec<ReferencePassage> = Vec::new();
    let mut decision = None;
    let subject = collection.subject_of(&claim).await?;
    loop {
        let (passages, base) = if sources.is_empty() {
            (vec![], vec![])
        } else {
            collection.verification_page(&claim, &shown, fits).await?
        };
        let more = if shown.is_empty() {
            !passages.is_empty()
        } else {
            passages
                .iter()
                .any(|passage| subject.mentioned_in(&passage.text))
        };
        if !more {
            break;
        }
        let key = claim_key(llm.model_name(), &claim, &passages);
        let (judged, seen) = match draft.claim_receipts.get(&key) {
            Some(receipt) => (
                (
                    receipt.verdict,
                    receipt.reason.clone(),
                    receipt
                        .source_id
                        .clone()
                        .zip(receipt.supporting_quote.clone()),
                ),
                receipt
                    .shown
                    .clone()
                    .unwrap_or_else(|| (0..passages.len()).collect()),
            ),
            None => {
                progress.detail(format!(
                    "Finding {position} of {total}: judging the claim against {} saved passages",
                    passages.len()
                ));
                let texts: Vec<String> = passages.iter().map(|p| p.text.clone()).collect();
                let (outcome, seen) = judge(llm, &claim, &texts, &ranking(&passages)).await?;
                let evidence = outcome.quote.as_deref().and_then(|located| {
                    locate(located, outcome.verdict, &passages, &base, sources)
                });
                let reason = outcome
                    .reason
                    .unwrap_or_else(|| "The claim check gave no reason.".into());
                // A check that could not run is not a decision to keep.
                if outcome.verdict != ClaimVerdict::Unverified {
                    draft.claim_receipts.insert(
                        key,
                        OutlineClaimReceipt {
                            verdict: outcome.verdict,
                            reason: reason.clone(),
                            supporting_quote: evidence.as_ref().map(|(_, quote)| quote.clone()),
                            checked_at: chrono::Utc::now().timestamp_millis(),
                            claim: claim.clone(),
                            path: issue.path.clone(),
                            source_id: evidence.as_ref().map(|(id, _)| id.clone()),
                            shown: (seen.len() < passages.len()).then(|| seen.clone()),
                        },
                    );
                }
                ((outcome.verdict, reason, evidence), seen)
            }
        };
        // The judge saw the passages at `seen`; one cut for its window was
        // never compared, so it stays unshown for the next page.
        let passages = shown_of(&passages, &seen);
        let unsupported = judged.0 == ClaimVerdict::Unsupported;
        // An unsupported verdict on a deeper page does not replace the first.
        if decision.is_none() || !unsupported {
            decision = Some((judged, passages.clone()));
        }
        if !unsupported {
            break;
        }
        shown.extend(passages);
    }
    let Some(((verdict, reason, evidence), page)) = decision else {
        issue_mut(draft, id)?.message = "No saved reference addresses this claim.".into();
        return Ok(Resolution::Rewrite(NARROW));
    };
    // A supported assertion stands only when its support meets the bar for
    // its kind, as a lesson's claim does (source provenance §6.2).
    let (verdict, reason) = match (&verdict, &evidence) {
        (ClaimVerdict::Supported, Some((source_id, quote))) => {
            let unit = module_index(&issue.path).unwrap_or_default();
            match insufficient(llm, unit, &claim, &page, source_id, quote).await? {
                Some(lacks) => (ClaimVerdict::Unsupported, lacks),
                None => (verdict, reason),
            }
        }
        _ => (verdict, reason),
    };
    tracing::info!(finding = %id, path = %issue.path, ?verdict, "Judged an outline assertion against the saved references");
    match verdict {
        ClaimVerdict::Supported => {
            if let Some((source_id, quote)) = &evidence {
                attach(
                    draft,
                    sources,
                    llm.model_name(),
                    &issue.path,
                    source_id,
                    quote,
                )?;
                draft.review.citations_verified = outline_linking::cited_count(&draft.candidate);
            }
            draft.review.issues.retain(|open| open.id != id);
            progress.detail(format!(
                "Finding {position} of {total}: supported by a saved reference; closed"
            ));
            repo.checkpoint_outline(program, draft, &[], &[]).await?;
            Ok(Resolution::Closed)
        }
        ClaimVerdict::Contradicted => {
            let open = issue_mut(draft, id)?;
            if let Some((source_id, quote)) = evidence {
                open.quote = quote;
                open.source_id = Some(source_id);
            }
            open.message = format!("Contradicted by a saved reference: {reason}");
            repo.checkpoint_outline(program, draft, &[], &[]).await?;
            Ok(Resolution::Rewrite(CORRECT))
        }
        ClaimVerdict::Unsupported | ClaimVerdict::Unverified => {
            issue_mut(draft, id)?.message =
                format!("Not supported by the saved references: {reason}");
            Ok(Resolution::Rewrite(NARROW))
        }
    }
}

/// One repair pass over the actionable findings. Factual findings are
/// researched and judged first (attempt 1) or scheduled for removal (attempt
/// 2); then each module with findings to act on is rewritten once, within
/// its budget. Attempts and budgets are spent here; `finish` decides what is
/// exhausted after the next review.
pub(in crate::features::learning) async fn repair(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    program: &mut LearningProgramDto,
    draft: &mut OutlineDraft,
    sources: &mut Vec<LearningSourceDto>,
    web: Option<&dyn WebServiceTrait>,
    budgets: &mut ModuleBudgets,
    progress: &OutlineProgress,
) -> Result<RepairOutcome> {
    let mut outcome = RepairOutcome::default();
    draft.review.repair_passes += 1;
    repo.checkpoint_outline(program, draft, &[], &[]).await?;
    // Step 1: factual findings.
    let mut instructions: BTreeMap<String, &'static str> = BTreeMap::new();
    let factual: Vec<String> = draft
        .review
        .issues
        .iter()
        .filter(|issue| issue.kind == LearningOutlineIssueKind::Factual && !issue.exhausted)
        .map(|issue| issue.id.clone())
        .collect();
    let total = factual.len();
    for (position, id) in factual.iter().enumerate() {
        progress.check_cancelled()?;
        let attempt = {
            let issue = issue_mut(draft, id)?;
            issue.attempts += 1;
            issue.attempts
        };
        if attempt > 1 {
            // The planner already saw everything searched; no more research.
            instructions.insert(id.clone(), REMOVE);
            continue;
        }
        match resolve_factual(
            repo,
            llm,
            program,
            draft,
            sources,
            web,
            id,
            position + 1,
            total,
            progress,
        )
        .await?
        {
            Resolution::Closed => outcome.evidence_added = true,
            Resolution::Rewrite(instruction) => {
                instructions.insert(id.clone(), instruction);
            }
        }
    }
    // Step 2: module rewrites for plan findings and the factual findings the
    // evidence did not settle. Every finding names the module it belongs to.
    let module_count = draft
        .candidate
        .get("modules")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut counted: HashSet<String> = HashSet::new();
    for index in 0..module_count {
        progress.check_cancelled()?;
        let path = format!("/modules/{index}");
        let ids: Vec<String> = draft
            .review
            .issues
            .iter()
            .filter(|issue| issue.kind != LearningOutlineIssueKind::References && !issue.exhausted)
            .filter(|issue| issue.path == path || issue.path.starts_with(&format!("{path}/")))
            .filter(|issue| {
                issue.kind == LearningOutlineIssueKind::Plan || instructions.contains_key(&issue.id)
            })
            .map(|issue| issue.id.clone())
            .collect();
        if ids.is_empty() {
            continue;
        }
        if !budgets.spend(index) {
            tracing::info!(
                module = index + 1,
                findings = ids.len(),
                "Module rewrite budget spent; its findings wait for the user"
            );
            continue;
        }
        progress.stage(OutlineStage::Repairing);
        progress.detail(format!(
            "Module {} of {module_count}: rewriting for {} {}",
            index + 1,
            ids.len(),
            if ids.len() == 1 {
                "finding"
            } else {
                "findings"
            }
        ));
        let mut listed = Vec::new();
        for id in &ids {
            let issue = issue_mut(draft, id)?;
            if issue.kind == LearningOutlineIssueKind::Plan && counted.insert(id.clone()) {
                issue.attempts += 1;
            }
            let instruction = match issue.kind {
                LearningOutlineIssueKind::Plan if issue.attempts >= ATTEMPT_LIMIT => PLAN_SMALLEST,
                LearningOutlineIssueKind::Plan => PLAN_FIX,
                _ => instructions.get(id).copied().unwrap_or(NARROW),
            };
            let mut entry = json!({"id":issue.id,"path":issue.path,"kind":kind_name(issue.kind),"claim":issue.claim,"reason":issue.message,"instruction":instruction});
            if let (Some(source_id), Some(object)) = (&issue.source_id, entry.as_object_mut()) {
                object.insert("contradictingPassage".into(), json!({"sourceIndex":sources.iter().position(|s| &s.id == source_id),"quote":issue.quote}));
            }
            listed.push(entry);
        }
        let rewrite =
            rewrite_module(repo, llm, program, draft, sources, index, &listed, progress).await?;
        if let Rewrite::Refused(reason) = &rewrite {
            outcome
                .refused
                .push(format!("Module {}: {reason}", index + 1));
        }
        if matches!(rewrite, Rewrite::Saved) {
            outcome.changed = true;
        } else {
            // A last attempt that changed nothing, whether its answer kept the
            // module or could not be used, settles a plan finding: it is
            // exhausted and stays as an advisory suggestion, like any
            // exhausted one.
            for id in &ids {
                let issue = issue_mut(draft, id)?;
                if issue.kind == LearningOutlineIssueKind::Plan && issue.attempts >= ATTEMPT_LIMIT {
                    issue.exhausted = true;
                    tracing::info!(finding = %id, "The last rewrite left a plan finding's module unchanged; it stays as a suggestion");
                }
            }
        }
    }
    Ok(outcome)
}

mod evidence;
use evidence::{insufficient, locate};
mod rewrite;
use rewrite::{rewrite_module, Rewrite};

#[cfg(test)]
mod tests;
