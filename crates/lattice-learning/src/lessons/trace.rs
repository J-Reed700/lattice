//! The lesson trace: the one place a record of what the preparation pipeline
//! did to a lesson is written. A record is an INFO event on [`TARGET`], inside
//! the lesson's span ([`lesson_span`]), so the app's log alone shows, for each
//! lesson, every change made to its text and every decision taken about it, in
//! order. Free text is logged with Debug formatting so the log stays
//! parseable, and never truncated. Pure helpers never log: the stage that
//! calls them records what they returned.
use super::review_findings::{sentences_in, ReviewFinding};
use lattice_core::domain::learning::acceptance::Step;
use lattice_core::domain::learning::claim_review::{
    CheckResult, ClaimEvent, ClaimId, RepairOutcome,
};
use serde_json::Value;
use std::collections::HashMap;

/// The target of every trace record.
pub const TARGET: &str = "lattice_learning::lesson_trace";

/// The stages of the preparation pipeline a record names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// What the course does to its sources before any lesson reads them.
    CourseSetup,
    /// Planning a lesson's evidence and gathering the passages for it.
    ReferenceGathering,
    /// Searching the web for references and choosing which pages to keep.
    Research,
    /// Writing the lesson and the normalising done to the draft.
    Drafting,
    /// Extracting a lesson's statements and identifying them as claims.
    ClaimIdentification,
    /// Reading each claim against its evidence, to a verdict.
    ClaimVerification,
    /// Rewriting, re-citing or cutting a claim's sentence.
    ClaimRepair,
    /// Restoring the flow of a section after a cut.
    FlowRepair,
    /// The teaching review raising and judging findings.
    TeachingReview,
    /// The teaching repair editing the lesson for its findings.
    TeachingRepair,
    /// A change the user made that the pipeline applied.
    UserEdit,
    /// The Verifier audit.
    Audit,
    /// Writing the lesson's quiz bank.
    Quiz,
    /// Publishing or holding the finished lesson.
    Publishing,
}

impl Stage {
    /// The name a record carries.
    pub const fn name(self) -> &'static str {
        match self {
            Self::CourseSetup => "course_setup",
            Self::ReferenceGathering => "reference_gathering",
            Self::Research => "research",
            Self::Drafting => "drafting",
            Self::ClaimIdentification => "claim_identification",
            Self::ClaimVerification => "claim_verification",
            Self::ClaimRepair => "claim_repair",
            Self::FlowRepair => "flow_repair",
            Self::TeachingReview => "teaching_review",
            Self::TeachingRepair => "teaching_repair",
            Self::UserEdit => "user_edit",
            Self::Audit => "audit",
            Self::Quiz => "quiz",
            Self::Publishing => "publishing",
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The span every record of one lesson is made inside.
pub fn lesson_span(lesson_id: &str) -> tracing::Span {
    tracing::info_span!(target: TARGET, "lesson", lesson = ?lesson_id)
}

/// The span the records of a course-level stage are made inside.
pub fn course_span(course_id: &str) -> tracing::Span {
    tracing::info_span!(target: TARGET, "course", course = ?course_id)
}

/// What a change was made for: the findings and claims behind it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cause {
    findings: Vec<String>,
    claims: Vec<String>,
}

impl Cause {
    /// A change with no finding or claim behind it.
    pub fn none() -> Self {
        Self::default()
    }

    /// The ids of the review findings behind the change.
    pub fn findings<'a>(mut self, findings: impl IntoIterator<Item = &'a ReviewFinding>) -> Self {
        self.findings.extend(
            findings
                .into_iter()
                .map(|finding| finding.id().as_str().to_owned()),
        );
        self
    }

    /// The claims behind the change.
    pub fn claims<'a>(mut self, claims: impl IntoIterator<Item = &'a ClaimId>) -> Self {
        self.claims
            .extend(claims.into_iter().map(|id| id.0.clone()));
        self
    }
}

/// The sentences a change removed and the sentences it added, in order, with
/// duplicates kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

/// The sentences of `side` with no unused equal sentence in `other`, matching
/// each sentence of `other` at most once.
fn unmatched(side: &[String], other: &[String]) -> Vec<String> {
    let mut available: HashMap<&str, usize> = HashMap::new();
    for sentence in other {
        *available.entry(sentence.as_str()).or_default() += 1;
    }
    side.iter()
        .filter(|sentence| match available.get_mut(sentence.as_str()) {
            Some(left) if *left > 0 => {
                *left -= 1;
                false
            }
            _ => true,
        })
        .cloned()
        .collect()
}

/// What changed between two lists of sentences: those only `before` has are
/// removed, those only `after` has are added, each in its own order.
pub fn diff_sentences(before: &[String], after: &[String]) -> Diff {
    Diff {
        removed: unmatched(before, after),
        added: unmatched(after, before),
    }
}

