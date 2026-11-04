//! `Knowledge` and `Veto`: the knowledge judge's closed-book samples, read by
//! one of two rules. Knowledge passes a statement on unanimity; the veto fails
//! it only on a majority that calls it false.
use super::super::class_checks::{finding_of, knowledge_of, standard, unjudged_finding, vetoed};
use super::*;
use lattice_core::application::services::claim_verification::{KnowledgeOutcome, KnowledgeVerdict};

/// How the samples are read.
#[derive(Clone, Copy)]
enum Rule {
    Knowledge,
    Veto,
}

pub(super) struct Knowledge {
    rule: Rule,
}

impl Knowledge {
    pub(super) fn unanimous() -> Self {
        Self {
            rule: Rule::Knowledge,
        }
    }

    pub(super) fn veto() -> Self {
        Self { rule: Rule::Veto }
    }
}

#[async_trait]
impl Reader for Knowledge {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        match self.rule {
            Rule::Knowledge => Reading::Knowledge,
            Rule::Veto => Reading::Veto,
        }
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut reads = Vec::with_capacity(statements.len());
        for statement in statements {
            let Statement {
                unit,
                claim,
                classed,
            } = *statement;
            let read = match knowledge_of(at.llm, &claim.statement).await? {
                None => Read {
                    outcome: Outcome::Unjudged,
                    finding: unjudged_finding(unit, claim, classed, &[]),
                    reused: false,
                },
                Some((knowledge, reused)) => {
                    let outcome = match self.rule {
                        Rule::Knowledge => knowledge.knowledge_outcome(),
                        Rule::Veto => knowledge.veto_outcome(),
                    };
                    let (verdict, reason) = match (outcome, self.rule) {
                        (Outcome::Refuted, _) => (ClaimVerdict::Contradicted, vetoed(&knowledge)),
                        (Outcome::Stands, Rule::Knowledge) => {
                            (ClaimVerdict::Supported, standard(&knowledge))
                        }
                        (Outcome::Stands, Rule::Veto) => {
                            (ClaimVerdict::Supported, not_false(&knowledge))
                        }
                        _ => (ClaimVerdict::Unsupported, not_common(&knowledge)),
                    };
                    Read {
                        outcome,
                        finding: finding_of((unit, claim, classed), verdict, reason, &[], None),
                        reused,
                    }
                }
            };
            reads.push(read);
        }
        Ok(reads)
    }
}

/// How many samples gave `verdict`.
fn count(knowledge: &KnowledgeOutcome, verdict: KnowledgeVerdict) -> usize {
    knowledge
        .samples
        .iter()
        .filter(|sample| sample.verdict == verdict)
        .count()
}

/// Why a statement the model does not call false passes the veto.
fn not_false(knowledge: &KnowledgeOutcome) -> String {
    format!(
        "Asked on its own, the model does not call this false ({} of {}).",
        count(knowledge, KnowledgeVerdict::False),
        knowledge.samples.len()
    )
}

/// Why a statement the model will not call standard knowledge needs a source.
fn not_common(knowledge: &KnowledgeOutcome) -> String {
    let reason = knowledge
        .samples
        .iter()
        .find(|sample| sample.verdict != KnowledgeVerdict::TrueCommon)
        .map(|sample| sample.reason.as_str())
        .unwrap_or_default();
    format!(
        "Asked on its own, the model does not call this standard knowledge ({} of {}): {reason}",
        count(knowledge, KnowledgeVerdict::TrueCommon),
        knowledge.samples.len()
    )
}
