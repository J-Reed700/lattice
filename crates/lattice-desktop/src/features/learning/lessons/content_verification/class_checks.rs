//! The checks of the claim classes (claim classes design §3.1). The
//! statements of an inventory are sorted; the lesson's own are left out of
//! its claims; a Reasoning statement is read for consistency against the
//! passages its section cites and a Common one is put to the model alone,
//! and both meet the falsity veto, the knowledge judge's three samples. What
//! a lighter check cannot pass on its own goes to the strict check as
//! Sourced. Every judgment is saved as a receipt, so a resumed pass asks
//! nothing twice.
use super::claim_ledger::{sentences, Place, Rewrites};
use super::classify::{classify, rule_class, Case, ToSort};
use super::*;
use crate::features::learning::lessons::call_plan::{Planned, PlannedAt, SAMPLES};
use lattice_core::application::services::claim_verification::{
    ClassJudgment, ConsistencyOutcome, ConsistencyVerdict, KnowledgeOutcome, KnowledgeSample,
    KnowledgeVerdict, CONSISTENCY_POLICY, KNOWLEDGE_POLICY,
};
use lattice_core::domain::learning::acceptance::AcceptancePolicy;
use lattice_core::domain::learning::claim_review::ExtractedClaim;

/// What becomes of a statement once it is sorted and has met its lighter
/// check.
pub(super) enum Route {
    /// Not a claim about the world: left out of the lesson's claims.
    Left,
    /// To be checked against its sources by the strict check, as this class.
    Strict(Classed),
    /// Checked: its finding.
    Done(Box<Finding>),
}

/// Tokens a passage costs beside its text in the consistency check: its
/// label and the blank line after it.
const LABEL_TOKENS: usize = 12;

/// The class of each of `claims`, in order: that of the statement it
/// replaces when a repair wrote it, otherwise the one the ledger holds for
/// it, otherwise Sourced when the writer cited its sentence, otherwise the
/// rule layer's where it decides, otherwise the model's, which is asked once
/// per statement.
pub(super) async fn sort(
    llm: &dyn LLMPort,
    policy: &dyn AcceptancePolicy,
    candidate: &Value,
    claims: &[(usize, Claim)],
    rewrites: &Rewrites,
    citations: &Citations,
) -> Result<Vec<Classed>> {
    let extracted: Vec<ExtractedClaim> = claims
        .iter()
        .map(|(unit, claim)| ExtractedClaim {
            location: claim_ledger::locate(candidate, *unit, &claim.quote, Some(&claim.statement)),
            statement: claim.statement.clone(),
        })
        .collect();
    let fields: Vec<String> = extracted
        .iter()
        .map(|claim| claim.location.field.clone())
        .collect();
    let cases: Vec<Case> = claims
        .iter()
        .map(|(unit, _)| Case::of(candidate, *unit))
        .collect();
    let parents = rewrites.classes(candidate, &extracted);
    let mut sorted: Vec<Option<Classed>> = Vec::with_capacity(claims.len());
    for ((((unit, claim), field), case), parent) in
        claims.iter().zip(&fields).zip(&cases).zip(parents)
    {
        let rule = rule_class(&claim.statement, field, case);
        let held = rewrites.held(*unit, &claim.statement);
        sorted.push(match (parent, held, rule) {
            (Some(parent), held, rule) => {
                let parent = held
                    .filter(|held| held.class.strictness() > parent.class.strictness())
                    .unwrap_or(parent);
                let inherited = carried(parent, rule);
                // A resumed pass finds the class again without its parent.
                if rule != Some(inherited.class) {
                    save_class(llm, &claim.statement, inherited.class).await?;
                }
                Some(inherited)
            }
            (None, Some(held), rule) => Some(carried(held, rule)),
            (None, None, _) if citations.is_cited(*unit, &claim.statement) => Some(Classed {
                class: Class::Sourced,
                by: ClassifiedBy::Writer,
            }),
            (None, None, Some(class)) => Some(Classed {
                class,
                by: ClassifiedBy::Rule,
            }),
            (None, None, None) => saved_class(llm, &claim.statement)
                .await?
                .map(|class| Classed {
                    class,
                    by: ClassifiedBy::Model,
                }),
        });
    }
    {
        let mut open: Vec<(&mut Option<Classed>, &str, &str, &Case)> = Vec::new();
        for (((slot, (_, claim)), field), case) in
            sorted.iter_mut().zip(claims).zip(&fields).zip(&cases)
        {
            if slot.is_none() {
                open.push((slot, claim.statement.as_str(), field.as_str(), case));
            }
        }
        if !open.is_empty() {
            let to_sort: Vec<ToSort<'_>> = open
                .iter()
                .map(|(_, statement, field, case)| ToSort {
                    statement,
                    field,
                    case: (*case).clone(),
                })
                .collect();
            let answered = classify(llm, &to_sort).await?;
            for ((slot, statement, _, _), (class, by)) in open.into_iter().zip(answered) {
                save_class(llm, statement, class).await?;
                *slot = Some(Classed { class, by });
            }
        }
    }
    let classes = sorted
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| AppError::InternalError("A statement was left without a class".into()))?;
    plan_calls(policy, rewrites, &classes, claims, &fields);
    Ok(classes)
}

