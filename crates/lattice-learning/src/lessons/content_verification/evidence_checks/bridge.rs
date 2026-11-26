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
use super::requests::{finish, shown, unlabelled};
use super::*;
use lattice_core::application::services::claim_verification::{
    BridgeOutcome, BridgePart, ClassJudgment, PartKind,
};
use lattice_core::domain::citation::Anchor;
use lattice_core::domain::learning::acceptance::Outcome;

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
    let passages = shown(&pending, labels);
    let answered = {
        let _model_call = crate::lesson_progress::model_call(crate::lesson_progress::Term::Reading);
        bridge_checker(llm)
            .check_bridge(&pending.checked, &passages)
            .await
    };
    let answer = AnswerSize::of(answered.output_tokens, 1);
    let (judgment, parts) = match answered.judgments {
        ClassJudgment::Judged(outcome) => weigh(outcome, &pending, names_a_specific),
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
/// judgment of it, with the parts as the finding records them, each with the
/// sentences it rests on; `names_a_specific` guards what may count as common.
pub(super) fn weigh(
    outcome: BridgeOutcome,
    pending: &PendingClaim,
    names_a_specific: impl Fn(&str) -> bool,
) -> (ClaimJudgment, Vec<CheckedPart>) {
    let (outcome, smuggled) = outcome.guarded(names_a_specific);
    let parts: Vec<CheckedPart> = outcome
        .parts
        .iter()
        .map(|part| CheckedPart {
            part: part.part.clone(),
            kind: part.kind,
            source_id: None,
            anchors: anchors_of(part, pending),
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
    // The text of the passage the first part of one of `kinds` rests on.
    let cited = |kinds: &[PartKind]| {
        outcome
            .parts
            .iter()
            .filter(|part| kinds.contains(&part.kind))
            .find_map(|part| {
                part.sentences
                    .iter()
                    .find_map(|at| pending.evidence.get(at.passage))
            })
            .map(|passage| passage.text.clone())
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
            (
                ClaimVerdict::Supported,
                cited(&[PartKind::Stated, PartKind::Follows]),
                reason,
            )
        }
        Outcome::Refuted => (
            ClaimVerdict::Contradicted,
            cited(&[PartKind::Contradicted]),
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
            } else if !parts
                .iter()
                .any(|part| matches!(part.kind, PartKind::Stated | PartKind::Follows))
            {
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

/// The sentences `part` rests on, as spans of the saved versions of the
/// passages they are in. Only a part that rests on sentences has any.
fn anchors_of(_part: &BridgePart, _pending: &PendingClaim) -> Vec<Anchor> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
    use super::*;
    use crate::reference_collection::PassageKind;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use lattice_core::application::services::claim_verification::{SentenceAt, ShownPassage};
    use serde_json::{json, Value};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    const CITED: &str = "Tesseract is an OCR engine that ships with the toolchain.";
    const GUIDE: &str = "Quillet keeps notes in plain files. Quillet stores each note as one file. A note never leaves the folder.";
    const GUIDE_START: usize = 500;
    const STORES: &str = "Quillet stores each note as one file.";

    /// Answers each request with the next reply it was given, and keeps what
    /// the model was asked.
    struct Said {
        replies: Mutex<VecDeque<String>>,
        heard: Mutex<Vec<String>>,
    }

    impl Said {
        fn new(replies: impl IntoIterator<Item = String>) -> Self {
            Self {
                replies: Mutex::new(replies.into_iter().collect()),
                heard: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl LLMPort for Said {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let (_, user) = crate::lessons::structured_call::task_messages(request);
            self.heard.lock().unwrap().push(user.to_owned());
            Ok(CompletionResponse::from_text(
                self.replies.lock().unwrap().pop_front().unwrap_or_default(),
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
        fn enforces(
            &self,
            _schema: &lattice_core::application::services::model_contract::shape::Schema,
        ) -> bool {
            false
        }
    }

    fn passage(source: &str, text: &str, start_byte: usize) -> EvidencePassage {
        EvidencePassage {
            source_id: source.into(),
            text: text.into(),
            start_byte,
            end_byte: start_byte + text.len(),
            retrieval_kind: "lexical".into(),
            score: 1.0,
        }
    }

    fn pending_on(statement: &str, evidence: Vec<EvidencePassage>) -> PendingClaim {
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
            evidence,
            pass: Pass::Attribution,
        }
    }

    fn pending(statement: &str) -> PendingClaim {
        pending_on(statement, vec![passage("cited", CITED, 0)])
    }

    /// The cited passage, then the guide, whose version starts at byte 500.
    fn pending_on_two(statement: &str) -> PendingClaim {
        pending_on(
            statement,
            vec![
                passage("cited", CITED, 0),
                passage("guide", GUIDE, GUIDE_START),
            ],
        )
    }

    fn reply(parts: Value) -> String {
        json!({"parts": parts, "reason": "The judge's reason."}).to_string()
    }

    fn stated(words: &str) -> Value {
        json!({"words": words, "kind": "stated", "from": ["s1"]})
    }

    fn unplaced(words: &str, kind: &str) -> Value {
        json!({"words": words, "kind": kind, "from": []})
    }

    fn contradicted(words: &str) -> Value {
        json!({"words": words, "kind": "contradicted", "from": ["s1"]})
    }

    /// What the judge made of `pending` when it answered `replies`, and what
    /// it was asked.
    async fn asked(
        pending: &PendingClaim,
        labels: &Labels,
        replies: Vec<String>,
    ) -> (BridgeOutcome, Vec<String>) {
        let model = Said::new(replies);
        let checker = ClaimChecker::for_policy(
            &model,
            SamplingOverride::deterministic(),
            CheckPolicy::Bridge,
        );
        let passages = shown(pending, labels);
        let ClassJudgment::Judged(outcome) = checker
            .check_bridge(&pending.checked, &passages)
            .await
            .judgments
        else {
            panic!("the reply was not read");
        };
        let heard = model.heard.lock().unwrap().clone();
        (outcome, heard)
    }

    fn judged(judgment: ClaimJudgment) -> JudgeOutcome {
        match judgment {
            ClaimJudgment::Judged(judged) => judged,
            other => panic!("{other:?}"),
        }
    }

    /// The judgment and the recorded parts for `statement` when the judge
    /// answers with `parts`, whatever verdict it writes beside them.
    async fn weighed(statement: &str, parts: Value) -> (JudgeOutcome, Vec<CheckedPart>) {
        let pending = pending(statement);
        let (outcome, _) = asked(&pending, &Labels::new(), vec![reply(parts)]).await;
        let (judgment, parts) = weigh(outcome, &pending, states_a_specific);
        (judged(judgment), parts)
    }

    fn kinds(parts: &[CheckedPart]) -> Vec<PartKind> {
        parts.iter().map(|part| part.kind).collect()
    }

    /// The anchor of `sentence` in `text`, a passage of `source` that starts at
    /// `start_byte` of its version.
    fn anchor_of(source: &str, text: &str, start_byte: usize, sentence: &str) -> Anchor {
        let start = text.find(sentence).unwrap();
        Anchor {
            source_version_id: source.into(),
            start_byte: start_byte + start,
            end_byte: start_byte + start + sentence.len(),
        }
    }

    #[tokio::test]
    async fn a_part_of_the_lesson_s_own_is_named_in_the_reason() {
        let statement = "A rule set of 48 checks has 12 that fail, which is 25%.";
        let reply = reply(json!([
            stated("A rule set of 48 checks"),
            unplaced("has 12 that fail, which is 25%", "own"),
        ]));
        let pending = pending(statement);
        let (outcome, _) = asked(&pending, &Labels::new(), vec![reply.clone(), reply]).await;
        let (judgment, _) = weigh(outcome, &pending, states_a_specific);
        let judged = judged(judgment);
        assert_eq!(judged.verdict, ClaimVerdict::Supported);
        assert!(judged.reason.unwrap().contains(
            "The lesson's own figures, not in the passages: has 12 that fail, which is 25%."
        ));
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
            json!([contradicted("modular document parsing pipelines were introduced by Google in 2019")]),
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
                contradicted("written by Microsoft"),
                contradicted("in 2019"),
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
                    source_id: None,
                    anchors: vec![anchor_of("cited", CITED, 0, CITED)],
                },
                CheckedPart {
                    part: "OCR turns the pixels of a page image into text".into(),
                    kind: PartKind::Common,
                    source_id: None,
                    anchors: Vec::new(),
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
                &format!("Tesseract is an OCR engine and {smuggled}."),
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
            "Tesseract is an OCR engine and OCR reads pixels into text.",
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

    #[tokio::test]
    async fn a_stated_part_s_anchor_is_its_sentence_in_the_version_that_holds_it() {
        let pending = pending_on_two(STORES);
        let (outcome, _) = asked(
            &pending,
            &Labels::new(),
            vec![reply(json!([
                {"words": STORES, "kind": "stated", "from": ["s3"]}
            ]))],
        )
        .await;
        let (judgment, parts) = weigh(outcome, &pending, states_a_specific);
        assert_eq!(judged(judgment).quote.as_deref(), Some(GUIDE));
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].source_id, None);
        assert_eq!(
            parts[0].anchors,
            vec![anchor_of("guide", GUIDE, GUIDE_START, STORES)]
        );
        let anchor = &parts[0].anchors[0];
        assert_eq!(
            GUIDE.get(anchor.start_byte - GUIDE_START..anchor.end_byte - GUIDE_START),
            Some(STORES)
        );
    }

    #[tokio::test]
    async fn a_follows_part_keeps_its_sentences_and_a_common_part_has_none() {
        let statement = "Quillet stores each note as one file, so a note is a file.";
        let pending = pending_on_two(statement);
        let (outcome, _) = asked(
            &pending,
            &Labels::new(),
            vec![reply(json!([
                {"words": "Quillet stores each note as one file", "kind": "follows", "from": ["s2", "s3"]},
                {"words": "so a note is a file", "kind": "common", "from": ["s4"]},
            ]))],
        )
        .await;
        let (judgment, parts) = weigh(outcome, &pending, states_a_specific);
        let judged = judged(judgment);
        assert_eq!(judged.verdict, ClaimVerdict::Supported);
        assert_eq!(judged.quote.as_deref(), Some(GUIDE));
        assert_eq!(kinds(&parts), vec![PartKind::Follows, PartKind::Common]);
        assert_eq!(
            parts[0].anchors,
            vec![
                anchor_of(
                    "guide",
                    GUIDE,
                    GUIDE_START,
                    "Quillet keeps notes in plain files."
                ),
                anchor_of("guide", GUIDE, GUIDE_START, STORES),
            ]
        );
        assert!(parts[1].anchors.is_empty());
    }

    #[tokio::test]
    async fn a_contradicted_part_keeps_its_sentence() {
        let pending = pending_on_two("A note never leaves the folder.");
        let (outcome, _) = asked(
            &pending,
            &Labels::new(),
            vec![reply(json!([
                {"words": "A note never leaves the folder", "kind": "contradicted", "from": ["s1"]}
            ]))],
        )
        .await;
        let (_, parts) = weigh(outcome, &pending, states_a_specific);
        assert_eq!(parts[0].anchors, vec![anchor_of("cited", CITED, 0, CITED)]);
    }

    #[test]
    fn a_sentence_the_passage_does_not_hold_is_dropped() {
        let pending = pending_on_two(STORES);
        let at = |passage, start, end| SentenceAt {
            passage,
            start,
            end,
        };
        let start = GUIDE.find(STORES).unwrap();
        let outcome = BridgeOutcome {
            parts: vec![BridgePart {
                part: STORES.into(),
                kind: PartKind::Stated,
                sentences: vec![
                    at(1, start, start + STORES.len()),
                    at(9, 0, 3),
                    at(1, 0, GUIDE.len() + 1),
                ],
            }],
            reason: "The judge's reason.".into(),
        };
        let (_, parts) = weigh(outcome, &pending, states_a_specific);
        assert_eq!(
            parts[0].anchors,
            vec![anchor_of("guide", GUIDE, GUIDE_START, STORES)]
        );
    }

    #[tokio::test]
    async fn the_judge_is_shown_each_label_and_then_the_passage_with_its_sentences_marked() {
        let pending = pending_on_two(STORES);
        let labels = Labels::from([("guide".to_owned(), "The Quillet Guide".to_owned())]);
        let (_, heard) = asked(
            &pending,
            &labels,
            vec![reply(json!([
                {"words": STORES, "kind": "stated", "from": ["s3"]}
            ]))],
        )
        .await;
        assert_eq!(heard.len(), 1);
        let marked = format!(
            "[s1] {CITED}\n\nThe Quillet Guide\n[s2] Quillet keeps notes in plain files. [s3] {STORES} [s4] A note never leaves the folder."
        );
        assert!(heard[0].contains(&marked), "{}", heard[0]);
    }

    #[test]
    fn the_passages_are_shown_with_their_labels_and_their_own_text() {
        let pending = pending_on_two(STORES);
        let labels = Labels::from([("guide".to_owned(), "The Quillet Guide".to_owned())]);
        assert_eq!(
            shown(&pending, &labels),
            vec![
                ShownPassage {
                    label: String::new(),
                    text: CITED.into()
                },
                ShownPassage {
                    label: "The Quillet Guide".into(),
                    text: GUIDE.into()
                },
            ]
        );
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
