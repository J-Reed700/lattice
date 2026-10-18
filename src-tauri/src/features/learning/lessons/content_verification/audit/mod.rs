//! The Verifier audit (lesson claim review §14): a final grounded audit of a
//! lesson's claims before it publishes, opt-in per course.
//!
//! It runs once every claim is resolved (verified, confirmed, accepted as
//! general knowledge, or removed) and before the lesson publishes. Each
//! claim that lacks a current audit receipt — a Verifier check of the same
//! statement, the same passages, under the same policy and model — is sent,
//! with its lesson section and the exact passages the local checker
//! compared, to the Verifier in batches the Verifier's window holds. Every
//! answer becomes a `Checked { checker: Verifier }` event in the claim's
//! ledger; the ledger's rules turn a disagreement into a dispute that blocks
//! publishing. The audit never edits the lesson.
//!
//! Between batches the course's opt-in and consent are read again, so
//! switching the audit off (or changing the Verifier's provider) stops it at
//! the next batch boundary. Each batch's record and token usage is written
//! before the next is sent; a stopped or interrupted audit resumes with the
//! claims that still lack a receipt.

pub(crate) mod batches;
mod calls;
mod ledger;
pub(crate) mod lesson;
pub(crate) mod material;
pub(crate) mod pages;
pub(crate) mod policy;
pub(crate) mod runs;
pub(crate) mod tools;

use std::collections::HashMap;

use async_trait::async_trait;

use lattice_core::application::ports::{ClaimReviewStore, LLMPort};
use lattice_core::application::services::chat_model_resolution::cloud_provider_name;
use lattice_core::domain::learning::claim_review::{ClaimId, StopReason};
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};
use lattice_core::shared::error::{AppError, Result};

use batches::Item;
use calls::{AskError, WindowSizer};
use ledger::LedgerWriter;
use policy::{Consent, Judged, AUDIT_POLICY_ID};
use runs::{AuditRunStore, BatchOutcome, BatchRecord, RunKey, RunState};
use tools::VerifierWeb;

/// One lesson unit as the audit sends it: a section, a question or an
/// answer key, with the report's unit index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditUnit {
    pub index: usize,
    pub title: String,
    pub text: String,
}

/// A passage the local checker compared, or that resolved the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditPassage {
    pub source_version_id: String,
    pub title: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: String,
}

/// A claim the audit sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditClaim {
    pub claim_id: ClaimId,
    pub unit_index: usize,
    pub statement: String,
    pub quote: String,
    pub passages: Vec<AuditPassage>,
    /// Accepted as general knowledge: audited for contradiction only.
    pub general_knowledge: bool,
}

/// Why an audit does not run, or stopped (§14.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuditStop {
    /// The course's audit is off, or was switched off during the run.
    TurnedOff,
    /// The audit is on and no Verifier model is set.
    NoVerifier,
    /// The Verifier could not be loaded: its refusal, verbatim (no key
    /// saved, not downloaded, a window too small, ...).
    Unavailable { message: String },
    /// Sending lesson text to this provider was never confirmed.
    ConsentMissing { provider: CloudProvider },
    /// Consent names another provider (its stored id) than the Verifier's.
    ConsentForOtherProvider {
        consented: String,
        now: CloudProvider,
    },
}

impl AuditStop {
    /// The stop a consent state calls for; `None` when sending is allowed.
    pub(crate) fn from_consent(consent: &Consent) -> Option<Self> {
        match consent {
            Consent::NotNeeded | Consent::Given { .. } => None,
            Consent::Missing { provider } => Some(Self::ConsentMissing {
                provider: *provider,
            }),
            Consent::OtherProvider { consented, now } => Some(Self::ConsentForOtherProvider {
                consented: consented.clone(),
                now: *now,
            }),
        }
    }

