//! `Knowledge` and `Veto`: the knowledge judge's closed-book samples, read by
//! one of two rules. Knowledge passes a statement on unanimity; the veto fails
//! it only when every sample calls it false. Every receipt of a batch is
//! restored before its first call.
use super::super::class_checks::{
    finding_of, knowledge_key, knowledge_of, saved_knowledge, standard, unjudged_finding, vetoed,
};
use super::super::classify::case_text;
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
        let mut made = Made::new(statements.len());
        let mut asks = Vec::new();
        for (ordinal, statement) in statements.iter().enumerate() {
            let case = case_text(at.candidate, statement.unit, &statement.claim.quote);
            let key = knowledge_key(at.llm, &statement.claim.statement, case.as_deref());
            match saved_knowledge(&key).await? {
                Some(saved) => {
                    let read = self.read_of(statement, Some((saved, true)), case.is_some());
                    made.put(at, ordinal, read);
                }
                None => asks.push((ordinal, *statement, key, case)),
            }
        }
        let mut asked: futures::stream::FuturesUnordered<_> = asks
            .into_iter()
            .map(|(ordinal, statement, key, case)| async move {
                let judged =
                    knowledge_of(at.llm, &key, &statement.claim.statement, case.as_deref()).await?;
                Ok::<_, AppError>((ordinal, statement, judged, case.is_some()))
            })
            .collect();
        while let Some(asked) = futures::StreamExt::next(&mut asked).await {
            let (ordinal, statement, judged, with_case) = asked?;
            let read = self.read_of(
                &statement,
                judged.map(|knowledge| (knowledge, false)),
                with_case,
            );
            made.put(at, ordinal, read);
        }
        made.all()
    }
}

impl Knowledge {
    /// The reading of `statement` that the knowledge judge's samples make by
    /// this rule; `None` when a reply could not be read. `with_case` says the
    /// samples were asked with the case the statement comes from.
    fn read_of(
        &self,
        statement: &Statement<'_>,
        judged: Option<(KnowledgeOutcome, bool)>,
        with_case: bool,
    ) -> Read {
        let Statement {
            unit,
            claim,
            classed,
        } = *statement;
        match judged {
            None => Read::new(
                Outcome::Unjudged,
                unjudged_finding(unit, claim, classed, &[]),
                false,
            ),
            Some((knowledge, reused)) => {
                let outcome = match self.rule {
                    Rule::Knowledge => knowledge.knowledge_outcome(),
                    Rule::Veto => knowledge.veto_outcome(),
                };
                let (verdict, reason) = match (outcome, self.rule) {
                    (Outcome::Refuted, _) => {
                        (ClaimVerdict::Contradicted, vetoed(&knowledge, with_case))
                    }
                    (Outcome::Stands, Rule::Knowledge) => {
                        (ClaimVerdict::Supported, standard(&knowledge))
                    }
                    (Outcome::Stands, Rule::Veto) => {
                        (ClaimVerdict::Supported, not_false(&knowledge))
                    }
                    _ => (ClaimVerdict::Unsupported, not_common(&knowledge)),
                };
                Read::new(
                    outcome,
                    finding_of((unit, claim, classed), verdict, reason, &[], None),
                    reused,
                )
            }
        }
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