/// The class a statement a repair wrote takes: its parent's, which the rule
/// layer can only make stricter (a digit, a quotation or an attribution the
/// rewrite introduced makes it Sourced). A rewrite is held to what the
/// passage states or the same point without the specific, so it keeps the
/// nature of what it replaces, and a rule that would leave a statement out of
/// the claims does not apply to one: that would cut the check from a rewrite
/// (claim classes design §3.1).
fn carried(parent: Classed, rule: Option<Class>) -> Classed {
    match rule {
        Some(Class::Sourced) if parent.class != Class::Sourced => Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Rule,
        },
        _ => parent,
    }
}

/// The calls the statements' checks can make, now that each has a class: where
/// each stands in its class's graph, as the ledger has it, and which of them
/// stand on one sentence.
fn plan_calls(
    policy: &dyn AcceptancePolicy,
    rewrites: &Rewrites,
    classes: &[Classed],
    claims: &[(usize, Claim)],
    fields: &[String],
) {
    let counted: Vec<(Class, Place<'_>, &str)> = classes
        .iter()
        .zip(claims.iter().zip(fields))
        .filter(|(classed, _)| classed.class != Class::Internal)
        .map(|(classed, ((unit, claim), field))| {
            (
                classed.class,
                Place {
                    unit: *unit,
                    field: field.as_str(),
                    quote: claim.quote.as_str(),
                },
                claim.statement.as_str(),
            )
        })
        .collect();
    let places: Vec<Place<'_>> = counted.iter().map(|(_, place, _)| *place).collect();
    let statements: Vec<Planned> = counted
        .iter()
        .zip(sentences(&places))
        .map(|((class, place, statement), sentence)| Planned {
            class: *class,
            at: PlannedAt::of(rewrites.walked(place.unit, statement)),
            sentence,
        })
        .collect();
    let sections: HashSet<usize> = places.iter().map(|place| place.unit).collect();
    crate::features::learning::lesson_progress::calls_planned(
        policy,
        &statements,
        u32::try_from(sections.len()).unwrap_or(u32::MAX),
    );
}

/// Each of `claims` met with the check its class names, in order. A claim
/// the lighter checks cannot pass is the strict check's, as Sourced.
pub(super) async fn check(
    llm: &dyn LLMPort,
    claims: &[(usize, Claim)],
    classes: Vec<Classed>,
    citations: &Citations,
) -> Result<Vec<Route>> {
    let mut routes = Vec::with_capacity(claims.len());
    for ((unit, claim), classed) in claims.iter().zip(classes) {
        let route = match classed.class {
            Class::Internal => Route::Left,
            Class::Sourced => Route::Strict(classed),
            Class::Reasoning => reasoning(llm, *unit, claim, classed, citations).await?,
            Class::Common => common(llm, *unit, claim, classed).await?,
        };
        routes.push(route);
    }
    Ok(routes)
}

/// The class a statement the lighter checks cannot pass is checked as.
fn sourced() -> Classed {
    Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Model,
    }
}

/// A Reasoning statement: contradicted by the passages its section cites, or
/// asserting a specific they lack (which makes it Sourced), or consistent
/// with them. A consistent one is still put to the knowledge judge: a
/// majority that calls it false fails it unless the passages state it
/// outright, so the sources outrank the model.
async fn reasoning(
    llm: &dyn LLMPort,
    unit: usize,
    claim: &Claim,
    classed: Classed,
    citations: &Citations,
) -> Result<Route> {
    let shown = fitted(llm, &claim.statement, citations.of(unit));
    let Some(consistency) = consistency_of(llm, &claim.statement, &shown).await? else {
        return Ok(unjudged(unit, claim, classed, &shown));
    };
    Ok(match consistency.verdict {
        ConsistencyVerdict::UnfoundedSpecific => Route::Strict(sourced()),
        ConsistencyVerdict::Contradicted => done(
            (unit, claim, classed),
            ClaimVerdict::Contradicted,
            consistency.reason,
            &shown,
            None,
        ),
        ConsistencyVerdict::Consistent => {
            let Some(knowledge) = knowledge_of(llm, &claim.statement).await? else {
                return Ok(unjudged(unit, claim, classed, &shown));
            };
            if !knowledge.is_false() {
                return Ok(done(
                    (unit, claim, classed),
                    ClaimVerdict::Supported,
                    consistency.reason,
                    &shown,
                    None,
                ));
            }
            match stated_by(llm, &claim.statement, &shown).await? {
                Some((quote, reason)) => done(
                    (unit, claim, classed),
                    ClaimVerdict::Supported,
                    reason,
                    &shown,
                    Some(quote),
                ),
                None => done(
                    (unit, claim, classed),
                    ClaimVerdict::Contradicted,
                    vetoed(&knowledge),
                    &shown,
                    None,
                ),
            }
        }
    })
}