    /// Whether a review's stop reason is an audit stop: one the user fixes
    /// in Settings or with the course's switch, not in the ledger, so a
    /// Resume needs no claim change.
    pub(crate) fn is_audit_stop(code: &str) -> bool {
        code.starts_with(STOP_CODE_PREFIX)
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::TurnedOff => "verifier_turned_off",
            Self::NoVerifier => "verifier_unset",
            Self::Unavailable { .. } => "verifier_unavailable",
            Self::ConsentMissing { .. } => "verifier_consent_missing",
            Self::ConsentForOtherProvider { .. } => "verifier_consent_changed",
        }
    }

    /// What the lesson card says; `progress` is `(batches done, planned)`
    /// when the audit had started.
    pub(crate) fn message(&self, progress: Option<(usize, usize)>) -> String {
        match (self, progress) {
            (Self::TurnedOff, Some((done, of))) => {
                format!("Audit stopped: Verifier turned off after batch {done} of {of}")
            }
            (Self::TurnedOff, None) => "Audit stopped: you turned the Verifier off".into(),
            (Self::NoVerifier, _) => "Audit cannot run: no Verifier model is set.".into(),
            (Self::Unavailable { message }, _) => format!("Audit cannot run: {message}"),
            (Self::ConsentMissing { provider }, _) => format!(
                "Audit cannot run: sending lesson text to {} was not confirmed for this course.",
                cloud_provider_name(*provider)
            ),
            (Self::ConsentForOtherProvider { consented, now }, _) => format!(
                "Consent was for {}; confirm again for {}",
                provider_name(consented),
                cloud_provider_name(*now)
            ),
        }
    }

    pub(crate) fn fix(&self) -> String {
        match self {
            Self::TurnedOff => {
                "The lesson publishes without the audit at the next Resume; turn the audit on \
                 again to audit it."
                    .into()
            }
            Self::NoVerifier => "Choose one in Settings → Models under Verifier, or turn the \
                                 audit off for this course."
                .into(),
            Self::Unavailable { .. } => {
                "Fix the Verifier in Settings → Models, or turn the audit off for this course."
                    .into()
            }
            Self::ConsentMissing { provider }
            | Self::ConsentForOtherProvider { now: provider, .. } => {
                format!(
                    "Turn the audit on again for this course and confirm sending lesson text to \
                     {}, or turn the audit off.",
                    cloud_provider_name(*provider)
                )
            }
        }
    }

    /// The review row's `stop_reason` for this stop.
    pub(crate) fn stop_reason(&self, progress: Option<(usize, usize)>, at: i64) -> StopReason {
        StopReason {
            code: self.code().into(),
            message: self.message(progress),
            fix: self.fix(),
            at,
            request: None,
        }
    }
}

/// What every audit stop's code starts with.
const STOP_CODE_PREFIX: &str = "verifier_";

/// A stored provider id as the user knows it.
fn provider_name(id: &str) -> String {
    [
        CloudProvider::Openai,
        CloudProvider::Anthropic,
        CloudProvider::Google,
    ]
    .into_iter()
    .find(|provider| {
        lattice_core::application::services::chat_model_resolution::cloud_provider_id(*provider)
            == id
    })
    .map_or_else(
        || id.to_owned(),
        |provider| cloud_provider_name(provider).to_owned(),
    )
}

/// Reads the course's opt-in and consent again before each batch.
#[async_trait]
pub(crate) trait AuditGate: Send + Sync {
    /// `Some` when the audit must not send anything more.
    async fn check(&self) -> Result<Option<AuditStop>>;
}

/// The audit's progress, for the lesson's progress line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuditProgress {
    Batch { index: usize, of: usize },
}

impl AuditProgress {
    pub(crate) fn label(self) -> String {
        match self {
            Self::Batch { index, of } => format!("Audit batch {index} of {of}"),
        }
    }
}

/// What an audit run did, for the card and the course's totals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AuditSummary {
    pub batches_done: usize,
    pub batches_planned: usize,
    /// Claims that received a Verifier check in this run.
    pub claims_checked: usize,
    /// Claims the Verifier disputes, now waiting for the user.
    pub disputes: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Why web search was not used, when it was allowed but unavailable.
    pub web_note: Option<String>,
}

