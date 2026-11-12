//! What a scripted model that knows exactly some facts answers every judge.
use super::batch::{BatchReply, ClaimBatch};
use super::bridge::ClaimParts;
use super::classes::{Consistency, Knowledge};
use super::*;
use crate::application::services::model_contract::test_support::{Reply, ScriptedModel, Sent};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
enum Fact {
    Stated { quote: String },
    Contradicted { quote: String },
    Unsupported,
    Common,
    Parts(Vec<BridgePart>),
}

/// Statement text to what is true of it, for every judge to answer from.
#[derive(Debug, Clone, Default)]
pub struct Truth {
    facts: HashMap<String, Fact>,
}

impl Truth {
    /// A passage states `statement`; `quote` is the words in it that do.
    pub fn stated(self, statement: &str, passage: &str, quote: &str) -> Self {
        let quote = quoted(passage, quote);
        self.with(statement, Fact::Stated { quote })
    }

    /// A passage states the opposite of `statement`, in the words of `quote`.
    pub fn contradicted(self, statement: &str, passage: &str, quote: &str) -> Self {
        let quote = quoted(passage, quote);
        self.with(statement, Fact::Contradicted { quote })
    }

    /// No passage states `statement`, and it needs a source.
    pub fn unsupported(self, statement: &str) -> Self {
        self.with(statement, Fact::Unsupported)
    }

    /// `statement` is general knowledge no passage needs to state.
    pub fn common(self, statement: &str) -> Self {
        self.with(statement, Fact::Common)
    }

    /// The judge's part-wise reading of `statement`, as given.
    pub fn parts(self, statement: &str, parts: Vec<BridgePart>) -> Self {
        self.with(statement, Fact::Parts(parts))
    }

    fn with(mut self, statement: &str, fact: Fact) -> Self {
        let before = self.facts.insert(statement.to_owned(), fact);
        assert!(before.is_none(), "{statement:?} is in the truth twice");
        self
    }

    fn fact(&self, statement: &str) -> std::result::Result<&Fact, String> {
        self.facts
            .get(statement.trim())
            .ok_or_else(|| format!("the statement is not in the truth: {statement}"))
    }

    fn question(&self, prompt: &str, label: &str) -> std::result::Result<Question<'_>, String> {
        let (passages, statement) = shown_passages_and_statement(prompt, label)
            .ok_or_else(|| format!("the prompt holds no {label}: {prompt}"))?;
        let fact = self.fact(&statement)?;
        Ok(Question {
            fact,
            statement,
            passages,
        })
    }
}

fn quoted(passage: &str, quote: &str) -> String {
    assert!(
        quote_in(passage, quote).is_some(),
        "the quote {quote:?} is not in its passage {passage:?}"
    );
    quote.to_owned()
}

struct Question<'t> {
    fact: &'t Fact,
    statement: String,
    passages: Vec<String>,
}

impl Question<'_> {
    fn parts(&self) -> Vec<BridgePart> {
        self.fact.read(&self.statement, &self.passages)
    }
}

impl Fact {
    /// The parts a judge shown `shown` finds in `statement`: a fact is stated
    /// or contradicted only by a shown passage that holds its quote.
    fn read(&self, statement: &str, shown: &[String]) -> Vec<BridgePart> {
        let part = |kind, passage| {
            vec![BridgePart {
                part: statement.to_owned(),
                kind,
                passage,
            }]
        };
        let found = |quote: &str| {
            shown
                .iter()
                .position(|passage| quote_in(passage, quote).is_some())
        };
        match self {
            Self::Stated { quote } => found(quote).map_or_else(
                || part(PartKind::SpecificMissing, None),
                |at| part(PartKind::Stated, Some(at)),
            ),
            Self::Contradicted { quote } => found(quote).map_or_else(
                || part(PartKind::SpecificMissing, None),
                |at| part(PartKind::Contradicted, Some(at)),
            ),
            Self::Unsupported => part(PartKind::SpecificMissing, None),
            Self::Common => part(PartKind::Common, None),
            Self::Parts(parts) => parts.clone(),
        }
    }

