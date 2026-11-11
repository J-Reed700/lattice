//! What the counterpart may read of a session, in the engine's terms: every block that shows the work comes from
//! the kind's roles.

use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use crate::practice::session::kind::Kind;
use lattice_core::domain::learning::practice::work::{
    Brief, Case, CaseResult, CaseStatus, Change, ChangeKind, CheckScope, CheckStatus, Form,
    LearnerCaseVerdict, Piece, Rung, Setting, Stage,
};
use lattice_core::shared::error::Result;

#[cfg(test)]
mod tests;

/// The session's clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockView {
    pub active_ms: u64,
    pub budget_seconds: Option<u32>,
}

/// A case of a check, its result once it ran, and whether a learner may look inside it.
#[derive(Debug, Clone)]
pub struct CaseView {
    pub case: Case,
    pub result: Option<CaseResult>,
    pub open: bool,
}

/// One check as the learner sees it.
#[derive(Debug, Clone)]
pub struct CheckView {
    pub id: String,
    pub scope: CheckScope,
    pub status: CheckStatus,
    pub cases: Vec<CaseView>,
    pub detail: serde_json::Value,
}

/// A case the learner wrote and what the reference said about it.
#[derive(Debug, Clone)]
pub struct LearnerCaseView {
    pub ordinal: u32,
    pub body: serde_json::Value,
    pub verdict: LearnerCaseVerdict,
}

/// How many of the proven misreadings the learner's cases catch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub caught: u32,
    pub total: u32,
    pub uncaught: Option<String>,
}

/// A hint already given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintView {
    pub rung: Rung,
    pub text: String,
    pub check_id: Option<String>,
}

/// What the counterpart reads: what the learner sees in the session, already cut by the setting.
#[derive(Debug, Clone)]
pub struct Observation {
    pub session_id: String,
    pub form: Form,
    pub setting: Setting,
    pub stage: Stage,
    /// The form's stages in order, up to and including the current one.
    pub stages_reached: Vec<Stage>,
    pub brief: Brief,
    /// The cases shown with the brief.
    pub shown: Vec<Case>,
    pub clock: ClockView,
    pub piece: Piece,
    /// Whether the draft the counterpart last saw is known; `changes` is empty otherwise.
    pub has_baseline: bool,
    pub changes: Vec<Change>,
    pub last_check: Option<CheckView>,
    pub learner_cases: Vec<LearnerCaseView>,
    pub coverage: Coverage,
    pub hints: Vec<HintView>,
    pub turns: Vec<LearningSimulationTurnDto>,
}

fn push_line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

impl Observation {
    /// The ids of this session the counterpart must never repeat: the session, its turns, its checks and the cases
    /// the learner has seen results for.
    pub fn internal_ids(&self) -> Vec<String> {
        let mut ids = vec![self.session_id.clone()];
        ids.extend(self.turns.iter().map(|turn| turn.id.clone()));
        ids.extend(self.hints.iter().filter_map(|hint| hint.check_id.clone()));
        if let Some(check) = &self.last_check {
            ids.push(check.id.clone());
            ids.extend(check.cases.iter().map(|view| view.case.id.clone()));
        }
        ids.extend(self.shown.iter().map(|case| case.id.clone()));
        ids
    }

    /// The observation as the model reads it: text, with the blocks in a fixed order.
    pub fn render(&self, kind: &Kind) -> Result<String> {
        let wording = &kind.wording;
        let showing = &kind.showing;
        let brief = &self.brief;
        let mut out = String::new();
        out.push_str(&showing.brief(brief, &self.shown)?);
        let reached = self
            .stages_reached
            .iter()
            .map(|stage| wording.stage_label(brief, self.form, *stage))
            .collect::<Vec<_>>()
            .join(", ");
        push_line(
            &mut out,
            &format!(
                "Task: {}. Setting: {}. Stage: {}. Stages reached: {reached}.",
                wording.form_label(self.form),
                self.setting.label(),
                wording.stage_label(brief, self.form, self.stage)
            ),
        );
        let spent = format!("{} s active", self.clock.active_ms / 1000);
        match self.clock.budget_seconds {
            Some(budget) => push_line(&mut out, &format!("Clock: {spent} of a {budget} s budget.")),
            None => push_line(&mut out, &format!("Clock: {spent}, no budget.")),
        }
        push_line(&mut out, "Draft:");
        out.push_str(&showing.piece(brief, &self.piece)?);
        self.render_changes(kind, &mut out)?;
        self.render_check(kind, &mut out)?;
        if let Some(learner_cases) = &kind.learner_cases {
            for case in &self.learner_cases {
                push_line(
                    &mut out,
                    &format!(
                        "Learner's example {}: {} - {}",
                        case.ordinal,
                        learner_cases.show(brief, &case.body)?,
                        case.verdict.label()
                    ),
                );
            }
            push_line(
                &mut out,
                &format!(
                    "Examples catch {} of {} common mistakes.",
                    self.coverage.caught, self.coverage.total
                ),
            );
            if let Some(uncaught) = &self.coverage.uncaught {
                push_line(
                    &mut out,
                    &format!("One mistake they do not catch yet: {uncaught}"),
                );
            }
        }
        for hint in &self.hints {
            push_line(
                &mut out,
                &format!(
                    "Hint given ({}): {}",
                    wording.rung_label(hint.rung),
                    hint.text
                ),
            );
        }
        push_line(&mut out, "Transcript:");
        for turn in &self.turns {
            let speaker = match turn.speaker {
                LearningSimulationSpeaker::Learner => "Learner",
                LearningSimulationSpeaker::Counterpart | LearningSimulationSpeaker::Coach => "You",
            };
            push_line(&mut out, &format!("{speaker}: {}", turn.content));
        }
        Ok(out)
    }

    fn render_changes(&self, kind: &Kind, out: &mut String) -> Result<()> {
        if !self.has_baseline {
            push_line(
                out,
                "Changes since you last looked: you have not looked at the draft before.",
            );
        } else if self.changes.is_empty() {
            push_line(out, "Changes since you last looked: none.");
        } else {
            push_line(out, "Changes since you last looked:");
            for change in &self.changes {
                let mark = match change.kind {
                    ChangeKind::Added => "added",
                    ChangeKind::Removed => "removed",
                };
                let part = kind.parts.name(&self.brief, &self.piece, &change.part)?;
                push_line(out, &format!("{mark} {part}: {}", change.text));
            }
        }
        Ok(())
    }

    fn render_check(&self, kind: &Kind, out: &mut String) -> Result<()> {
        let noun = kind.wording.check_noun();
        let Some(check) = &self.last_check else {
            push_line(out, &format!("Last {noun}: none yet."));
            return Ok(());
        };
        push_line(
            out,
            &format!(
                "Last {noun}: {}, {}.",
                kind.wording.scope_label(check.scope),
                check.status.label()
            ),
        );
        for view in &check.cases {
            let status = view
                .result
                .as_ref()
                .map_or(CaseStatus::NotRun, |result| result.status);
            let case =
                kind.showing
                    .case(&self.brief, &view.case, view.result.as_ref(), view.open)?;
            push_line(out, &format!("- {}: {case}", status.label()));
        }
        for note in kind.showing.check_notes(&self.brief, &check.detail)? {
            push_line(out, &note);
        }
        Ok(())
    }
}
