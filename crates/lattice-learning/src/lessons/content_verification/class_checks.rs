//! The statements of a lesson sorted into the claim classes (claim classes
//! design §3.1) and the judges the readings ask: the consistency judge on the
//! passages a section cites, the knowledge judge's three samples. What each
//! statement is asked next is the acceptance policy's; the readings in
//! `readings` ask these judges along its graphs. Every judgment is saved as a
//! receipt, so a resumed pass asks nothing twice.
use super::classify::{classify, ruling, saved_class, Case, ToSort};
use super::*;
use lattice_core::application::services::claim_verification::{
    ClassJudgment, ConsistencyOutcome, KnowledgeOutcome, KnowledgeSample, KnowledgeVerdict,
    CONSISTENCY_POLICY, KNOWLEDGE_POLICY,
};
use lattice_core::domain::learning::acceptance::KNOWLEDGE_SAMPLES;

/// The class of each of `claims`, in order: the one `given` holds for it,
/// which the match of the pass gave (that of the claim it continues, or of the
/// parent of the repair it is the heir of), otherwise Sourced when the writer
/// cited its sentence, otherwise the rule layer's where it decides, otherwise
/// the model's, which is asked once per statement. A statement is sorted once
/// in its lineage, so only a statement new to the ledger is sorted.
pub(super) async fn sort(
    llm: &dyn LLMPort,
    candidate: &Value,
    claims: &[(usize, Claim)],
    given: &[Option<Classed>],
    citations: &Citations,
) -> Result<Vec<Classed>> {
    let fields: Vec<String> = claims
        .iter()
        .map(|(unit, claim)| {
            claim_ledger::locate(candidate, *unit, &claim.quote, Some(&claim.statement)).field
        })
        .collect();
    let cases: Vec<Case> = claims
        .iter()
        .map(|(unit, _)| Case::of(candidate, *unit))
        .collect();
    let mut sorted: Vec<Option<Classed>> = Vec::with_capacity(claims.len());
    for (index, (((unit, claim), field), case)) in
        claims.iter().zip(&fields).zip(&cases).enumerate()
    {
        let held = given.get(index).copied().flatten();
        sorted.push(match held {
            Some(held) => Some(held),
            None if citations.is_cited(*unit, &claim.quote) => Some(Classed {
                class: Class::Sourced,
                by: ClassifiedBy::Writer,
            }),
            None => {
                let ruling = ruling(&claim.statement, field, case);
                match ruling.decided() {
                    Some(class) => Some(Classed {
                        class,
                        by: ClassifiedBy::Rule,
                    }),
                    None => saved_class(llm, &claim.statement)
                        .await?
                        .map(|class| Classed {
                            class: ruling.settle(class),
                            by: ClassifiedBy::Model,
                        }),
                }
            }
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
            for ((slot, _, _, _), (class, by)) in open.into_iter().zip(answered) {
                *slot = Some(Classed { class, by });
            }
        }
    }
    let classes = sorted
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| AppError::InternalError("A statement was left without a class".into()))?;
    Ok(classes)
}

/// The finding of a statement a reading decided.
pub(super) fn finding_of(
    (unit, claim, classed): (usize, &Claim, Classed),
    verdict: ClaimVerdict,
    reason: String,
    evidence: &[EvidencePassage],
    supporting_quote: Option<String>,
) -> Finding {
    Finding {
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
        claim_id: None,
        two_pass: Default::default(),
    }
}

/// The finding of a statement whose check's answer could not be read.
pub(super) fn unjudged_finding(
    unit: usize,
    claim: &Claim,
    classed: Classed,
    evidence: &[EvidencePassage],
) -> Finding {
    finding_of(
        (unit, claim, classed),
        ClaimVerdict::Unverified,
        "The answer to this statement's check could not be read.".into(),
        evidence,
        None,
    )
}

/// Why a statement the model called false fails, `with_case` when it was
/// asked with the case it comes from.
pub(super) fn vetoed(knowledge: &KnowledgeOutcome, with_case: bool) -> String {
    let says_false = |sample: &&KnowledgeSample| sample.verdict == KnowledgeVerdict::False;
    let reason = knowledge
        .samples
        .iter()
        .find(says_false)
        .map(|sample| sample.reason.as_str())
        .unwrap_or_default();
    let asked = if with_case {
        "Asked with only the case it comes from"
    } else {
        "Asked on its own"
    };
    format!(
        "{asked}, the model calls this false ({} of {}): {reason}",
        knowledge.samples.iter().filter(says_false).count(),
        knowledge.samples.len()
    )
}

