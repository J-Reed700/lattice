//! Code hint rungs and assistance policy for coding sessions
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 4.5).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodeHintRung {
    /// Rung 1: A question about what to look at. Never gives code.
    Nudge,
    /// Rung 2: The idea or structure that fits, and why. Never gives code.
    Idea,
    /// Rung 3: Steps in pseudo-code, each with its reason. Never gives runnable Python.
    Plan,
    /// Rung 4: The lines in your code that are wrong, and why. Never gives the fixed line.
    PointAtLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HelpLevel {
    FullHelp,
    HintsOnly,
    NoHelp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Help {
    CodeHint(CodeHintRung),
    WorkedSolution,
}

pub trait AssistancePolicy: Send + Sync {
    fn allows(&self, help: Help, level: HelpLevel) -> bool;
}

pub struct CodingAssistancePolicy;

impl AssistancePolicy for CodingAssistancePolicy {
    fn allows(&self, help: Help, level: HelpLevel) -> bool {
        match level {
            HelpLevel::FullHelp => true,
            HelpLevel::HintsOnly => matches!(help, Help::CodeHint(_)),
            HelpLevel::NoHelp => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckedCodeHint {
    pub rung: CodeHintRung,
    pub content: String,
    pub reason: String,
    #[serde(default)]
    pub lines: Vec<u32>,
}