    /// What a model that knows this fact says of it with no passage in front of it.
    fn known(&self) -> KnowledgeVerdict {
        let kinds = |parts: &[BridgePart], wanted: &[PartKind]| {
            parts.iter().any(|part| wanted.contains(&part.kind))
        };
        match self {
            Self::Stated { .. } | Self::Unsupported => KnowledgeVerdict::NeedsSource,
            Self::Contradicted { .. } => KnowledgeVerdict::False,
            Self::Common => KnowledgeVerdict::TrueCommon,
            Self::Parts(parts) if kinds(parts, &[PartKind::Contradicted]) => {
                KnowledgeVerdict::False
            }
            Self::Parts(parts)
                if !parts.is_empty()
                    && parts
                        .iter()
                        .all(|part| matches!(part.kind, PartKind::Common | PartKind::Own)) =>
            {
                KnowledgeVerdict::TrueCommon
            }
            Self::Parts(_) => KnowledgeVerdict::NeedsSource,
        }
    }
}

fn has(parts: &[BridgePart], kind: PartKind) -> bool {
    parts.iter().any(|part| part.kind == kind)
}

fn why(parts: &[BridgePart]) -> &'static str {
    if has(parts, PartKind::Contradicted) {
        "A passage states something incompatible with it."
    } else if has(parts, PartKind::SpecificMissing) {
        "No passage shown gives a specific it needs."
    } else if parts.iter().all(|part| part.kind == PartKind::Stated) {
        "A passage states it."
    } else {
        "Nothing it says is missing, but not all of it is in a passage."
    }
}

/// A page judge's verdict on the parts, and the passage it cites.
fn page(parts: &[BridgePart]) -> (ClaimVerdict, Option<usize>) {
    if let Some(part) = parts
        .iter()
        .find(|part| part.kind == PartKind::Contradicted)
    {
        (ClaimVerdict::Contradicted, part.passage)
    } else if !parts.is_empty() && parts.iter().all(|part| part.kind == PartKind::Stated) {
        (
            ClaimVerdict::Supported,
            parts.first().and_then(|part| part.passage),
        )
    } else {
        (ClaimVerdict::Unsupported, None)
    }
}

/// The passages a judge prompt shows and the statement it asks about, in the
/// layout `[passage-N]` blocks under `Source passages:` and `label: statement`.
fn shown_passages_and_statement(prompt: &str, label: &str) -> Option<(Vec<String>, String)> {
    let rest = prompt.split_once("Source passages:\n")?.1;
    let (block, after) = rest.split_once(&format!("\n\n{label}: "))?;
    let statement = after.split("\n\n").next()?.trim().to_owned();
    let passages = match block.strip_prefix("[passage-") {
        _ if block.is_empty() => Vec::new(),
        None => vec![block.to_owned()],
        Some(first) => first
            .split("\n\n[passage-")
            .filter_map(|chunk| chunk.split_once("]\n").map(|(_, text)| text.to_owned()))
            .collect(),
    };
    Some((passages, statement))
}

