//! One check that a prepared lesson's stages agree: the claim ledger, the
//! report, the learner's view and the lesson itself, each read through the
//! owner of its rule.
use super::super::claim_ledger::{self, ClaimLedger};
use super::super::resolution::{standing, Resolution, Standing};
use super::super::{ClaimStates, Finding, LessonVerificationReport};
use crate::dto::{LearningBlockDto, LearningLessonDto};
use crate::lesson_evidence::{
    LearningClaimResolutionDto, LearningLessonEvidenceDto, SavedEvidenceVersion,
    UnverifiedReasonDto,
};
use crate::lessons::claim_reviews::repository::{ClaimReviewRepository, ShownPassage};
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::services::claim_verification::quote_in;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, StandardPolicy, Then, Walked};
use lattice_core::domain::learning::claim_review::{
    CheckResult, ClaimCounts, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, Ledger,
    ReopenCause,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::hash::sha256_hex;
use serde_json::{json, Value};
use std::collections::HashMap;

/// What a whole-lesson test has at its end.
pub(in crate::lessons::content_verification) struct PreparedRun {
    /// The published lesson as its draft holds it.
    pub lesson: Value,
    /// The claim ledger as the production store loads it.
    pub ledger: Ledger,
    /// The passages each claim's latest check compared, from the same store.
    pub shown: HashMap<ClaimId, Vec<ShownPassage>>,
    pub report: LessonVerificationReport,
    /// What `learner_view` builds from the report and the ledger.
    pub view: LearningLessonEvidenceDto,
}

impl PreparedRun {
    /// The run of a lesson already loaded: its learner's view is built here.
    pub(in crate::lessons::content_verification) fn assemble(
        lesson: Value,
        ledger: Ledger,
        shown: HashMap<ClaimId, Vec<ShownPassage>>,
        mut report: LessonVerificationReport,
    ) -> Result<Self> {
        report.lesson_id.clone_from(&ledger.lesson_id);
        report.content_sha256 = sha256_hex(lesson.to_string());
        let view = report.learner_view(
            &lesson_of(&ledger.lesson_id, &lesson)?,
            &versions(&report),
            &ClaimStates::of(&ledger),
        )?;
        Ok(Self {
            lesson,
            ledger,
            shown,
            report,
            view,
        })
    }

    /// The run of `lesson_id`, its ledger and shown passages loaded through
    /// the production store.
    pub(in crate::lessons::content_verification) async fn load(
        pool: &sqlx::SqlitePool,
        lesson_id: &str,
        lesson: Value,
        report: LessonVerificationReport,
    ) -> Result<Self> {
        let store = ClaimReviewRepository::new(pool.clone());
        let ledger = store.load(lesson_id).await?;
        let shown = store.shown_passages(lesson_id).await?;
        Self::assemble(lesson, ledger, shown, report)
    }
}

/// [`assert_pipeline_invariants`] on the lesson `raw` and its `report`, with
/// the rest loaded from the store.
pub(in crate::lessons::content_verification) async fn assert_prepared(
    pool: &sqlx::SqlitePool,
    lesson_id: &str,
    raw: &str,
    report: &LessonVerificationReport,
) -> Result<()> {
    let lesson = serde_json::from_str(raw)?;
    let run = PreparedRun::load(pool, lesson_id, lesson, report.clone()).await?;
    assert_pipeline_invariants(&run);
    Ok(())
}

/// Fails once, listing every way the stages of `run` disagree.
pub(in crate::lessons::content_verification) fn assert_pipeline_invariants(run: &PreparedRun) {
    let found = disagreements(run);
    assert!(
        found.is_empty(),
        "the stages of the prepared lesson disagree:\n{}",
        found.join("\n")
    );
}

fn disagreements(run: &PreparedRun) -> Vec<String> {
    let policy = StandardPolicy::new();
    let ledger = ClaimLedger::holding(run.ledger.claims.clone());
    let mut found = Vec::new();
    faults(run, &ledger, &mut found);
    published(run, &mut found);
    labels(run, &ledger, &policy, &mut found);
    quotes(run, &mut found);
    steps(run, &ledger, &policy, &mut found);
    found
}

fn lesson_of(lesson_id: &str, lesson: &Value) -> Result<LearningLessonDto> {
    let mut dto = crate::tests::fixture().modules.remove(0).lessons.remove(0);
    dto.id = lesson_id.to_owned();
    dto.blocks = lesson
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|block| {
            let mut block = block.clone();
            if let Some(fields) = block.as_object_mut() {
                fields.entry("sourceIds").or_insert_with(|| json!([]));
            }
            Ok(serde_json::from_value::<LearningBlockDto>(block)?)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(dto)
}

fn versions(report: &LessonVerificationReport) -> Vec<SavedEvidenceVersion> {
    report
        .sources
        .iter()
        .map(|binding| SavedEvidenceVersion {
            id: binding.id.clone(),
            title: "Source".into(),
            resolved_url: None,
            requested_url: None,
            content_sha256: binding.sha256.clone(),
            active_version_id: Some(binding.id.clone()),
            deleted_at: None,
        })
        .collect()
}

fn entry_of<'r>(run: &'r PreparedRun, id: &str) -> Option<&'r ClaimLedgerEntry> {
    run.ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id().0 == id)
}

