//! A statement's check against its own citation, part by part (claim classes
//! design §3.3). A statement against the passages its sentence cites is read
//! by the part-wise judge in the one call its check costs: one whose source
//! states what is specific in it and leaves its common mechanism unsaid
//! passes, and one that carries a specific no passage gives does not. The judge
//! reads the parts; what may count as common knowledge is guarded here, by the
//! rule layer. A check against a page retrieved from the course, or one
//! research found, stays the plain strict judge, which finds fault with a
//! study quoted against an off-subject page less readily.
use super::super::classify::states_a_specific;
use super::requests::{finish, labelled, unlabelled};
use super::*;
use lattice_core::application::services::claim_verification::{
    BridgeOutcome, ClassJudgment, Outcome, PartKind,
};

/// Whether `pending` is read part by part: it is a comparison with the
/// statement's own citation and a source's passage is among those it cites (an
/// example's run is no source).
pub(in crate::lessons::content_verification) fn applies(pending: &PendingClaim) -> bool {
    pending.pass == Pass::Attribution && pending.evidence.iter().any(|passage| !passage.is_run())
}

/// `pending` judged part by part, with what the answer cost, a common part
/// that names a specific counted as one by [`states_a_specific`].
pub(super) async fn individual(
    llm: &dyn LLMPort,
    pending: PendingClaim,
    labels: &Labels,
) -> Result<((Pass, CheckedClaim), AnswerSize)> {
    part_wise(llm, pending, labels, states_a_specific).await
}

/// The part-wise judge, as it is asked and as a page is fitted to it.
pub(in crate::lessons::content_verification) fn bridge_checker(
    llm: &dyn LLMPort,
) -> ClaimChecker<'_> {
    ClaimChecker::for_policy(llm, SamplingOverride::deterministic(), CheckPolicy::Bridge)
}

/// [`individual`], with `names_a_specific` deciding which of the parts the
/// judge called common name a specific.
pub(in crate::lessons::content_verification) async fn part_wise(
    llm: &dyn LLMPort,
    pending: PendingClaim,
    labels: &Labels,
    names_a_specific: impl Fn(&str) -> bool,
) -> Result<((Pass, CheckedClaim), AnswerSize)> {
    let texts = labelled(&pending, labels);
    let answered = {
        let _model_call = crate::lesson_progress::model_call(crate::lesson_progress::Term::Reading);
        bridge_checker(llm)
            .check_bridge(&pending.checked, &texts)
            .await
    };
    let answer = AnswerSize::of(answered.output_tokens, 1);
    let (judgment, parts) = match answered.judgments {
        ClassJudgment::Judged(outcome) => weigh(outcome, &pending, &texts, names_a_specific),
        ClassJudgment::Failed(error) => (ClaimJudgment::Failed(error), Vec::new()),
        ClassJudgment::Unusable => (ClaimJudgment::Unusable, Vec::new()),
    };
    let judgment = unlabelled(judgment, &pending, labels);
    let pass = pending.pass;
    Ok((
        (pass, finish(pending, judgment, None, parts).await?),
        answer,
    ))
}