/// The content a text stands for: a lesson or part as JSON, else one string.
fn content(text: &str) -> Value {
    match serde_json::from_str::<Value>(text) {
        Ok(value) if value.is_object() || value.is_array() => value,
        _ => Value::String(text.to_owned()),
    }
}

/// The sentence-level change from `before` to `after`, each a lesson (or part
/// of one) as JSON, or plain text.
pub fn diff(before: &str, after: &str) -> Diff {
    diff_sentences(
        &sentences_in(&content(before)),
        &sentences_in(&content(after)),
    )
}

/// A stage replaced the lesson, or one of its parts, with new content: the
/// sentences removed and added, and what the change was made for. Nothing is
/// recorded when the sentences are the same. Only a real replacement of the
/// text is a `changed`; a ledger event is a `decided`.
pub fn changed(stage: Stage, cause: &Cause, before: &str, after: &str) {
    if before == after {
        return;
    }
    let Diff { removed, added } = diff(before, after);
    tracing::info!(
        target: TARGET,
        stage = ?stage.name(),
        cause_findings = ?cause.findings,
        cause_claims = ?cause.claims,
        removed = ?removed,
        added = ?added,
        "changed"
    );
}

/// A stage refused or put back a proposed change, with what was proposed
/// (`before`, `after`) and why it was refused.
pub fn refused(stage: Stage, subject: &str, reason: &str, proposed: Option<(&str, &str)>) {
    let Diff { removed, added } = proposed.map(|(b, a)| diff(b, a)).unwrap_or_default();
    tracing::info!(
        target: TARGET,
        stage = ?stage.name(),
        subject = ?subject,
        reason = ?reason,
        proposed_removed = ?removed,
        proposed_added = ?added,
        "refused"
    );
}

/// A finding, claim, source or page got a status.
pub fn decided(stage: Stage, subject: &str, verdict: &str, why: &str) {
    tracing::info!(
        target: TARGET,
        stage = ?stage.name(),
        subject = ?subject,
        verdict = ?verdict,
        why = ?why,
        "decided"
    );
}

/// A review finding got a status: its id, kind, site, quoted passage and note
/// are the subject and the reason's context.
pub fn decided_finding(stage: Stage, finding: &ReviewFinding, verdict: &str, why: &str) {
    let site = finding
        .site
        .map_or_else(|| "the lesson".to_owned(), |site| site.plain());
    let passage = finding
        .passage
        .as_deref()
        .or(finding.sentence.as_deref())
        .unwrap_or("");
    decided(
        stage,
        &format!("finding {}", finding.id().as_str()),
        verdict,
        &format!(
            "kind {}; site {site}; passage \"{passage}\"; note: {}; {why}",
            finding.kind.as_str(),
            finding.note
        ),
    );
}

/// A lesson was published, or finished, with its final counts.
pub fn published(
    subject: &str,
    verdict: &str,
    sections: usize,
    questions: usize,
    unverified: usize,
) {
    tracing::info!(
        target: TARGET,
        stage = ?Stage::Publishing.name(),
        subject = ?subject,
        verdict = ?verdict,
        sections,
        questions,
        unverified,
        "published"
    );
}

/// Names a group of findings sent together: `findings a, b`.
pub fn group(findings: &[ReviewFinding]) -> String {
    let ids: Vec<String> = findings
        .iter()
        .map(|finding| finding.id().as_str().to_owned())
        .collect();
    format!("findings {}", ids.join(", "))
}

fn step_stage(step: Step) -> Stage {
    match step {
        Step::Read(_) => Stage::ClaimVerification,
        Step::Research => Stage::Research,
        _ => Stage::ClaimRepair,
    }
}

fn verdict_of(result: &CheckResult) -> (String, String) {
    match result {
        CheckResult::Supported {
            source_version_id,
            supporting_quote,
        } => (
            "supported".to_owned(),
            format!("by {source_version_id}: \"{supporting_quote}\""),
        ),
        CheckResult::SupportedElsewhere {
            cited,
            source_version_id,
            supporting_quote,
        } => (
            "supported elsewhere".to_owned(),
            format!("by {source_version_id}, not the cited {cited:?}: \"{supporting_quote}\""),
        ),
        CheckResult::Consistent => ("consistent".to_owned(), String::new()),
        CheckResult::KnownCommon => ("known common".to_owned(), String::new()),
        CheckResult::NotFalse => ("not false".to_owned(), String::new()),
        CheckResult::Failed { kind } => ("failed".to_owned(), format!("{kind:?}")),
    }
}