fn about(entry: &ClaimLedgerEntry) -> String {
    format!("claim {} ({:?})", entry.claim_id().0, entry.statement())
}

fn about_id(run: &PreparedRun, id: &ClaimId) -> String {
    entry_of(run, &id.0).map_or_else(|| format!("claim {}", id.0), about)
}

fn about_finding(finding: &Finding) -> String {
    format!(
        "the finding on {:?} (section {})",
        finding.statement, finding.unit
    )
}

fn standing_of(run: &PreparedRun, state: &ClaimState) -> Standing {
    standing(state, &|id: &ClaimId| {
        run.ledger
            .claims
            .iter()
            .find(|entry| entry.claim_id() == id)
            .map(|entry| entry.state())
    })
}

/// 1. No statement met a fault the machine contained.
fn faults(run: &PreparedRun, ledger: &ClaimLedger, found: &mut Vec<String>) {
    for fault in ledger.faults() {
        found.push(format!(
            "{}: a fault was recorded at {:?}: {}",
            about_id(run, &fault.claim),
            fault.stage,
            fault.message
        ));
    }
}

/// 2. Every statement the published lesson holds stands on a claim a lesson
/// can publish with, and none the ledger removed or cut is in it.
fn published(run: &PreparedRun, found: &mut Vec<String>) {
    for entry in &run.ledger.claims {
        let place = entry.location();
        let in_lesson =
            claim_ledger::stands(&run.lesson, place.unit_index, &place.field, &place.quote);
        if entry.code_example().is_some() || !in_lesson {
            continue;
        }
        match standing_of(run, entry.state()) {
            Standing::Unsettled => found.push(format!(
                "{}: its sentence is in the published lesson, but the claim is {:?}, which a lesson cannot publish with",
                about(entry),
                entry.state()
            )),
            Standing::Gone if !carried_on(run, entry) => found.push(format!(
                "{}: the ledger removed or cut it, yet its sentence is still in the published lesson and no live claim stands on it",
                about(entry)
            )),
            _ => {}
        }
    }
    for finding in run.report.findings.iter().filter(|f| f.is_claim()) {
        let Some(entry) = finding.claim_id.as_deref().and_then(|id| entry_of(run, id)) else {
            found.push(format!(
                "{} names a claim the ledger does not hold ({:?})",
                about_finding(finding),
                finding.claim_id
            ));
            continue;
        };
        match standing_of(run, entry.state()) {
            Standing::Unsettled | Standing::Gone => found.push(format!(
                "{}: the report publishes it, but the claim is {:?}",
                about(entry),
                entry.state()
            )),
            Standing::Verified | Standing::Resolved(_) => {}
        }
        if !finding.publishable() {
            found.push(format!(
                "{}: the report publishes a finding that is neither supported nor resolved",
                about(entry)
            ));
        }
    }
    let counts = ClaimCounts::tally(run.ledger.claims.iter());
    if counts.unresolved() != 0 {
        found.push(format!(
            "the ledger holds {} unresolved claims at publication",
            counts.unresolved()
        ));
    }
}

/// Whether a live claim stands on the sentence `entry` stood on.
fn carried_on(run: &PreparedRun, entry: &ClaimLedgerEntry) -> bool {
    let place = entry.location();
    run.ledger.claims.iter().any(|other| {
        other.claim_id() != entry.claim_id()
            && other.location().unit_index == place.unit_index
            && other.location().quote == place.quote
            && !matches!(standing_of(run, other.state()), Standing::Gone)
    })
}