#[derive(Debug)]
pub(crate) enum AuditOutcome {
    /// Every claim carries a current Verifier receipt.
    Completed(AuditSummary),
    /// Nothing more is sent: see the stop. Verdicts written so far stand.
    Stopped {
        stop: AuditStop,
        summary: AuditSummary,
    },
    /// A provider or network failure: deferred as §8.3, and resumed with
    /// the claims that still lack a receipt.
    Deferred {
        error: AppError,
        summary: AuditSummary,
    },
}

impl AuditOutcome {
    pub(crate) fn summary(&self) -> &AuditSummary {
        match self {
            Self::Completed(summary)
            | Self::Stopped { summary, .. }
            | Self::Deferred { summary, .. } => summary,
        }
    }

    /// Whether the audit lets the lesson publish: no dispute, and either
    /// finished or switched off (opt-in off means no audit gate). An audit
    /// that is on but cannot run, or is waiting to retry, holds the lesson.
    pub(crate) fn publishable(&self) -> bool {
        match self {
            Self::Completed(summary)
            | Self::Stopped {
                stop: AuditStop::TurnedOff,
                summary,
            } => summary.disputes == 0,
            Self::Stopped { .. } | Self::Deferred { .. } => false,
        }
    }
}

/// One audit of one lesson draft.
pub(crate) struct AuditRequest<'a> {
    pub lesson_id: &'a str,
    pub job_id: Option<&'a str>,
    pub app_version: &'a str,
    pub draft_sha256: &'a str,
    /// The pinned Verifier, as its `Checked` events and the audit row name it.
    pub verifier_model: &'a ModelRef,
    pub units: &'a [AuditUnit],
    /// Each claim's passages as its card shows them; a claim with none is
    /// sent with its section only.
    pub passages: &'a HashMap<ClaimId, Vec<AuditPassage>>,
}

pub(crate) struct AuditDeps<'a> {
    pub store: &'a dyn ClaimReviewStore,
    pub runs: &'a dyn AuditRunStore,
    pub gate: &'a dyn AuditGate,
    /// The web tools, when the course allows them and a web service is
    /// available; `None` runs the audit on the cited passages only.
    pub web: Option<&'a dyn VerifierWeb>,
    pub progress: &'a (dyn Fn(AuditProgress) + Send + Sync),
    /// Milliseconds since the Unix epoch.
    pub clock: &'a (dyn Fn() -> i64 + Send + Sync),
}

