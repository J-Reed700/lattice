//! A course's Verifier audit (lesson claim review §14): the opt-in switch
//! with its consent, and one lesson's audit as its card shows it.
use super::*;
use lattice_core::domain::models::model_ref::ModelRef;

/// The audit switch of one course, with what turning it on would send where.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningVerifierAuditDto {
    pub program_id: String,
    pub on: bool,
    /// The Verifier may search the web during this course's audits.
    pub web_search: bool,
    /// When sending lesson text to `consentProvider` was confirmed.
    pub consent_at: Option<i64>,
    /// The cloud provider consented to (`openai`, `anthropic`, `google`).
    pub consent_provider: Option<String>,
    /// The Verifier set in Settings → Models; null when none is.
    pub verifier: Option<ModelRef>,
    /// Where an audit sends lesson text, in words: "Anthropic (claude-x)",
    /// "your llama.cpp server (qwen3.5-27b)". Null when no Verifier is set.
    pub destination: Option<String>,
    /// The provider whose name the consent dialog must show and the user
    /// must confirm before the switch turns on; null for a Verifier that is
    /// not a cloud model.
    pub consent_required_for: Option<String>,
    /// Why an audit that is on would refuse before sending: consent for
    /// another provider ("Consent was for Anthropic; confirm again for
    /// OpenAI"), or no Verifier set.
    pub blocked: Option<String>,
    pub usage: LearningAuditUsageDto,
}

/// Tokens a course's audits sent and received, as the provider reported.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningAuditUsageDto {
    pub audits: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningLessonAuditState {
    Running,
    Completed,
    Stopped,
}

/// A lesson's latest audit: "Audit: 7 batches · 118,420 tokens sent ·
/// 6,140 received · Anthropic".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLessonAuditDto {
    pub lesson_id: String,
    pub state: LearningLessonAuditState,
    /// The Verifier that audited, as its checks name it (`cloud:anthropic/claude-x`).
    pub verifier_model: String,
    pub batches_done: i64,
    /// Batches planned by the latest pass; 0 before the first batch.
    pub batches_planned: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// The last batch's error when it failed, sanitized.
    pub last_error: Option<String>,
    /// Why web search was not used, when it was allowed.
    pub web_note: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningAuditOutcome {
    /// Every claim carries a current Verifier verdict.
    Completed,
    /// The audit refused or stopped: `message` says why, `fix` what to do.
    Stopped,
    /// A provider or network failure; the next audit continues where this
    /// one stopped.
    Deferred,
}

/// What "Audit this lesson now" did.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLessonAuditResultDto {
    pub outcome: LearningAuditOutcome,
    pub message: Option<String>,
    pub fix: Option<String>,
    /// Claims the Verifier disputes, now waiting in the review.
    pub disputes: i64,
    /// Whether the audit lets the lesson publish; a dispute sends a ready
    /// lesson back to Needs review.
    pub publishable: bool,
    pub audit: Option<LearningLessonAuditDto>,
}
