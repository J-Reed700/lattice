//! Fixtures of the interview answer tests, shared with lattice-desktop's live evals.

use crate::{
    dto::{LearningSourceVersionDto, LearningSourceVersionSummaryDto},
    practical_dto::{
        LearningInterviewPlanDto, LearningInterviewQuestionDto, LearningInterviewRoundKind,
    },
};
use serde_json::{json, Value};

pub fn source() -> LearningSourceVersionDto {
    let text = "An idempotent operation has the same intended effect when repeated. This does not mean that repeated requests must return the same response.";
    LearningSourceVersionDto {
        source_id: "source".into(),
        version: LearningSourceVersionSummaryDto {
            id: "version".into(),
            version_number: 1,
            title: "Idempotent operations".into(),
            publisher: Some("Specification publisher".into()),
            resolved_url: Some("https://spec.example/idempotency".into()),
            excerpt: text.into(),
            content_sha256: "digest".into(),
            word_count: text.split_whitespace().count(),
            truncated: false,
            extraction_version: "test".into(),
            acquired_at: 0,
            snapshot_hash: None,
        },
        full_text: text.into(),
        usage: vec![],
    }
}

pub fn plan() -> LearningInterviewPlanDto {
    LearningInterviewPlanDto {
        company_name: "Acme".into(),
        company_url: None,
        target_role: "Engineer".into(),
        round_kind: LearningInterviewRoundKind::TechnicalDeepDive,
        company_summary: "Practice constraints are hypothetical.".into(),
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        researched_source_version_ids: vec![],
        questions: vec![LearningInterviewQuestionDto {
            id: "question".into(),
            round: LearningInterviewRoundKind::TechnicalDeepDive,
            question: "Assume an upload response can be lost. How would you make retrying safe?"
                .into(),
            company_rationale: "Tests reliable retry reasoning.".into(),
            follow_up_probes: vec!["How would you verify it?".into()],
            evaluation_signals: vec!["Explains repeated effects and lost responses.".into()],
        }],
    }
}

pub fn standard() -> Value {
    json!({
        "referenceAnswer":"Preserve the intended effect across retries; the returned response can differ. Check a lost-response retry against the same intended effect.",
        "keyPoints":[
            {"point":"Idempotency is about the intended effect, not identical responses.","evidence":"sourced","citations":[{
                "sourceId":"source","versionId":"version","quote":"An idempotent operation has the same intended effect when repeated."
            }]},
            {"point":"For this exercise verify that retrying after the stipulated lost response preserves the intended effect.","evidence":"reasoning","citations":[]}
        ],
        "acceptableAlternatives":["Any mechanism that preserves the specified effect under the stated retry conditions."],
        "commonMistakes":["Equating idempotent effects with identical responses."],
        "scoringCriteria":[
            {"criterion":"Semantics","fullCredit":"Explains repeated intended effects.","partialCredit":"Mentions safe retries without their effect."},
            {"criterion":"Verification","fullCredit":"Checks a retry following a lost response.","partialCredit":"Tests a retry without the lost response."}
        ],"evidenceGaps":[]
    })
}