/// Audit the lesson's claims that lack a current receipt with `llm`, the
/// pinned Verifier.
///
/// # Errors
///
/// Storage failures; `AppError::ModelRejected` when the Verifier rejects a
/// request, or a batch fails with the same kind of error, not an outage, as
/// the attempt before it with no batch completed in between (§8.1, §8.3).
pub(crate) async fn run(
    llm: &dyn LLMPort,
    request: &AuditRequest<'_>,
    deps: &AuditDeps<'_>,
) -> Result<AuditOutcome> {
    let model = request.verifier_model.to_string();
    let loaded = deps.store.load(request.lesson_id).await?;
    if loaded.review.is_none() {
        return Err(AppError::InvalidState(
            "The lesson has no claim review to audit yet".into(),
        ));
    }
    let mut writer = LedgerWriter::new(deps, request, &model, llm.max_context_tokens(), &loaded);
    let mut summary = AuditSummary::default();
    if let Some(stop) = deps.gate.check().await? {
        summary.disputes = ledger::disputes(&loaded);
        return Ok(AuditOutcome::Stopped { stop, summary });
    }
    let claims = ledger::due(&loaded, request.passages, &model);
    if claims.is_empty() {
        summary.disputes = ledger::disputes(&loaded);
        return Ok(AuditOutcome::Completed(summary));
    }
    let key = RunKey {
        lesson_id: request.lesson_id.to_owned(),
        draft_sha256: request.draft_sha256.to_owned(),
        verifier_model: model.clone(),
    };
    let previous_failure = deps
        .runs
        .begin(&key, AUDIT_POLICY_ID, (deps.clock)())
        .await?
        .last_failure()
        .map(str::to_owned);
    let sizer = WindowSizer::new(llm, &claims, request.units, deps.web.is_some());
    let planned = batches::plan(&claims, &sizer);
    summary.batches_planned = planned.batches.len();
    for oversize in &planned.oversize {
        if let Some(claim) = claims.get(oversize.claim) {
            let reason = format!(
                "The Verifier's window of {} tokens cannot hold this claim's section and \
                 passages ({} tokens).",
                llm.max_context_tokens(),
                oversize.needed_tokens
            );
            writer
                .append(claim, Judged::could_not_verify(reason))
                .await?;
            summary.claims_checked += 1;
        }
    }
    let judge = Judge {
        llm,
        claims: &claims,
        units: request.units,
        sizer: &sizer,
        web: deps.web,
    };
    let mut parts: HashMap<usize, Vec<Judged>> = HashMap::new();
    let of = planned.batches.len();
    for (position, items) in planned.batches.iter().enumerate() {
        let index = position + 1;
        if let Some(stop) = deps.gate.check().await? {
            deps.runs
                .finish(&key, RunState::Stopped, (deps.clock)())
                .await?;
            summary.disputes = writer.disputes().await?;
            return Ok(AuditOutcome::Stopped { stop, summary });
        }
        (deps.progress)(AuditProgress::Batch { index, of });
        let claim_ids = items
            .iter()
            .filter_map(|item| claims.get(item.claim))
            .map(|claim| claim.claim_id.0.clone())
            .collect();
        let mut record = BatchRecord {
            index,
            of,
            claim_ids,
            outcome: BatchOutcome::Done,
            input_tokens: 0,
            output_tokens: 0,
            error: None,
            error_code: None,
            web_note: None,
            at: (deps.clock)(),
        };
        let judged = match judge.items(items).await {
            Ok(judged) => judged,
            Err(error) => {
                record.outcome = BatchOutcome::Failed;
                record.error = Some(error.to_string());
                record.error_code = Some(error.error_code().to_owned());
                deps.runs.record(&key, &record).await?;
                deps.runs
                    .finish(&key, RunState::Stopped, (deps.clock)())
                    .await?;
                summary.disputes = writer.disputes().await?;
                return failed(error, previous_failure.as_deref(), summary);
            }
        };
        record.input_tokens = judged.input_tokens;
        record.output_tokens = judged.output_tokens;
        record.web_note = judged.web_note.clone();
        deps.runs.record(&key, &record).await?;
        summary.batches_done += 1;
        summary.input_tokens += judged.input_tokens;
        summary.output_tokens += judged.output_tokens;
        if judged.web_note.is_some() {
            summary.web_note = judged.web_note;
        }
        for (item, verdict) in items.iter().zip(judged.verdicts) {
            let Some(claim) = claims.get(item.claim) else {
                continue;
            };
            let verdict = match item.part {
                None => Some(verdict),
                Some((_, parts_total)) => {
                    let collected = parts.entry(item.claim).or_default();
                    collected.push(verdict);
                    if collected.len() < parts_total {
                        None
                    } else {
                        policy::combine(parts.remove(&item.claim).unwrap_or_default())
                    }
                }
            };
            if let Some(verdict) = verdict {
                writer.append(claim, verdict).await?;
                summary.claims_checked += 1;
            }
        }
    }
    deps.runs
        .finish(&key, RunState::Completed, (deps.clock)())
        .await?;
    summary.disputes = writer.disputes().await?;
    Ok(AuditOutcome::Completed(summary))
}

/// How a failed batch ends the run (§8.1, §8.3): an outage (network, rate
/// limit, provider unavailable) is deferred, however often it recurs; a
/// rejection is final; any other error is deferred once and final when the
/// next attempt fails with the same kind before any batch completes.
fn failed(error: AppError, previous: Option<&str>, summary: AuditSummary) -> Result<AuditOutcome> {
    let outage = matches!(
        error,
        AppError::Network(_)
            | AppError::RateLimitExceeded(_)
            | AppError::ServiceNotAvailable(_)
            | AppError::QueueFull
    );
    if outage {
        return Ok(AuditOutcome::Deferred { error, summary });
    }
    if matches!(error, AppError::ModelRejected(_)) {
        return Err(error);
    }
    if previous == Some(error.error_code()) {
        return Err(AppError::ModelRejected(format!(
            "The Verifier failed the same way twice: {error}"
        )));
    }
    Ok(AuditOutcome::Deferred { error, summary })
}