/// 3. Every label the learner reads and every count of them is what the
/// ledger permits.
fn labels(
    run: &PreparedRun,
    ledger: &ClaimLedger,
    policy: &dyn AcceptancePolicy,
    found: &mut Vec<String>,
) {
    let sections = run
        .lesson
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let listed = run
        .report
        .findings
        .iter()
        .filter(|finding| finding.is_claim() && finding.unit < sections)
        .count();
    if run.view.claim_count != listed || run.view.claim_count != run.view.teaching_claims.len() {
        found.push(format!(
            "the learner is told of {} claims and lists {}, while the report holds {} in the sections",
            run.view.claim_count,
            run.view.teaching_claims.len(),
            listed
        ));
    }
    let shown = &run.view.teaching_claims;
    let mut confirmed = 0;
    let mut not_verified = 0;
    for item in shown {
        use LearningClaimResolutionDto::{ConfirmedByYou, NotVerified, SourceRemoved, Verified};
        confirmed += usize::from(item.resolution == ConfirmedByYou);
        not_verified += usize::from(item.resolution == NotVerified);
        if item.unverified_reason.is_some() != (item.resolution == NotVerified) {
            found.push(format!(
                "{:?} (section {}): the learner is given the reason {:?} on a claim read as {:?}",
                item.claim, item.section_index, item.unverified_reason, item.resolution
            ));
        }
        let Some(entry) = item.claim_id.as_deref().and_then(|id| entry_of(run, id)) else {
            found.push(format!(
                "the learner's view lists {:?} (section {}) for a claim the ledger does not hold ({:?})",
                item.claim, item.section_index, item.claim_id
            ));
            continue;
        };
        let state = entry.state();
        let standing = standing_of(run, state);
        let mates = || {
            shown.iter().filter(|other| {
                other.claim_id != item.claim_id
                    && other.section_index == item.section_index
                    && other.content_quote == item.content_quote
            })
        };
        let verified = state.is_verified() && matches!(standing, Standing::Verified);
        let mate_not_verified = mates().any(|other| other.resolution == NotVerified);
        let ledger_mate_unverified = mates().any(|other| {
            other
                .claim_id
                .as_deref()
                .and_then(|id| entry_of(run, id))
                .is_some_and(|mate| {
                    matches!(
                        standing_of(run, mate.state()),
                        Standing::Unsettled | Standing::Resolved(Resolution::Unverified { .. })
                    )
                })
        });
        let allowed = match item.resolution {
            Verified => verified && !mate_not_verified,
            ConfirmedByYou => matches!(standing, Standing::Resolved(Resolution::Confirmed { .. })),
            SourceRemoved => matches!(
                state,
                ClaimState::Reopened {
                    cause: ReopenCause::SourceDeleted(_),
                    ..
                }
            ),
            NotVerified => !verified || ledger_mate_unverified,
        };
        if !allowed {
            found.push(format!(
                "{}: the learner reads it as {:?}, but the ledger holds it {:?} (verified {}, an unverified claim on its sentence {})",
                about(entry),
                item.resolution,
                state,
                verified,
                ledger_mate_unverified
            ));
        }
    }
    if run.view.confirmed_count != confirmed {
        found.push(format!(
            "the learner is told {} claims are confirmed by them, its list holds {}",
            run.view.confirmed_count, confirmed
        ));
    }
    let claim_findings = run
        .report
        .findings
        .iter()
        .filter(|finding| finding.is_claim())
        .count();
    if claim_findings == listed && run.report.unverified_count() != not_verified {
        found.push(format!(
            "the report counts {} statements not verified, the learner's view shows {}",
            run.report.unverified_count(),
            not_verified
        ));
    }
    let tally = ledger.tally(policy);
    let verified = run
        .ledger
        .claims
        .iter()
        .filter(|entry| entry.state().is_verified())
        .count();
    if usize::try_from(tally.verified).ok() != Some(verified) {
        found.push(format!(
            "the lesson's claim tally counts {} verified, the ledger holds {}",
            tally.verified, verified
        ));
    }
    let machine_and_user = tally.unchecked + tally.searching + tally.repairing + tally.waiting;
    if machine_and_user != 0 {
        found.push(format!(
            "the lesson's claim tally still has {machine_and_user} claims to check, repair or resolve"
        ));
    }
}