/// What the judge's parts come to for the statement: the strict judge's
/// judgment of it, with the parts as the finding records them. `texts` are the
/// passages as the judge was shown them; `names_a_specific` guards what may
/// count as common.
pub(super) fn weigh(
    outcome: BridgeOutcome,
    pending: &PendingClaim,
    texts: &[String],
    names_a_specific: impl Fn(&str) -> bool,
) -> (ClaimJudgment, Vec<CheckedPart>) {
    let (outcome, smuggled) = outcome.guarded(names_a_specific);
    let parts: Vec<CheckedPart> = outcome
        .parts
        .iter()
        .map(|part| CheckedPart {
            part: part.part.clone(),
            kind: part.kind,
            source_id: part
                .passage
                .and_then(|at| pending.evidence.get(at))
                .map(|passage| passage.source_id.clone()),
        })
        .collect();
    let of_kind = |kind: PartKind| -> Vec<&str> {
        outcome
            .parts
            .iter()
            .filter(|part| part.kind == kind)
            .map(|part| part.part.as_str())
            .collect()
    };
    // The passage a part of `kind` rests on, as the judge was shown it.
    let cited = |kind: PartKind| {
        outcome
            .parts
            .iter()
            .filter(|part| part.kind == kind)
            .find_map(|part| part.passage.and_then(|at| texts.get(at)))
            .cloned()
    };
    let said = outcome.reason.trim();
    let (verdict, quote, reason) = match outcome.outcome() {
        Outcome::Stands => {
            let common = of_kind(PartKind::Common);
            let own = of_kind(PartKind::Own);
            let mut reason = said.to_owned();
            if !common.is_empty() {
                reason = format!(
                    "{reason} Common knowledge, not in the passages: {}.",
                    common.join("; ")
                );
            }
            if !own.is_empty() {
                reason = format!(
                    "{reason} The lesson's own figures, not in the passages: {}.",
                    own.join("; ")
                );
            }
            (ClaimVerdict::Supported, cited(PartKind::Stated), reason)
        }
        Outcome::Refuted => (
            ClaimVerdict::Contradicted,
            cited(PartKind::Contradicted),
            if said.is_empty() {
                "A passage says otherwise.".to_owned()
            } else {
                said.to_owned()
            },
        ),
        _ => {
            let missing = of_kind(PartKind::SpecificMissing);
            let mut reason = said.to_owned();
            if !missing.is_empty() {
                reason = format!(
                    "{reason} Not given by the passages: {}.",
                    missing.join("; ")
                );
            } else if !parts.iter().any(|part| part.kind == PartKind::Stated) {
                reason = format!("{reason} No passage states any part of it.");
            }
            if !smuggled.is_empty() {
                reason = format!(
                    "{reason} Taken as specific, not as common knowledge: {}.",
                    smuggled.join("; ")
                );
            }
            (ClaimVerdict::Unsupported, None, reason)
        }
    };
    (
        ClaimJudgment::Judged(JudgeOutcome {
            verdict,
            quote,
            reason: Some(reason.trim().to_owned()),
            confidence: None,
            uncertain: false,
        }),
        parts,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
    use super::*;
    use crate::reference_collection::PassageKind;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use serde_json::{json, Value};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    const CITED: &str = "Tesseract is an OCR engine that ships with the toolchain.";

    /// Answers each request with the next reply it was given.
    struct Said(Mutex<VecDeque<String>>);

    #[async_trait::async_trait]
    impl LLMPort for Said {
        async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse::from_text(
                self.0.lock().unwrap().pop_front().unwrap_or_default(),
            ))
        }
        fn model_name(&self) -> &str {
            "said-fixture"
        }
        fn max_context_tokens(&self) -> usize {
            8_192
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    fn pending(statement: &str) -> PendingClaim {
        PendingClaim {
            ordinal: 0,
            key: String::new(),
            receipt_key: String::new(),
            unit: 0,
            claim: Claim {
                quote: statement.into(),
                statement: statement.into(),
            },
            checked: statement.into(),
            evidence: vec![EvidencePassage {
                source_id: "cited".into(),
                text: CITED.into(),
                start_byte: 0,
                end_byte: CITED.len(),
                retrieval_kind: "lexical".into(),
                score: 1.0,
            }],
            pass: Pass::Attribution,
        }
    }

    fn stated(part: &str) -> Value {
        json!({"part": part, "kind": "stated", "passage": "passage-0"})
    }

    fn unplaced(part: &str, kind: &str) -> Value {
        json!({"part": part, "kind": kind, "passage": null})
    }

    /// The judgment and the recorded parts for `statement` when the judge
    /// answers with `parts`, whatever verdict it writes beside them.
    async fn weighed(statement: &str, parts: Value) -> (JudgeOutcome, Vec<CheckedPart>) {
        let reply =
            json!({"parts": parts, "verdict": "supported", "reason": "The judge's reason."})
                .to_string();
        let model = Said(Mutex::new(VecDeque::from([reply])));
        let texts = vec![CITED.to_owned()];
        let checker = ClaimChecker::for_policy(
            &model,
            SamplingOverride::deterministic(),
            CheckPolicy::Bridge,
        );
        let ClassJudgment::Judged(outcome) =
            checker.check_bridge(statement, &texts).await.judgments
        else {
            panic!("the reply was not read");
        };
        match weigh(outcome, &pending(statement), &texts, states_a_specific) {
            (ClaimJudgment::Judged(judged), parts) => (judged, parts),
            (other, _) => panic!("{other:?}"),
        }
    }

    fn kinds(parts: &[CheckedPart]) -> Vec<PartKind> {
        parts.iter().map(|part| part.kind).collect()
    }

    #[tokio::test]
    async fn a_part_of_the_lesson_s_own_is_named_in_the_reason() {
        let statement = "A rule set of 48 checks has 12 that fail, which is 25%.";
        let reply = json!({
            "parts": [
                stated("A rule set holds checks"),
                unplaced("12 of 48 is 25%", "own"),
            ],
            "verdict": "supported",
            "reason": "The judge's reason.",
        })
        .to_string();
        let model = Said(Mutex::new(VecDeque::from([reply.clone(), reply])));
        let texts = vec![CITED.to_owned()];
        let checker = ClaimChecker::for_policy(
            &model,
            SamplingOverride::deterministic(),
            CheckPolicy::Bridge,
        );
        let answered = checker.check_bridge(statement, &texts).await.judgments;
        assert!(matches!(answered, ClassJudgment::Judged(_)), "{answered:?}");
        let ClassJudgment::Judged(outcome) = answered else {
            unreachable!()
        };
        let (judgment, _) = weigh(outcome, &pending(statement), &texts, states_a_specific);
        let judged = match judgment {
            ClaimJudgment::Judged(judged) => judged,
            other => panic!("{other:?}"),
        };
        assert_eq!(judged.verdict, ClaimVerdict::Supported);
        assert!(judged
            .reason
            .unwrap()
            .contains("The lesson's own figures, not in the passages: 12 of 48 is 25%."),);
    }

    /// The five planted rows of design 4.5, as the model answered them, through
    /// the reading, the guard and the decision.
    #[tokio::test]
    async fn the_planted_rows_are_decided_as_the_design_measured_them() {
        // An invented figure: nothing in a passage gives it.
        let (judged, parts) = weighed(
            "The study found that OCR-based RAG improved retrieval recall by 23% over the vision-based pipeline on degraded documents.",
            json!([unplaced("OCR-based RAG improved retrieval recall by 23% over the vision-based pipeline on degraded documents", "specific_missing")]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Unsupported);
        assert_eq!(judged.quote, None);
        assert_eq!(kinds(&parts), vec![PartKind::SpecificMissing]);
        assert!(judged.reason.unwrap().contains("Not given by the passages"));

        // An invented attribution a passage contradicts.
        let (judged, _) = weighed(
            "The survey reports that modular document parsing pipelines were introduced by Google in 2019.",
            json!([{"part": "modular document parsing pipelines were introduced by Google in 2019", "kind": "contradicted", "passage": "passage-0"}]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Contradicted);
        assert_eq!(judged.quote.as_deref(), Some(CITED));

        // Two invented dates.
        let (judged, parts) = weighed(
            "Vision-language models were first proposed in 2017 and reached human parity on document parsing in 2021.",
            json!([
                unplaced("first proposed in 2017", "specific_missing"),
                unplaced("reached human parity on document parsing in 2021", "specific_missing"),
            ]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Unsupported);
        assert_eq!(parts.len(), 2);

        // An invented specific beside a true common part sinks the whole claim.
        let (judged, _) = weighed(
            "Tesseract is an OCR engine written by Microsoft in 2019 that reads pixels into text.",
            json!([
                stated("Tesseract is an OCR engine"),
                {"part": "written by Microsoft", "kind": "contradicted", "passage": "passage-0"},
                {"part": "in 2019", "kind": "contradicted", "passage": "passage-0"},
                unplaced("reads pixels into text", "common"),
            ]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Contradicted);

        // The stated part and a common mechanism: supported, with the common
        // part on the record.
        let (judged, parts) = weighed(
            "Tesseract is an OCR engine, and OCR turns the pixels of a page image into text.",
            json!([
                stated("Tesseract is an OCR engine"),
                unplaced("OCR turns the pixels of a page image into text", "common"),
            ]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Supported);
        assert_eq!(judged.quote.as_deref(), Some(CITED));
        assert!(judged
            .reason
            .unwrap()
            .contains("Common knowledge, not in the passages: OCR turns the pixels of a page image into text."));
        assert_eq!(
            parts,
            vec![
                CheckedPart {
                    part: "Tesseract is an OCR engine".into(),
                    kind: PartKind::Stated,
                    source_id: Some("cited".into()),
                },
                CheckedPart {
                    part: "OCR turns the pixels of a page image into text".into(),
                    kind: PartKind::Common,
                    source_id: None,
                },
            ]
        );
    }

    #[tokio::test]
    async fn a_common_part_that_names_a_figure_or_a_quotation_is_a_specific() {
        for smuggled in [
            "it was released in 2006",
            "it is known as \"the page that reads text from images\"",
        ] {
            let (judged, parts) = weighed(
                "Tesseract is an OCR engine and a specific.",
                json!([
                    stated("Tesseract is an OCR engine"),
                    unplaced(smuggled, "common")
                ]),
            )
            .await;
            assert_eq!(judged.verdict, ClaimVerdict::Unsupported, "{smuggled}");
            assert_eq!(
                kinds(&parts),
                vec![PartKind::Stated, PartKind::SpecificMissing],
                "{smuggled}"
            );
            let reason = judged.reason.unwrap();
            assert!(
                reason.contains(&format!(
                    "Taken as specific, not as common knowledge: {smuggled}."
                )),
                "{reason}"
            );
        }
        // A mechanism in general terms is left as the judge called it.
        let (judged, parts) = weighed(
            "Tesseract is an OCR engine that reads pixels into text.",
            json!([
                stated("Tesseract is an OCR engine"),
                unplaced("OCR reads pixels into text", "common")
            ]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Supported);
        assert_eq!(kinds(&parts), vec![PartKind::Stated, PartKind::Common]);
    }

    #[tokio::test]
    async fn a_common_part_that_only_names_the_documentation_is_left_as_the_judge_called_it() {
        let (judged, parts) = weighed(
            "Tesseract is an OCR engine and the documentation is available from its help screen.",
            json!([
                stated("Tesseract is an OCR engine"),
                unplaced(
                    "the documentation is available from its help screen",
                    "common"
                )
            ]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Supported);
        assert_eq!(kinds(&parts), vec![PartKind::Stated, PartKind::Common]);
    }

    #[tokio::test]
    async fn nothing_stated_is_nothing_supported_whatever_is_common() {
        let (judged, _) = weighed(
            "OCR turns the pixels of a page image into text.",
            json!([unplaced(
                "OCR turns the pixels of a page image into text",
                "common"
            )]),
        )
        .await;
        assert_eq!(judged.verdict, ClaimVerdict::Unsupported);
        assert!(judged
            .reason
            .unwrap()
            .contains("No passage states any part of it."));
    }

    #[test]
    fn a_comparison_is_read_part_by_part_only_against_what_a_section_cites() {
        let mut item = pending("A statement.");
        assert!(applies(&item));
        // A retrieved page is the strict judge's.
        item.pass = Pass::Corroboration;
        assert!(!applies(&item));
        // So is an example's run: it is no source.
        item.pass = Pass::Attribution;
        item.evidence[0].retrieval_kind = PassageKind::Run.label().into();
        assert!(!applies(&item));
    }
}