/// Why a statement every sample called standard knowledge passes.
pub(super) fn standard(knowledge: &KnowledgeOutcome) -> String {
    let reason = knowledge
        .samples
        .first()
        .map(|sample| sample.reason.as_str())
        .unwrap_or_default();
    format!("Standard knowledge: {reason}")
}

pub(super) fn consistency_checker(llm: &dyn LLMPort) -> ClaimChecker<'_> {
    ClaimChecker::for_policy(
        llm,
        SamplingOverride::deterministic(),
        CheckPolicy::Consistency,
    )
}

/// Where the consistency judge's answer on `statement` and `passages` is saved.
pub(super) fn consistency_key(
    llm: &dyn LLMPort,
    statement: &str,
    passages: &[EvidencePassage],
) -> String {
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
    format!(
        "{CONSISTENCY_POLICY}:{}",
        sha256_hex(
            json!({
                "model": llm.model_name(),
                "context": llm.max_context_tokens(),
                "statement": statement,
                "passages": shown,
            })
            .to_string()
        )
    )
}

/// The consistency judge's answer saved under `key`, when it was asked before.
pub(super) async fn saved_consistency(key: &str) -> Result<Option<ConsistencyOutcome>> {
    Ok(crate::lesson_drafts::checkpoint(key)
        .await?
        .and_then(|value| serde_json::from_value::<ConsistencyOutcome>(value).ok()))
}

/// The consistency judge's answer on `statement` and `passages`, asked for and
/// saved under `key`. `None` when its reply could not be read.
pub(super) async fn consistency_of(
    llm: &dyn LLMPort,
    key: &str,
    statement: &str,
    passages: &[EvidencePassage],
) -> Result<Option<ConsistencyOutcome>> {
    let texts: Vec<String> = passages
        .iter()
        .map(|passage| passage.text.clone())
        .collect();
    let judged = {
        let _model_call = crate::lesson_progress::model_call(crate::lesson_progress::Term::Reading);
        consistency_checker(llm)
            .check_consistency(statement, &texts)
            .await
    };
    match judged {
        ClassJudgment::Judged(outcome) => {
            crate::lesson_drafts::record_checkpoint(key, serde_json::to_value(&outcome)?).await?;
            Ok(Some(outcome))
        }
        ClassJudgment::Unusable => Ok(None),
        ClassJudgment::Failed(error) => Err(error),
    }
}

/// Where the knowledge judge's three samples on `statement`, asked with
/// `case`, are saved.
pub(super) fn knowledge_key(llm: &dyn LLMPort, statement: &str, case: Option<&str>) -> String {
    format!(
        "{KNOWLEDGE_POLICY}:{}",
        sha256_hex(
            json!({
                "model": llm.model_name(),
                "context": llm.max_context_tokens(),
                "statement": statement,
                "case": case,
            })
            .to_string()
        )
    )
}

/// The knowledge judge's three samples saved under `key`, when they were asked
/// before.
pub(super) async fn saved_knowledge(key: &str) -> Result<Option<KnowledgeOutcome>> {
    Ok(crate::lesson_drafts::checkpoint(key)
        .await?
        .and_then(|value| serde_json::from_value::<KnowledgeOutcome>(value).ok()))
}

/// The knowledge judge's three samples on `statement` and its `case`, asked
/// for and saved under `key`. `None` when a reply could not be read.
pub(super) async fn knowledge_of(
    llm: &dyn LLMPort,
    key: &str,
    statement: &str,
    case: Option<&str>,
) -> Result<Option<KnowledgeOutcome>> {
    let checker = ClaimChecker::for_policy(
        llm,
        SamplingOverride::deterministic(),
        CheckPolicy::Knowledge,
    );
    let judged = {
        let _model_calls = crate::lesson_progress::model_calls(
            KNOWLEDGE_SAMPLES,
            crate::lesson_progress::Term::Reading,
        );
        checker.check_knowledge(statement, case).await
    };
    match judged {
        ClassJudgment::Judged(outcome) => {
            crate::lesson_drafts::record_checkpoint(key, serde_json::to_value(&outcome)?).await?;
            Ok(Some(outcome))
        }
        ClassJudgment::Unusable => Ok(None),
        ClassJudgment::Failed(error) => Err(error),
    }
}