/// The name of the variant of a debug-printed value.
fn variant_of(event: &ClaimEvent) -> String {
    format!("{event:?}")
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn claim_subject(claim: &ClaimId, statement: &str) -> String {
    format!("claim {}: {statement}", claim.0)
}

/// A claim's citation was left where it was, for `why`.
pub(crate) fn citation_kept(claim: &ClaimId, statement: &str, why: &str) {
    decided(
        Stage::ClaimRepair,
        &claim_subject(claim, statement),
        "citation not moved",
        why,
    );
}

/// A claim's event was recorded in the ledger: always a decision.
pub(crate) fn claim_event(claim: &ClaimId, statement: &str, event: &ClaimEvent) {
    let subject = claim_subject(claim, statement);
    match event {
        ClaimEvent::Extracted { location, .. } => decided(
            Stage::ClaimIdentification,
            &subject,
            "extracted",
            &format!("unit {} field {}", location.unit_index, location.field),
        ),
        ClaimEvent::Classified { class, by } => decided(
            Stage::ClaimIdentification,
            &subject,
            &format!("class {class:?}"),
            &format!("decided by {by:?}"),
        ),
        ClaimEvent::Checked(check) => {
            let (verdict, detail) = verdict_of(&check.result);
            let passages: Vec<String> = check
                .evidence
                .iter()
                .map(|seen| {
                    format!(
                        "{}@{}-{}",
                        seen.source_version_id, seen.start_byte, seen.end_byte
                    )
                })
                .collect();
            decided(
                Stage::ClaimVerification,
                &subject,
                &verdict,
                &format!(
                    "reader {:?}; passages {passages:?}; {detail}; {}",
                    check.checker, check.reason
                ),
            );
        }
        ClaimEvent::Executed {
            observation_id,
            passed,
            evidence,
        } => decided(
            Stage::ClaimVerification,
            &subject,
            if *passed { "example passed" } else { "example failed" },
            &format!("run {observation_id}: {evidence}"),
        ),
        ClaimEvent::Researched {
            queries,
            sources_added,
            passages_found,
            ..
        } => decided(
            Stage::Research,
            &subject,
            "researched",
            &format!(
                "queries {queries:?}; sources saved {sources_added:?}; passages found {passages_found}"
            ),
        ),
        ClaimEvent::Stepped {
            step,
            outcome,
            then,
            reason,
            ..
        } => decided(
            step_stage(*step),
            &subject,
            &format!("{step:?} gave {outcome:?}"),
            &format!("then {then:?}; {reason}"),
        ),
        ClaimEvent::RepairAttempted {
            attempt,
            escalated,
            before,
            after,
            outcome,
            ..
        } => match outcome {
            RepairOutcome::Rewritten => decided(
                Stage::ClaimRepair,
                &subject,
                "rewritten",
                &format!(
                    "attempt {attempt}, escalated {escalated}: from {before:?} to {:?}",
                    after.as_deref().unwrap_or_default()
                ),
            ),
            RepairOutcome::Removed => decided(
                Stage::ClaimRepair,
                &subject,
                "removed",
                &format!("attempt {attempt}, escalated {escalated}: {before:?}"),
            ),
            RepairOutcome::Rejected { reason } => refused(
                Stage::ClaimRepair,
                &format!("{subject} (attempt {attempt}, escalated {escalated})"),
                reason,
                after.as_deref().map(|after| (before.as_str(), after)),
            ),
            RepairOutcome::SupersededByUser => decided(
                Stage::ClaimRepair,
                &subject,
                "repair superseded by the user",
                &format!("attempt {attempt} on \"{before}\""),
            ),
        },
        ClaimEvent::CitationRepaired {
            cited,
            source_version_id,
            quote,
            added_source,
        } => decided(
            Stage::ClaimRepair,
            &subject,
            "citation moved",
            &format!(
                "from {cited:?} to {source_version_id} (new to the lesson: {added_source}): \"{quote}\""
            ),
        ),
        ClaimEvent::CitationDropped { cited } if cited.is_empty() => decided(
            Stage::ClaimRepair,
            &subject,
            "no citation of its own to drop",
            "its section's citation stays",
        ),
        ClaimEvent::CitationDropped { cited } => decided(
            Stage::ClaimRepair,
            &subject,
            "citation dropped",
            &format!("it cited {cited:?}"),
        ),
        ClaimEvent::SentenceCut { cut } => decided(
            Stage::ClaimRepair,
            &subject,
            "cut",
            &format!("{:?}", cut.sentence),
        ),
        ClaimEvent::PublishedUnverified { reason } => decided(
            Stage::ClaimVerification,
            &subject,
            "published not verified",
            &format!("{reason:?}"),
        ),
        ClaimEvent::Stopped { reason } => decided(
            Stage::ClaimVerification,
            &subject,
            "stopped",
            &reason.message,
        ),
        other => decided(
            Stage::ClaimVerification,
            &subject,
            &variant_of(other),
            &format!("{other:?}"),
        ),
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