/// A Common statement: it passes when all three samples call it true and
/// common, fails when most call it false, and is Sourced otherwise, a lone
/// dissent forfeiting the pass without failing the statement.
async fn common(llm: &dyn LLMPort, unit: usize, claim: &Claim, classed: Classed) -> Result<Route> {
    let Some(knowledge) = knowledge_of(llm, &claim.statement).await? else {
        return Ok(unjudged(unit, claim, classed, &[]));
    };
    Ok(if knowledge.is_common() {
        done(
            (unit, claim, classed),
            ClaimVerdict::Supported,
            standard(&knowledge),
            &[],
            None,
        )
    } else if knowledge.is_false() {
        done(
            (unit, claim, classed),
            ClaimVerdict::Contradicted,
            vetoed(&knowledge),
            &[],
            None,
        )
    } else {
        if !knowledge.needs_source() {
            tracing::info!(statement = %claim.statement, "claim.class: a lone dissent sent a Common statement to its source check");
        }
        Route::Strict(sourced())
    })
}

/// The finding of a statement a lighter check decided.
fn done(
    (unit, claim, classed): (usize, &Claim, Classed),
    verdict: ClaimVerdict,
    reason: String,
    evidence: &[EvidencePassage],
    supporting_quote: Option<String>,
) -> Route {
    Route::Done(Box::new(Finding {
        unit,
        quote: claim.quote.clone(),
        statement: claim.statement.clone(),
        verdict,
        reason,
        evidence: evidence.to_vec(),
        supporting_quote,
        resolution: None,
        trust: None,
        label: None,
        class: Some(classed),
        two_pass: Default::default(),
    }))
}

/// A statement whose check's answer could not be read.
fn unjudged(unit: usize, claim: &Claim, classed: Classed, evidence: &[EvidencePassage]) -> Route {
    done(
        (unit, claim, classed),
        ClaimVerdict::Unverified,
        "The answer to this statement's check could not be read.".into(),
        evidence,
        None,
    )
}

/// Why a statement the model called false fails.
fn vetoed(knowledge: &KnowledgeOutcome) -> String {
    let says_false = |sample: &&KnowledgeSample| sample.verdict == KnowledgeVerdict::False;
    let reason = knowledge
        .samples
        .iter()
        .find(says_false)
        .map(|sample| sample.reason.as_str())
        .unwrap_or_default();
    format!(
        "Asked on its own, the model calls this false ({} of {}): {reason}",
        knowledge.samples.iter().filter(says_false).count(),
        knowledge.samples.len()
    )
}

/// Why a statement every sample called standard knowledge passes.
fn standard(knowledge: &KnowledgeOutcome) -> String {
    let reason = knowledge
        .samples
        .first()
        .map(|sample| sample.reason.as_str())
        .unwrap_or_default();
    format!("Standard knowledge: {reason}")
}

/// The passages of a section, as many as the consistency check of
/// `statement` holds beside its answer.
fn fitted(
    llm: &dyn LLMPort,
    statement: &str,
    passages: &[EvidencePassage],
) -> Vec<EvidencePassage> {
    let checker = consistency_checker(llm);
    let room = checker.consistency_evidence_room(statement);
    let mut used = 0usize;
    passages
        .iter()
        .take_while(|passage| {
            used = used.saturating_add(llm.count_tokens(&passage.text) + LABEL_TOKENS);
            used <= room
        })
        .cloned()
        .collect()
}

fn consistency_checker(llm: &dyn LLMPort) -> ClaimChecker<'_> {
    ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        CHECKER_OUTPUT_TOKENS,
        CheckPolicy::Consistency,
    )
}

