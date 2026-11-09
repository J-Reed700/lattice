//! Scorecards and evidence for coding session and interview debriefs
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 4.8).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ScorecardCriterionId {
    ProblemSolving,
    Code,
    Testing,
    Communication,
}

impl ScorecardCriterionId {
    pub const ALL: [Self; 4] = [
        Self::ProblemSolving,
        Self::Code,
        Self::Testing,
        Self::Communication,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProblemSolving => "problem_solving",
            Self::Code => "code",
            Self::Testing => "testing",
            Self::Communication => "communication",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::ProblemSolving => "Problem solving",
            Self::Code => "Code",
            Self::Testing => "Testing",
            Self::Communication => "Communication",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::ProblemSolving => {
                "Understood and clarified the problem, chose a sound approach, and reached the target complexity or explained the trade-off."
            }
            Self::Code => "Correct on the cases, clear, idiomatic.",
            Self::Testing => {
                "Examples before code, edge cases, ran the code, read the failures and fixed them."
            }
            Self::Communication => {
                "Explained the plan and the decisions, answered probes, and used hints well."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ScorecardStatus {
    Observed,
    InsufficientEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Quote,
    Lines,
    Run,
    Example,
    Hint,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceItem {
    pub kind: EvidenceKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScorecardRow {
    pub criterion_id: String,
    pub title: String,
    pub description: String,
    pub status: ScorecardStatus,
    /// 0..=4 when `status == Observed`, `None` when `InsufficientEvidence`. Silence never scores 0.
    pub points: Option<u32>,
    pub max_points: u32,
    pub evidence: Vec<EvidenceItem>,
    pub feedback: String,
    pub next_step: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Scorecard {
    pub session_id: String,
    pub rows: Vec<ScorecardRow>,
    pub grader_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criterion_id_as_str_and_metadata() {
        assert_eq!(ScorecardCriterionId::ProblemSolving.as_str(), "problem_solving");
        assert_eq!(ScorecardCriterionId::ProblemSolving.title(), "Problem solving");
        assert!(ScorecardCriterionId::ProblemSolving.description().contains("clarified"));

        assert_eq!(ScorecardCriterionId::Code.as_str(), "code");
        assert_eq!(ScorecardCriterionId::Code.title(), "Code");
        assert!(ScorecardCriterionId::Code.description().contains("idiomatic"));

        assert_eq!(ScorecardCriterionId::Testing.as_str(), "testing");
        assert_eq!(ScorecardCriterionId::Testing.title(), "Testing");
        assert!(ScorecardCriterionId::Testing.description().contains("Examples before code"));

        assert_eq!(ScorecardCriterionId::Communication.as_str(), "communication");
        assert_eq!(ScorecardCriterionId::Communication.title(), "Communication");
        assert!(ScorecardCriterionId::Communication.description().contains("hints well"));
    }

    #[test]
    fn test_serde_criterion_id() {
        for criterion in ScorecardCriterionId::ALL {
            let json = serde_json::to_string(&criterion).expect("serialization failed");
            assert_eq!(json, format!("\"{}\"", criterion.as_str()));
            let back: ScorecardCriterionId =
                serde_json::from_str(&json).expect("deserialization failed");
            assert_eq!(back, criterion);
        }
    }

    #[test]
    fn test_serde_scorecard_status_and_evidence_kind() {
        assert_eq!(
            serde_json::to_string(&ScorecardStatus::Observed).expect("serialize"),
            "\"observed\""
        );
        assert_eq!(
            serde_json::to_string(&ScorecardStatus::InsufficientEvidence).expect("serialize"),
            "\"insufficient_evidence\""
        );

        assert_eq!(
            serde_json::to_string(&EvidenceKind::Quote).expect("serialize"),
            "\"quote\""
        );
        assert_eq!(
            serde_json::to_string(&EvidenceKind::Lines).expect("serialize"),
            "\"lines\""
        );
        assert_eq!(
            serde_json::to_string(&EvidenceKind::Run).expect("serialize"),
            "\"run\""
        );
        assert_eq!(
            serde_json::to_string(&EvidenceKind::Example).expect("serialize"),
            "\"example\""
        );
        assert_eq!(
            serde_json::to_string(&EvidenceKind::Hint).expect("serialize"),
            "\"hint\""
        );
        assert_eq!(
            serde_json::to_string(&EvidenceKind::Note).expect("serialize"),
            "\"note\""
        );
    }

    #[test]
    fn test_serde_scorecard_and_row_camel_case() {
        let scorecard = Scorecard {
            session_id: "sess_123".to_string(),
            grader_model: "test-model".to_string(),
            rows: vec![ScorecardRow {
                criterion_id: "code".to_string(),
                title: "Code".to_string(),
                description: "Correct and clean".to_string(),
                status: ScorecardStatus::Observed,
                points: Some(3),
                max_points: 4,
                evidence: vec![EvidenceItem {
                    kind: EvidenceKind::Lines,
                    text: "solution lines".to_string(),
                    start_line: Some(5),
                    end_line: Some(10),
                }],
                feedback: "Good concise code".to_string(),
                next_step: "Try edge cases".to_string(),
            }],
        };

        let json = serde_json::to_string(&scorecard).expect("serialize");
        assert!(json.contains("\"sessionId\":\"sess_123\""));
        assert!(json.contains("\"graderModel\":\"test-model\""));
        assert!(json.contains("\"criterionId\":\"code\""));
        assert!(json.contains("\"maxPoints\":4"));
        assert!(json.contains("\"startLine\":5"));
        assert!(json.contains("\"endLine\":10"));
        assert!(json.contains("\"nextStep\":\"Try edge cases\""));

        let deserialized: Scorecard = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, scorecard);
    }

    #[test]
    fn test_insufficient_evidence_invariants() {
        let row = ScorecardRow {
            criterion_id: "communication".to_string(),
            title: "Communication".to_string(),
            description: "No turns".to_string(),
            status: ScorecardStatus::InsufficientEvidence,
            points: None,
            max_points: 4,
            evidence: vec![],
            feedback: "Learner did not speak".to_string(),
            next_step: "Explain your reasoning aloud".to_string(),
        };

        let json = serde_json::to_string(&row).expect("serialize");
        assert!(json.contains("\"points\":null"));
        assert!(json.contains("\"status\":\"insufficient_evidence\""));

        let deserialized: ScorecardRow = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.status, ScorecardStatus::InsufficientEvidence);
        assert_eq!(deserialized.points, None);
    }
}