/// A batch's verdicts, one per item, and what they cost.
#[derive(Debug, Default)]
struct BatchJudged {
    verdicts: Vec<Judged>,
    input_tokens: u64,
    output_tokens: u64,
    web_note: Option<String>,
}

impl BatchJudged {
    fn absorb(&mut self, other: BatchJudged) {
        self.verdicts.extend(other.verdicts);
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        if other.web_note.is_some() {
            self.web_note = other.web_note;
        }
    }
}

struct Judge<'a> {
    llm: &'a dyn LLMPort,
    claims: &'a [AuditClaim],
    units: &'a [AuditUnit],
    sizer: &'a WindowSizer<'a>,
    web: Option<&'a dyn VerifierWeb>,
}

type Judging<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<BatchJudged>> + Send + 'a>>;

impl<'a> Judge<'a> {
    /// One verdict per item. An answer cut off at its output limit, or a
    /// request the planner refuses, is sent again as two halves; a single
    /// item that still cannot be answered is "could not verify". An answer
    /// that cannot be read is asked for once more (as `structured_call`
    /// does), then every item it left without a verdict is "could not
    /// verify", with the cause.
    fn items(&'a self, items: &'a [Item]) -> Judging<'a> {
        Box::pin(async move {
            let rendered = policy::render(self.claims, self.units, items);
            let reserve = self.sizer.reserve(items.len());
            let mut judged = BatchJudged::default();
            let mut best: Vec<Option<Judged>> = vec![None; items.len()];
            let mut why = String::new();
            for _attempt in 0..2 {
                let answer = match calls::ask(self.llm, &rendered, reserve, self.web).await {
                    Ok(answer) => answer,
                    Err(AskError::Model(error)) => return Err(error),
                    Err(AskError::TooLarge) => {
                        return self
                            .halves(items, judged, || {
                                format!(
                                    "The Verifier's window of {} tokens cannot hold this claim's \
                                     section and passages with its answer.",
                                    self.sizer.llm.max_context_tokens()
                                )
                            })
                            .await;
                    }
                };
                judged.input_tokens += answer.input_tokens;
                judged.output_tokens += answer.output_tokens;
                if answer.web_note.is_some() {
                    judged.web_note = answer.web_note.clone();
                }
                if answer.cut_off && items.len() > 1 {
                    return self
                        .halves(items, judged, || {
                            String::from("The Verifier's answer was cut off at its output limit.")
                        })
                        .await;
                }
                match policy::read(&answer.text, &rendered, &answer.fetched) {
                    Ok(read) if read.iter().all(Option::is_some) => {
                        judged.verdicts = read.into_iter().flatten().collect();
                        return Ok(judged);
                    }
                    Ok(read) => {
                        why = if answer.cut_off {
                            "the answer was cut off at its output limit".into()
                        } else {
                            "the answer left out some claims".into()
                        };
                        best = read;
                    }
                    Err(error) => why = error,
                }
            }
            judged.verdicts = best
                .into_iter()
                .map(|verdict| {
                    verdict.unwrap_or_else(|| {
                        Judged::could_not_verify(format!(
                            "The Verifier's answer could not be used twice: {why}."
                        ))
                    })
                })
                .collect();
            Ok(judged)
        })
    }

    /// `items` sent as two halves; a single item gets `single()` as its
    /// "could not verify" reason instead.
    async fn halves(
        &'a self,
        items: &'a [Item],
        mut judged: BatchJudged,
        single: impl FnOnce() -> String,
    ) -> Result<BatchJudged> {
        if items.len() <= 1 {
            judged.verdicts = vec![Judged::could_not_verify(single()); items.len()];
            return Ok(judged);
        }
        let (first, second) = items.split_at(items.len() / 2);
        judged.absorb(self.items(first).await?);
        judged.absorb(self.items(second).await?);
        Ok(judged)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