fn statement_of_knowledge_prompt(prompt: &str) -> Option<&str> {
    prompt
        .rsplit_once("\n\nStatement: ")
        .map(|(_, statement)| statement)
        .or_else(|| prompt.strip_prefix("Statement: "))
        .map(str::trim)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bank {
    source_passages: Vec<BankPassage>,
    claims: Vec<BankClaim>,
}

#[derive(Deserialize)]
struct BankPassage {
    id: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BankClaim {
    id: String,
    claim: String,
    evidence_ids: Vec<String>,
}

fn answer_check(truth: &Truth, sent: &Sent, input: &PromptInput) -> Reply<CheckReply> {
    if sent.request.want_logprobs {
        return Reply::Miss("a claim check that is not reasoned over located passages".into());
    }
    match truth.question(&input.prompt, "Claim") {
        Ok(question) => {
            let parts = question.parts();
            let (verdict, at) = page(&parts);
            Reply::Answer(CheckReply::on(verdict, why(&parts), at))
        }
        Err(what) => Reply::Miss(what),
    }
}

fn answer_batch(truth: &Truth, input: &PromptInput) -> Reply<BatchReply> {
    let Ok(bank) = serde_json::from_str::<Bank>(&input.prompt) else {
        return Reply::Miss(format!(
            "the batch prompt is no evidence bank: {}",
            input.prompt
        ));
    };
    let mut checks = Vec::new();
    for claim in &bank.claims {
        let fact = match truth.fact(&claim.claim) {
            Ok(fact) => fact,
            Err(what) => return Reply::Miss(what),
        };
        let shown: Vec<&BankPassage> = claim
            .evidence_ids
            .iter()
            .filter_map(|id| {
                bank.source_passages
                    .iter()
                    .find(|passage| &passage.id == id)
            })
            .collect();
        let texts: Vec<String> = shown.iter().map(|passage| passage.text.clone()).collect();
        let parts = fact.read(&claim.claim, &texts);
        let (verdict, at) = page(&parts);
        let source = at
            .and_then(|at| shown.get(at))
            .map_or_else(|| "none".to_owned(), |passage| passage.id.clone());
        let reason = why(&parts).to_owned();
        checks.push((
            claim.id.clone(),
            CheckReply {
                verdict,
                reason,
                source,
            },
        ));
    }
    Reply::Answer(BatchReply { checks })
}

fn answer_parts(truth: &Truth, input: &PromptInput) -> Reply<BridgeOutcome> {
    match truth.question(&input.prompt, "Claim") {
        Ok(question) => {
            let parts = question.parts();
            Reply::Answer(BridgeOutcome {
                reason: why(&parts).to_owned(),
                parts,
            })
        }
        Err(what) => Reply::Miss(what),
    }
}

fn answer_consistency(truth: &Truth, input: &PromptInput) -> Reply<ConsistencyOutcome> {
    match truth.question(&input.prompt, "Explanatory statement") {
        Ok(question) => {
            let parts = question.parts();
            let verdict = if has(&parts, PartKind::Contradicted) {
                ConsistencyVerdict::Contradicted
            } else if has(&parts, PartKind::SpecificMissing) {
                ConsistencyVerdict::UnfoundedSpecific
            } else {
                ConsistencyVerdict::Consistent
            };
            Reply::Answer(ConsistencyOutcome {
                verdict,
                reason: why(&parts).to_owned(),
            })
        }
        Err(what) => Reply::Miss(what),
    }
}

fn answer_knowledge(truth: &Truth, input: &PromptInput) -> Reply<KnowledgeSample> {
    let Some(statement) = statement_of_knowledge_prompt(&input.prompt) else {
        return Reply::Miss(format!("the prompt holds no statement: {}", input.prompt));
    };
    match truth.fact(statement) {
        Ok(fact) => {
            let verdict = fact.known();
            let reason = match verdict {
                KnowledgeVerdict::TrueCommon => "Standard knowledge of the field.",
                KnowledgeVerdict::False => "It is wrong as general knowledge.",
                KnowledgeVerdict::NeedsSource => "A textbook would cite a source for it.",
            };
            Reply::Answer(KnowledgeSample {
                verdict,
                reason: reason.to_owned(),
            })
        }
        Err(what) => Reply::Miss(what),
    }
}

impl ScriptedModel {
    /// A model that answers every judge from `truth`; a statement it does not hold is a miss.
    pub fn judging(truth: Truth) -> Self {
        let truth = Arc::new(truth);
        let (check, batch, parts, consistency, knowledge) = (
            Arc::clone(&truth),
            Arc::clone(&truth),
            Arc::clone(&truth),
            Arc::clone(&truth),
            truth,
        );
        Self::new()
            .on_sent::<ClaimCheck>(move |sent, input, _| answer_check(&check, sent, input))
            .on_nth::<ClaimBatch>(move |input, _| answer_batch(&batch, input))
            .on_nth::<ClaimParts>(move |input, _| answer_parts(&parts, input))
            .on_nth::<Consistency>(move |input, _| answer_consistency(&consistency, input))
            .on_nth::<Knowledge>(move |input, _| answer_knowledge(&knowledge, input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::learning::acceptance::KNOWLEDGE_SAMPLES;

    const LEXER: &str = "Quillet is a lexer shipped with the toolchain.";
    const NEVER_PARSES: &str = "Quillet never parses; it only lexes.";
    const STATED: &str = "Quillet is a lexer.";
    const CONTRADICTED: &str = "Quillet is a parser.";
    const UNSUPPORTED: &str = "Quillet lexes four languages.";
    const COMMON: &str = "A lexer turns text into tokens.";
    const PARTS: &str = "Quillet is a lexer and tokenizes.";

    fn truth() -> Truth {
        Truth::default()
            .stated(STATED, LEXER, "a lexer shipped with the toolchain")
            .contradicted(CONTRADICTED, NEVER_PARSES, "it only lexes")
            .unsupported(UNSUPPORTED)
            .common(COMMON)
            .parts(
                PARTS,
                vec![
                    BridgePart {
                        part: "Quillet is a lexer".into(),
                        kind: PartKind::Stated,
                        passage: Some(0),
                    },
                    BridgePart {
                        part: "it tokenizes".into(),
                        kind: PartKind::Common,
                        passage: None,
                    },
                ],
            )
    }

    fn shown() -> Vec<String> {
        vec![LEXER.into(), NEVER_PARSES.into()]
    }

    fn checker(model: &ScriptedModel, policy: CheckPolicy) -> ClaimChecker<'_> {
        ClaimChecker::for_policy(model, SamplingOverride::deterministic(), policy)
    }

    fn judged<T: std::fmt::Debug>(judgment: Judgment<T>) -> T {
        match judgment {
            Judgment::Judged(value) => value,
            other => panic!("not judged: {other:?}"),
        }
    }

    async fn bridged(model: &ScriptedModel, statement: &str, shown: &[String]) -> Vec<BridgePart> {
        let answered = checker(model, CheckPolicy::Bridge)
            .check_bridge(statement, shown)
            .await;
        judged(answered.judgments).parts
    }

    #[tokio::test]
    async fn the_part_wise_judge_reads_every_kind_of_fact() {
        let model = ScriptedModel::judging(truth());
        let kinds = |parts: Vec<BridgePart>| -> Vec<(PartKind, Option<usize>)> {
            parts.iter().map(|part| (part.kind, part.passage)).collect()
        };
        let cases = [
            (STATED, vec![(PartKind::Stated, Some(0))]),
            (CONTRADICTED, vec![(PartKind::Contradicted, Some(1))]),
            (UNSUPPORTED, vec![(PartKind::SpecificMissing, None)]),
            (COMMON, vec![(PartKind::Common, None)]),
            (
                PARTS,
                vec![(PartKind::Stated, Some(0)), (PartKind::Common, None)],
            ),
        ];
        for (statement, expected) in cases {
            let parts = bridged(&model, statement, &shown()).await;
            assert_eq!(kinds(parts), expected, "{statement}");
        }
        let unshown = bridged(&model, STATED, &[NEVER_PARSES.into()]).await;
        assert_eq!(kinds(unshown), [(PartKind::SpecificMissing, None)]);
    }

    #[tokio::test]
    async fn the_page_judge_reads_every_kind_of_fact() {
        let model = ScriptedModel::judging(truth());
        let strict = checker(&model, CheckPolicy::Strict);
        let cases = [
            (STATED, ClaimVerdict::Supported, Some(LEXER)),
            (CONTRADICTED, ClaimVerdict::Contradicted, Some(NEVER_PARSES)),
            (UNSUPPORTED, ClaimVerdict::Unsupported, None),
            (COMMON, ClaimVerdict::Unsupported, None),
            (PARTS, ClaimVerdict::Unsupported, None),
        ];
        for (statement, verdict, quote) in cases {
            let outcome = judged(
                strict
                    .check_passages_without_deadline(statement, &shown())
                    .await,
            );
            assert_eq!(outcome.verdict, verdict, "{statement}");
            assert_eq!(outcome.quote.as_deref(), quote, "{statement}");
        }
        let unshown = strict
            .check_passages_without_deadline(STATED, &[NEVER_PARSES.into()])
            .await;
        assert_eq!(judged(unshown).verdict, ClaimVerdict::Unsupported);
    }

    #[tokio::test]
    async fn a_batch_judges_each_claim_on_its_own_passages() {
        let model = ScriptedModel::judging(truth());
        let claim = |claim: &str, passages: &[&str]| LocatedClaim {
            claim: claim.into(),
            passages: passages.iter().map(|text| (*text).to_owned()).collect(),
        };
        let claims = [
            claim(STATED, &[LEXER]),
            claim(CONTRADICTED, &[LEXER, NEVER_PARSES]),
            claim(UNSUPPORTED, &[LEXER]),
            claim(STATED, &[NEVER_PARSES]),
        ];
        let answered = checker(&model, CheckPolicy::Strict)
            .check_batch_without_deadline(&claims, &|_| Ok(()), &|_| Ok(()))
            .await
            .unwrap();
        let verdicts: Vec<_> = answered
            .judgments
            .into_iter()
            .map(|judgment| judged(judgment).verdict)
            .collect();
        assert_eq!(
            verdicts,
            [
                ClaimVerdict::Supported,
                ClaimVerdict::Contradicted,
                ClaimVerdict::Unsupported,
                ClaimVerdict::Unsupported,
            ]
        );
    }

    #[tokio::test]
    async fn the_consistency_judge_reads_every_kind_of_fact() {
        let model = ScriptedModel::judging(truth());
        let consistency = checker(&model, CheckPolicy::Consistency);
        let cases = [
            (STATED, ConsistencyVerdict::Consistent),
            (CONTRADICTED, ConsistencyVerdict::Contradicted),
            (UNSUPPORTED, ConsistencyVerdict::UnfoundedSpecific),
            (COMMON, ConsistencyVerdict::Consistent),
            (PARTS, ConsistencyVerdict::Consistent),
        ];
        for (statement, verdict) in cases {
            let outcome = judged(consistency.check_consistency(statement, &shown()).await);
            assert_eq!(outcome.verdict, verdict, "{statement}");
        }
        let unshown = consistency
            .check_consistency(STATED, &[NEVER_PARSES.into()])
            .await;
        assert_eq!(
            judged(unshown).verdict,
            ConsistencyVerdict::UnfoundedSpecific
        );
    }

    #[tokio::test]
    async fn the_knowledge_judge_reads_every_kind_of_fact() {
        let model = ScriptedModel::judging(truth());
        let knowledge = checker(&model, CheckPolicy::Knowledge);
        let cases = [
            (STATED, KnowledgeVerdict::NeedsSource),
            (CONTRADICTED, KnowledgeVerdict::False),
            (UNSUPPORTED, KnowledgeVerdict::NeedsSource),
            (COMMON, KnowledgeVerdict::TrueCommon),
            (PARTS, KnowledgeVerdict::NeedsSource),
        ];
        for (statement, verdict) in cases {
            let outcome = judged(knowledge.check_knowledge(statement, None).await);
            assert_eq!(outcome.samples.len(), KNOWLEDGE_SAMPLES as usize);
            assert!(
                outcome
                    .samples
                    .iter()
                    .all(|sample| sample.verdict == verdict),
                "{statement}"
            );
        }
        let with_case = knowledge
            .check_knowledge(COMMON, Some("A worked case."))
            .await;
        assert!(judged(with_case).is_common());
    }

    #[tokio::test]
    #[should_panic(expected = "nothing answers")]
    async fn a_statement_outside_the_truth_fails_the_test() {
        let model = ScriptedModel::judging(truth());
        let answered = checker(&model, CheckPolicy::Bridge)
            .check_bridge("Nobody said this.", &shown())
            .await;
        assert!(matches!(answered.judgments, Judgment::Failed(_)));
    }

    #[test]
    #[should_panic(expected = "is not in its passage")]
    fn a_quote_the_passage_does_not_hold_is_refused() {
        let _ = Truth::default().stated(STATED, LEXER, "a parser");
    }
}