/// The consistency judge's answer on `statement` and `passages`: restored
/// from its receipt or asked for. `None` when its reply could not be read.
async fn consistency_of(
    llm: &dyn LLMPort,
    statement: &str,
    passages: &[EvidencePassage],
) -> Result<Option<ConsistencyOutcome>> {
    let shown: Vec<(&String, usize, usize, &String)> = passages
        .iter()
        .map(|passage| {
            (
                &passage.source_id,
                passage.start_byte,
                passage.end_byte,
                &passage.text,
            )
        })
        .collect();
    let key = format!(
        "{CONSISTENCY_POLICY}:{}",
        digest(
            &json!({
                "model": llm.model_name(),
                "context": llm.max_context_tokens(),
                "statement": statement,
                "passages": shown,
            })
            .to_string()
        )
    );
    if let Some(saved) = crate::features::learning::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|value| serde_json::from_value::<ConsistencyOutcome>(value).ok())
    {
        return Ok(Some(saved));
    }
    let texts: Vec<String> = passages
        .iter()
        .map(|passage| passage.text.clone())
        .collect();
    let judged = {
        let _model_call = crate::features::learning::lesson_progress::model_call();
        consistency_checker(llm)
            .check_consistency(statement, &texts)
            .await
    };
    match judged {
        ClassJudgment::Judged(outcome) => {
            crate::features::learning::lesson_drafts::record_checkpoint(
                &key,
                serde_json::to_value(&outcome)?,
            )
            .await?;
            Ok(Some(outcome))
        }
        ClassJudgment::Unusable => Ok(None),
        ClassJudgment::Failed(error) => Err(error),
    }
}

/// The knowledge judge's three samples on `statement`: restored from their
/// receipt or asked for. `None` when a reply could not be read.
async fn knowledge_of(llm: &dyn LLMPort, statement: &str) -> Result<Option<KnowledgeOutcome>> {
    let key = format!(
        "{KNOWLEDGE_POLICY}:{}",
        digest(
            &json!({
                "model": llm.model_name(),
                "context": llm.max_context_tokens(),
                "statement": statement,
            })
            .to_string()
        )
    );
    if let Some(saved) = crate::features::learning::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|value| serde_json::from_value::<KnowledgeOutcome>(value).ok())
    {
        return Ok(Some(saved));
    }
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        CHECKER_OUTPUT_TOKENS,
        CheckPolicy::Knowledge,
    );
    let judged = {
        let _model_calls = crate::features::learning::lesson_progress::model_calls(SAMPLES);
        checker.check_knowledge(statement).await
    };
    match judged {
        ClassJudgment::Judged(outcome) => {
            crate::features::learning::lesson_drafts::record_checkpoint(
                &key,
                serde_json::to_value(&outcome)?,
            )
            .await?;
            Ok(Some(outcome))
        }
        ClassJudgment::Unusable => Ok(None),
        ClassJudgment::Failed(error) => Err(error),
    }
}

/// The quote with which `passages` state `statement` outright, and the
/// judge's reason, when the strict judge finds that they do.
async fn stated_by(
    llm: &dyn LLMPort,
    statement: &str,
    passages: &[EvidencePassage],
) -> Result<Option<(String, String)>> {
    if passages.is_empty() {
        return Ok(None);
    }
    let texts: Vec<String> = passages
        .iter()
        .map(|passage| passage.text.clone())
        .collect();
    let judged = judge_for_publication(llm, statement, &texts, &ranking(passages)).await;
    match judged.judgment {
        ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported => {
            let Some(quote) = outcome.quote else {
                return Ok(None);
            };
            Ok(texts
                .iter()
                .any(|text| text.contains(&quote))
                .then(|| (quote, outcome.reason.unwrap_or_default())))
        }
        ClaimJudgment::Failed(error) => Err(error),
        _ => Ok(None),
    }
}

fn class_key(llm: &dyn LLMPort, statement: &str) -> String {
    format!(
        "classes-v1:{}",
        digest(&json!({"model": llm.model_name(), "statement": statement}).to_string())
    )
}

/// The class the model gave `statement`, when it was asked before.
async fn saved_class(llm: &dyn LLMPort, statement: &str) -> Result<Option<Class>> {
    Ok(
        crate::features::learning::lesson_drafts::checkpoint(&class_key(llm, statement))
            .await?
            .and_then(|value| serde_json::from_value(value).ok()),
    )
}

async fn save_class(llm: &dyn LLMPort, statement: &str, class: Class) -> Result<()> {
    crate::features::learning::lesson_drafts::record_checkpoint(
        &class_key(llm, statement),
        serde_json::to_value(class)?,
    )
    .await
}