/// 4. The quotation a claim's latest check rests on is in the passage that
/// check names.
fn quotes(run: &PreparedRun, found: &mut Vec<String>) {
    for entry in &run.ledger.claims {
        let latest = entry
            .events()
            .iter()
            .rev()
            .find_map(|record| match &record.event {
                ClaimEvent::Checked(check) => Some(&check.result),
                _ => None,
            });
        let Some((source, quote)) = latest.and_then(CheckResult::support) else {
            continue;
        };
        let Some(passages) = run.shown.get(entry.claim_id()).filter(|p| !p.is_empty()) else {
            continue;
        };
        let held = passages
            .iter()
            .filter(|passage| passage.source_version_id == source)
            .any(|passage| quote_in(&passage.text, quote).is_some());
        if !held {
            found.push(format!(
                "{}: it rests on {:?}, which no passage of {} it compared holds",
                about(entry),
                quote,
                source
            ));
        }
    }
}

/// 5. Each claim's last decision is where the acceptance policy sends a
/// claim from the position its earlier steps left it.
fn steps(
    run: &PreparedRun,
    ledger: &ClaimLedger,
    policy: &StandardPolicy,
    found: &mut Vec<String>,
) {
    for entry in &run.ledger.claims {
        let Some(recorded) = entry.decided() else {
            continue;
        };
        let leads_to = match ledger.position(policy, entry.claim_id()) {
            None => {
                found.push(format!(
                    "{}: its steps cannot be placed in any class's graph",
                    about(entry)
                ));
                continue;
            }
            Some(Walked::Diverged { at, taken }) => {
                found.push(format!(
                    "{}: the step {:?} is no edge of the policy's graph from {:?}",
                    about(entry),
                    taken,
                    at
                ));
                continue;
            }
            Some(Walked::At(at)) => policy.graph(at.class).node(at.node).map(|node| Then::Node {
                name: node.name.to_owned(),
                step: node.step,
            }),
            Some(Walked::Ended(end)) => Some(Then::End(end)),
            Some(Walked::Stopped) => Some(Then::Stop),
        };
        if leads_to.as_ref() != Some(recorded) {
            found.push(format!(
                "{}: its last decision is {:?}, the policy's edge from its earlier steps leads to {:?}",
                about(entry),
                recorded,
                leads_to
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::acceptance_switch_tests::{
        claim, lesson_of as lesson_with, open, stood, SOURCED,
    };
    use super::super::claim_ledger_tests::{pass_report, passage_finding};
    use super::super::{BAD, GOOD, SOURCE};
    use super::*;
    use crate::lessons::content_verification::claim_ledger::StatementFault;
    use lattice_core::domain::learning::acceptance::{End, Outcome, Reading, Step};
    use lattice_core::domain::learning::claim_review::{
        Actor, FaultStage, Revision, UnverifiedReason,
    };

    /// The run of a ledger holding a verified claim `kept` on `GOOD`, beside
    /// whatever `more` adds, in a lesson of `body`.
    async fn run_of(
        ledger: &ClaimLedger,
        body: &str,
        shown: HashMap<ClaimId, Vec<ShownPassage>>,
    ) -> Result<PreparedRun> {
        let mut finding = passage_finding(GOOD, GOOD, true, SOURCE);
        finding.claim_id = Some("kept".into());
        let ledger = Ledger {
            lesson_id: "lesson".into(),
            review: None,
            revision: Revision(0),
            claims: ledger.entries().to_vec(),
        };
        let mut report = pass_report(vec![finding]);
        report.policy = "fixture".into();
        report.checker_model = "fixture".into();
        PreparedRun::assemble(lesson_with(body), ledger, shown, report)
    }

    fn compared(text: &str) -> HashMap<ClaimId, Vec<ShownPassage>> {
        HashMap::from([(
            ClaimId("kept".into()),
            vec![ShownPassage {
                source_version_id: "source-1".into(),
                start_byte: 0,
                end_byte: text.len(),
                text: text.into(),
                retrieval_kind: "lexical".into(),
            }],
        )])
    }

    /// A ledger whose claim `kept` was verified against `SOURCE`.
    async fn verified() -> Result<ClaimLedger> {
        let mut ledger = open().await?;
        let body = lesson_body();
        let id = claim(&mut ledger, "kept", (GOOD, GOOD), &body, SOURCED).await?;
        let read = passage_finding(GOOD, GOOD, true, SOURCE);
        stood(&mut ledger, &id, SOURCED.0, (Reading::Parts, read)).await?;
        Ok(ledger)
    }

    fn lesson_body() -> String {
        GOOD.to_owned()
    }

    fn named(found: &[String], parts: &[&str]) -> bool {
        found
            .iter()
            .any(|line| parts.iter().all(|part| line.contains(part)))
    }

    #[tokio::test]
    async fn a_lesson_whose_stages_agree_has_no_disagreement() -> Result<()> {
        let run = run_of(&verified().await?, &lesson_body(), compared(SOURCE)).await?;

        assert_eq!(disagreements(&run), Vec::<String>::new());
        assert_pipeline_invariants(&run);
        Ok(())
    }

    #[tokio::test]
    async fn a_recorded_fault_is_a_disagreement_naming_its_claim() -> Result<()> {
        let mut ledger = verified().await?;
        ledger
            .record_fault(StatementFault {
                claim: ClaimId("kept".into()),
                stage: FaultStage::Take,
                message: "no edge".into(),
            })
            .await?;

        let run = run_of(&ledger, &lesson_body(), compared(SOURCE)).await?;

        let found = disagreements(&run);
        assert!(named(&found, &["claim kept", GOOD, "no edge"]), "{found:?}");
        Ok(())
    }

    #[tokio::test]
    async fn a_lesson_holding_the_sentence_of_a_cut_or_unsettled_claim_is_a_disagreement(
    ) -> Result<()> {
        let mut ledger = verified().await?;
        let body = format!("{GOOD} {BAD}");
        let cut = claim(&mut ledger, "cut", (BAD, BAD), &body, SOURCED).await?;
        let gone = ClaimEvent::Superseded {
            successor: None,
            by: Actor::Machine,
        };
        assert!(ledger.append(&cut, gone).await?);

        let found = disagreements(&run_of(&ledger, &body, compared(SOURCE)).await?);

        assert!(
            named(&found, &["claim cut", BAD, "removed or cut"]),
            "{found:?}"
        );

        let mut ledger = verified().await?;
        claim(&mut ledger, "open", (BAD, BAD), &body, SOURCED).await?;

        let found = disagreements(&run_of(&ledger, &body, compared(SOURCE)).await?);

        assert!(
            named(&found, &["claim open", BAD, "cannot publish"]),
            "{found:?}"
        );
        assert!(named(&found, &["unresolved claims"]), "{found:?}");
        Ok(())
    }

    #[tokio::test]
    async fn a_label_or_a_count_that_differs_from_the_ledger_is_a_disagreement() -> Result<()> {
        let ledger = verified().await?;
        let mut run = run_of(&ledger, &lesson_body(), compared(SOURCE)).await?;
        run.view.teaching_claims[0].resolution = LearningClaimResolutionDto::NotVerified;
        run.view.teaching_claims[0].unverified_reason = Some(UnverifiedReasonDto::Unsupported);
        run.view.confirmed_count = 1;

        let found = disagreements(&run);

        assert!(
            named(&found, &["claim kept", GOOD, "NotVerified", "Verified"]),
            "{found:?}"
        );
        assert!(named(&found, &["confirmed by them"]), "{found:?}");

        let mut run = run_of(&ledger, &lesson_body(), compared(SOURCE)).await?;
        run.view.claim_count = 4;
        run.report.findings[0].resolution = Some(Resolution::Unverified {
            reason: UnverifiedReason::Unsupported,
        });

        let found = disagreements(&run);

        assert!(named(&found, &["told of 4 claims"]), "{found:?}");
        assert!(
            named(&found, &["report counts 1", "view shows 0"]),
            "{found:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_quotation_the_named_passage_does_not_hold_is_a_disagreement() -> Result<()> {
        let run = run_of(&verified().await?, &lesson_body(), compared(BAD)).await?;

        let found = disagreements(&run);

        assert!(
            named(&found, &["claim kept", GOOD, "source-1"]),
            "{found:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_last_decision_that_is_no_edge_of_the_policy_is_a_disagreement() -> Result<()> {
        let mut ledger = verified().await?;
        let body = format!("{GOOD} {BAD}");
        let id = claim(&mut ledger, "odd", (BAD, BAD), &body, SOURCED).await?;
        let stepped = ClaimEvent::Stepped {
            policy: StandardPolicy::new().version().into(),
            step: Step::Read(Reading::Parts),
            outcome: Outcome::Unproven,
            then: Then::End(End::GeneralKnowledge),
            reason: "Because.".into(),
            rewrite_ahead: false,
        };
        ledger.append(&id, stepped).await?;

        let found = disagreements(&run_of(&ledger, &lesson_body(), compared(SOURCE)).await?);

        assert!(
            named(&found, &["claim odd", BAD, "last decision"]),
            "{found:?}"
        );
        Ok(())
    }
}
