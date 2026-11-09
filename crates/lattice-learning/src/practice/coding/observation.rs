//! What the counterpart may read of a coding session.

use serde::Serialize;

use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::dto::{
    stages_of, ClockDto, CodeHintDto, CodingExampleDto, CodingRunDto, CodingSessionDto,
    CaseViewDto, CodingStage, ExampleCoverageDto,
};
use crate::practice::coding::problem::{
    CodingSetting, CodingTask, Problem, ProblemClarification, ProblemSignature,
};
use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use lattice_core::domain::learning::practice::problem_patterns::problem_pattern;

/// Whether a line was added to a file or removed from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineChangeKind {
    Added,
    Removed,
}

/// One changed line. An added line is numbered in the current file, a removed line in the file as last seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChange {
    pub path: String,
    pub kind: LineChangeKind,
    pub line: u32,
    pub text: String,
}

/// What the counterpart reads: what the learner sees in the session, plus the problem's clarifications, target
/// complexity and pattern names. It never holds the reference, the misreadings, or a hidden case the setting keeps
/// back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceObservation {
    pub session_id: String,
    pub task: CodingTask,
    pub setting: CodingSetting,
    pub stage: CodingStage,
    /// The task's stages in order, up to and including the current one.
    pub stages_reached: Vec<CodingStage>,
    pub title: String,
    pub statement: String,
    pub signature: ProblemSignature,
    pub constraints: Vec<String>,
    pub problem_examples: Vec<CaseViewDto>,
    pub clarifications: Vec<ProblemClarification>,
    pub target_complexity: String,
    pub pattern_names: Vec<String>,
    pub clock: ClockDto,
    pub files: Vec<LearningLabFile>,
    /// Whether the draft the counterpart last saw is known; `changes` is empty otherwise.
    pub has_baseline: bool,
    pub changes: Vec<LineChange>,
    pub last_run: Option<CodingRunDto>,
    pub learner_examples: Vec<CodingExampleDto>,
    pub coverage: ExampleCoverageDto,
    pub hints: Vec<CodeHintDto>,
    pub turns: Vec<LearningSimulationTurnDto>,
}

/// The lines added to and removed from `new` against `old`, by longest common subsequence of lines.
pub fn diff_lines(path: &str, old: &str, new: &str) -> Vec<LineChange> {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(before, after)| before == after)
        .count();
    let suffix = old
        .iter()
        .skip(prefix)
        .rev()
        .zip(new.iter().skip(prefix).rev())
        .take_while(|(before, after)| before == after)
        .count();
    let old_mid: Vec<&str> = old
        .iter()
        .skip(prefix)
        .take(old.len().saturating_sub(prefix + suffix))
        .copied()
        .collect();
    let new_mid: Vec<&str> = new
        .iter()
        .skip(prefix)
        .take(new.len().saturating_sub(prefix + suffix))
        .copied()
        .collect();

    // lcs[i][j] is the length of the longest common subsequence of old_mid[i..] and new_mid[j..], flattened.
    let width = new_mid.len() + 1;
    let mut lcs = vec![0u32; (old_mid.len() + 1) * width];
    let at = |lcs: &[u32], i: usize, j: usize| lcs.get(i * width + j).copied().unwrap_or(0);
    for (i, before) in old_mid.iter().enumerate().rev() {
        for (j, after) in new_mid.iter().enumerate().rev() {
            let value = if before == after {
                at(&lcs, i + 1, j + 1) + 1
            } else {
                at(&lcs, i + 1, j).max(at(&lcs, i, j + 1))
            };
            if let Some(slot) = lcs.get_mut(i * width + j) {
                *slot = value;
            }
        }
    }

    let number = |offset: usize| u32::try_from(prefix + offset + 1).unwrap_or(u32::MAX);
    let change = |kind: LineChangeKind, offset: usize, text: &str| LineChange {
        path: path.to_owned(),
        kind,
        line: number(offset),
        text: text.to_owned(),
    };
    let mut changes = Vec::new();
    let (mut i, mut j) = (0, 0);
    loop {
        match (old_mid.get(i), new_mid.get(j)) {
            (Some(before), Some(after)) if before == after => {
                i += 1;
                j += 1;
            }
            (Some(before), Some(after)) => {
                if at(&lcs, i + 1, j) >= at(&lcs, i, j + 1) {
                    changes.push(change(LineChangeKind::Removed, i, before));
                    i += 1;
                } else {
                    changes.push(change(LineChangeKind::Added, j, after));
                    j += 1;
                }
            }
            (Some(before), None) => {
                changes.push(change(LineChangeKind::Removed, i, before));
                i += 1;
            }
            (None, Some(after)) => {
                changes.push(change(LineChangeKind::Added, j, after));
                j += 1;
            }
            (None, None) => break,
        }
    }
    changes
}

fn changes_since(draft: &[LearningLabFile], seen: &[LearningLabFile]) -> Vec<LineChange> {
    let mut changes = Vec::new();
    for file in draft {
        let before = seen
            .iter()
            .find(|earlier| earlier.path == file.path)
            .map_or("", |earlier| earlier.content.as_str());
        changes.extend(diff_lines(&file.path, before, &file.content));
    }
    for gone in seen
        .iter()
        .filter(|earlier| !draft.iter().any(|file| file.path == earlier.path))
    {
        changes.extend(diff_lines(&gone.path, &gone.content, ""));
    }
    changes
}

/// What the counterpart may read: what the learner sees in `session` (already cut by the setting), plus the
/// problem's clarifications, target complexity and pattern names, and the draft's change since `seen`.
pub fn observe(
    problem: &Problem,
    session: &CodingSessionDto,
    seen: Option<&[LearningLabFile]>,
) -> WorkspaceObservation {
    let mut stages_reached = Vec::new();
    for stage in stages_of(session.task) {
        stages_reached.push(*stage);
        if *stage == session.stage {
            break;
        }
    }
    WorkspaceObservation {
        session_id: session.session_id.clone(),
        task: session.task,
        setting: session.setting,
        stage: session.stage,
        stages_reached,
        title: session.problem.title.clone(),
        statement: session.problem.statement.clone(),
        signature: session.problem.signature.clone(),
        constraints: session.problem.constraints.clone(),
        problem_examples: session.problem.examples.clone(),
        clarifications: problem.spec.clarifications.clone(),
        target_complexity: problem.spec.target_complexity.clone(),
        pattern_names: problem
            .spec
            .patterns
            .iter()
            .filter_map(|id| problem_pattern(id))
            .map(|pattern| pattern.name.to_owned())
            .collect(),
        clock: session.clock.clone(),
        files: session.draft.clone(),
        has_baseline: seen.is_some(),
        changes: seen
            .map(|seen| changes_since(&session.draft, seen))
            .unwrap_or_default(),
        last_run: session.runs.last().cloned(),
        learner_examples: session.examples.clone(),
        coverage: session.coverage.clone(),
        hints: session.hints.clone(),
        turns: session.turns.clone(),
    }
}

impl WorkspaceObservation {
    /// The number of lines of the draft file at `path`, or None when the draft has no such file.
    pub fn line_count(&self, path: &str) -> Option<usize> {
        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.content.lines().count())
    }

    /// The ids of this session the counterpart must never repeat: the session, its turns, its runs and the cases
    /// the learner has seen results for.
    pub fn internal_ids(&self) -> Vec<String> {
        let mut ids = vec![self.session_id.clone()];
        ids.extend(self.turns.iter().map(|turn| turn.id.clone()));
        ids.extend(self.hints.iter().filter_map(|hint| hint.run_id.clone()));
        if let Some(run) = &self.last_run {
            ids.push(run.run_id.clone());
            ids.extend(run.cases.iter().map(|case| case.case_id.clone()));
        }
        ids.extend(self.problem_examples.iter().map(|case| case.case_id.clone()));
        ids
    }

    /// The observation as the model reads it: text, with the draft numbered by line.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let mut line = |text: String| {
            out.push_str(&text);
            out.push('\n');
        };
        line(format!("Problem: {}", self.title));
        line(self.statement.clone());
        let params = self
            .signature
            .params
            .iter()
            .map(|param| format!("{}: {}", param.name, param.annotation))
            .collect::<Vec<_>>()
            .join(", ");
        line(format!(
            "Signature: def {}({}) -> {}",
            self.signature.function, params, self.signature.returns
        ));
        for constraint in &self.constraints {
            line(format!("Constraint: {constraint}"));
        }
        for case in &self.problem_examples {
            line(format!("Example in the statement: {}", case_text(case)));
        }
        for (index, clarification) in self.clarifications.iter().enumerate() {
            line(format!(
                "Clarification {index}: asked \"{}\", answered \"{}\"",
                clarification.question, clarification.answer
            ));
        }
        line(format!("Target complexity: {}", self.target_complexity));
        if !self.pattern_names.is_empty() {
            line(format!(
                "Techniques the problem is built around (never name them): {}",
                self.pattern_names.join(", ")
            ));
        }
        let reached = self
            .stages_reached
            .iter()
            .map(|stage| format!("{stage:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        line(format!(
            "Task: {:?}. Setting: {:?}. Stage: {:?}. Stages reached: {reached}.",
            self.task, self.setting, self.stage
        ));
        let spent = format!("{} s active", self.clock.active_ms / 1000);
        match self.clock.budget_seconds {
            Some(budget) => line(format!("Clock: {spent} of a {budget} s budget.")),
            None => line(format!("Clock: {spent}, no budget.")),
        }
        line("Draft:".to_owned());
        for file in &self.files {
            line(format!("=== {} ===", file.path));
            for (index, text) in file.content.lines().enumerate() {
                line(format!("{} | {text}", index + 1));
            }
        }
        if !self.has_baseline {
            line("Changes since you last looked: you have not looked at the draft before.".to_owned());
        } else if self.changes.is_empty() {
            line("Changes since you last looked: none.".to_owned());
        } else {
            line("Changes since you last looked:".to_owned());
            for change in &self.changes {
                let mark = match change.kind {
                    LineChangeKind::Added => "added",
                    LineChangeKind::Removed => "removed",
                };
                line(format!("{mark} {} line {}: {}", change.path, change.line, change.text));
            }
        }
        match &self.last_run {
            None => line("Last run: none yet.".to_owned()),
            Some(run) => {
                line(format!("Last run: {:?}, {:?}.", run.scope, run.status));
                for case in &run.cases {
                    line(format!("- {:?}: {}", case.status, case_text(case)));
                }
                if !run.stdout.is_empty() {
                    line(format!("Printed outside any case: {}", run.stdout));
                }
                if !run.stderr.is_empty() {
                    line(format!("Error output: {}", run.stderr));
                }
            }
        }
        for example in &self.learner_examples {
            line(format!(
                "Learner's example {}: ({}) expects {} - {:?}",
                example.ordinal,
                example.args.join(", "),
                example.expected,
                example.verdict
            ));
        }
        line(format!(
            "Examples catch {} of {} common mistakes.",
            self.coverage.caught, self.coverage.total
        ));
        if let Some(uncaught) = &self.coverage.uncaught {
            line(format!("One mistake they do not catch yet: {uncaught}"));
        }
        for hint in &self.hints {
            line(format!("Hint given ({:?}): {}", hint.rung, hint.text));
        }
        line("Transcript:".to_owned());
        for turn in &self.turns {
            let speaker = match turn.speaker {
                LearningSimulationSpeaker::Learner => "Learner",
                LearningSimulationSpeaker::Counterpart | LearningSimulationSpeaker::Coach => "You",
            };
            line(format!("{speaker}: {}", turn.content));
        }
        out
    }
}

/// A case as the learner sees it: its category, then whatever the setting shows of its input and results.
fn case_text(case: &CaseViewDto) -> String {
    let mut parts = vec![case.category.clone()];
    if let Some(args) = &case.args {
        parts.push(format!("input ({})", args.join(", ")));
    }
    if let Some(expected) = &case.expected {
        parts.push(format!("expected {expected}"));
    }
    if let Some(actual) = &case.actual {
        parts.push(format!("got {actual}"));
    }
    if let Some(printed) = case.printed.as_ref().filter(|printed| !printed.is_empty()) {
        parts.push(format!("printed {printed}"));
    }
    if let Some(error) = &case.error {
        parts.push(format!("raised {error}"));
    }
    parts.join("; ")
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::BTreeMap;

    use super::*;
    use crate::practice::coding::dto::{CodeHintRung, PublicProblemDto};
    use crate::practice::coding::problem::{
        CaseCompare, CaseStatus, CaseVisibility, Misreading, ProblemCase, ProblemDifficulty,
        ProblemSpec,
        SignatureParam,
    };

    pub const REFERENCE: &str = "def total(values):\n    return REFERENCE_SECRET_BODY\n";
    pub const MISREADING_ID: &str = "misreading-secret-id";
    pub const MISREADING_DESCRIPTION: &str = "MISREADING_SECRET_DESCRIPTION";
    pub const MISREADING_CODE: &str = "def total(values):\n    return MISREADING_SECRET_BODY\n";
    pub const HIDDEN_ARGS: &str = "[HIDDEN_SECRET_ARGS]";
    pub const SESSION_ID: &str = "5b1f0c2e-1111-4222-8333-444455556666";

    pub fn case(id: &str, visibility: CaseVisibility, args: &str) -> ProblemCase {
        ProblemCase {
            id: id.to_owned(),
            category: "an ordinary list".to_owned(),
            args: vec![args.to_owned()],
            expected: Some("0".to_owned()),
            compare: CaseCompare::Exact,
            visibility,
            scale: false,
            digest: None,
        }
    }

    pub fn problem() -> Problem {
        let pattern = lattice_core::domain::learning::practice::problem_patterns::PROBLEM_PATTERNS
            .first()
            .map(|pattern| pattern.id.to_owned())
            .into_iter()
            .collect();
        Problem {
            spec: ProblemSpec {
                title: "Total of values".to_owned(),
                statement: "Return the sum of the values.".to_owned(),
                signature: ProblemSignature {
                    function: "total".to_owned(),
                    params: vec![SignatureParam {
                        name: "values".to_owned(),
                        annotation: "list[int]".to_owned(),
                    }],
                    returns: "int".to_owned(),
                },
                constraints: vec!["At most 1000 values".to_owned()],
                clarifications: vec![
                    ProblemClarification {
                        question: "Can the list be empty?".to_owned(),
                        answer: "Yes, and its total is 0.".to_owned(),
                    },
                    ProblemClarification {
                        question: "Can values be negative?".to_owned(),
                        answer: "Yes, any integer.".to_owned(),
                    },
                ],
                language: "python".to_owned(),
                difficulty: ProblemDifficulty::Easy,
                target_complexity: "O(n) time, O(1) space".to_owned(),
                patterns: pattern,
                starter: "def total(values):\n    pass\n".to_owned(),
            },
            reference: REFERENCE.to_owned(),
            misreadings: vec![Misreading {
                id: MISREADING_ID.to_owned(),
                description: MISREADING_DESCRIPTION.to_owned(),
                code: MISREADING_CODE.to_owned(),
                killed_by: vec!["case-example-1".to_owned()],
            }],
            cases: vec![
                case("case-example-1", CaseVisibility::Example, "[1, 2]"),
                case("case-hidden-1", CaseVisibility::Hidden, HIDDEN_ARGS),
            ],
            reference_ms: BTreeMap::new(),
            bug: None,
        }
    }

    pub fn session(problem: &Problem, setting: CodingSetting, stage: CodingStage) -> CodingSessionDto {
        let task = CodingTask::Problem;
        CodingSessionDto {
            session_id: SESSION_ID.to_owned(),
            program_id: "program-1".to_owned(),
            activity_id: "activity-1".to_owned(),
            lesson_id: None,
            task,
            setting,
            settings_used: vec![setting],
            stage,
            stages: stages_of(task).to_vec(),
            problem: PublicProblemDto {
                title: problem.spec.title.clone(),
                statement: problem.spec.statement.clone(),
                signature: problem.spec.signature.clone(),
                constraints: problem.spec.constraints.clone(),
                difficulty: problem.spec.difficulty,
                language: problem.spec.language.clone(),
                examples: vec![CaseViewDto {
                    case_id: "case-example-1".to_owned(),
                    category: "an ordinary list".to_owned(),
                    visibility: CaseVisibility::Example,
                    mine: false,
                    scale: false,
                    status: CaseStatus::Passed,
                    args: Some(vec!["[1, 2]".to_owned()]),
                    expected: Some("3".to_owned()),
                    actual: None,
                    printed: None,
                    error: None,
                    elapsed_ms: None,
                }],
                starter: problem.spec.starter.clone(),
            },
            clock: ClockDto {
                active_ms: 61_000,
                open_since: None,
                budget_seconds: None,
            },
            examples: Vec::new(),
            coverage: ExampleCoverageDto {
                caught: 0,
                total: 1,
                uncaught: None,
            },
            runs: Vec::new(),
            hints: Vec::new(),
            next_rung: CodeHintRung::Nudge,
            turns: Vec::new(),
            cue_pending: false,
            draft: vec![LearningLabFile {
                path: "solution.py".to_owned(),
                content: "def total(values):\n    result = 0\n    return result\n".to_owned(),
            }],
            submitted: false,
            solution_available: false,
            worked_solution: None,
            debrief: None,
            finished: false,
            revision: 1,
        }
    }

    pub fn observation(setting: CodingSetting, stage: CodingStage) -> (Problem, WorkspaceObservation) {
        let problem = problem();
        let session = session(&problem, setting, stage);
        let observation = observe(&problem, &session, None);
        (problem, observation)
    }

    pub fn failing_run() -> CodingRunDto {
        use crate::practice::coding::dto::CodingRunScope;
        use crate::practice::practical_dto::LearningPracticalRunStatus;
        CodingRunDto {
            run_id: "run-77777777-aaaa".to_owned(),
            scope: CodingRunScope::All,
            status: LearningPracticalRunStatus::Failed,
            cases: vec![CaseViewDto {
                case_id: "case-hidden-1".to_owned(),
                category: "an empty list".to_owned(),
                visibility: CaseVisibility::Hidden,
                mine: false,
                scale: false,
                status: CaseStatus::Failed,
                args: None,
                expected: None,
                actual: None,
                printed: None,
                error: None,
                elapsed_ms: Some(1),
            }],
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: Some(5),
            reference_actual: None,
            created_at: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    fn file(content: &str) -> LearningLabFile {
        LearningLabFile {
            path: "solution.py".to_owned(),
            content: content.to_owned(),
        }
    }

    fn change(kind: LineChangeKind, line: u32, text: &str) -> LineChange {
        LineChange {
            path: "solution.py".to_owned(),
            kind,
            line,
            text: text.to_owned(),
        }
    }

    #[test]
    fn the_observation_never_holds_the_reference_or_a_misreading() {
        let (_, observation) = observation(CodingSetting::Mock, CodingStage::Code);
        let json = serde_json::to_string(&observation).unwrap_or_default();
        assert!(!json.is_empty());
        for secret in [
            "REFERENCE_SECRET_BODY",
            MISREADING_ID,
            MISREADING_DESCRIPTION,
            "MISREADING_SECRET_BODY",
            HIDDEN_ARGS,
        ] {
            assert!(!json.contains(secret), "the observation holds {secret}");
        }
        assert!(!observation.render().contains("REFERENCE_SECRET_BODY"));
    }

    #[test]
    fn the_observation_holds_clarifications_complexity_and_pattern_names() {
        let (problem, observation) = observation(CodingSetting::Coached, CodingStage::Understand);
        assert_eq!(observation.clarifications, problem.spec.clarifications);
        assert_eq!(observation.target_complexity, "O(n) time, O(1) space");
        let first = lattice_core::domain::learning::practice::problem_patterns::PROBLEM_PATTERNS
            .first()
            .map(|pattern| pattern.name.to_owned());
        assert_eq!(observation.pattern_names, first.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn the_stages_reached_end_at_the_current_stage() {
        let (_, observation) = observation(CodingSetting::Coached, CodingStage::Plan);
        assert_eq!(
            observation.stages_reached,
            vec![CodingStage::Understand, CodingStage::Examples, CodingStage::Plan]
        );
    }

    #[test]
    fn the_last_run_is_the_newest_run_as_the_learner_sees_it() {
        let problem = problem();
        let mut session = session(&problem, CodingSetting::Mock, CodingStage::Test);
        session.runs.push(failing_run());
        let observation = observe(&problem, &session, None);
        let json = serde_json::to_string(&observation).unwrap_or_default();
        assert!(json.contains("an empty list"));
        assert!(!json.contains(HIDDEN_ARGS));
    }

    #[test]
    fn a_changed_line_is_a_removal_and_an_addition() {
        let changes = diff_lines(
            "solution.py",
            "def total(values):\n    pass\n",
            "def total(values):\n    return sum(values)\n",
        );
        assert_eq!(
            changes,
            vec![
                change(LineChangeKind::Removed, 2, "    pass"),
                change(LineChangeKind::Added, 2, "    return sum(values)"),
            ]
        );
    }

    #[test]
    fn identical_drafts_have_no_diff() {
        assert!(diff_lines("solution.py", "a\nb\n", "a\nb\n").is_empty());
    }

    #[test]
    fn removed_lines_are_numbered_in_the_old_file_and_added_in_the_new() {
        let changes = diff_lines("solution.py", "a\nb\nc\nd\n", "a\nc\nd\ne\nf\n");
        assert_eq!(
            changes,
            vec![
                change(LineChangeKind::Removed, 2, "b"),
                change(LineChangeKind::Added, 4, "e"),
                change(LineChangeKind::Added, 5, "f"),
            ]
        );
    }

    #[test]
    fn the_diff_keeps_the_longest_common_subsequence() {
        let changes = diff_lines("solution.py", "a\nx\nb\ny\nc\n", "a\nb\nc\nz\n");
        let kinds: Vec<(LineChangeKind, &str)> = changes
            .iter()
            .map(|change| (change.kind, change.text.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (LineChangeKind::Removed, "x"),
                (LineChangeKind::Removed, "y"),
                (LineChangeKind::Added, "z"),
            ]
        );
    }

    #[test]
    fn a_draft_with_no_baseline_has_no_changes() {
        let problem = problem();
        let session = session(&problem, CodingSetting::Watched, CodingStage::Code);
        let observation = observe(&problem, &session, None);
        assert!(!observation.has_baseline);
        assert!(observation.changes.is_empty());
    }

    #[test]
    fn the_diff_runs_against_the_draft_last_seen() {
        let problem = problem();
        let session = session(&problem, CodingSetting::Watched, CodingStage::Code);
        let seen = [file("def total(values):\n    pass\n")];
        let observation = observe(&problem, &session, Some(&seen));
        assert!(observation.has_baseline);
        assert_eq!(
            observation.changes,
            vec![
                change(LineChangeKind::Removed, 2, "    pass"),
                change(LineChangeKind::Added, 2, "    result = 0"),
                change(LineChangeKind::Added, 3, "    return result"),
            ]
        );
    }

    #[test]
    fn a_file_new_since_seen_is_all_added_and_a_dropped_file_all_removed() {
        let mut problem = problem();
        problem.spec.starter = String::new();
        let mut session = session(&problem, CodingSetting::Watched, CodingStage::Code);
        session.draft = vec![
            file("a\n"),
            LearningLabFile {
                path: "helpers.py".to_owned(),
                content: "h1\nh2\n".to_owned(),
            },
        ];
        let seen = [
            file("a\n"),
            LearningLabFile {
                path: "old.py".to_owned(),
                content: "o1\n".to_owned(),
            },
        ];
        let observation = observe(&problem, &session, Some(&seen));
        let found: Vec<(&str, LineChangeKind, u32)> = observation
            .changes
            .iter()
            .map(|change| (change.path.as_str(), change.kind, change.line))
            .collect();
        assert_eq!(
            found,
            vec![
                ("helpers.py", LineChangeKind::Added, 1),
                ("helpers.py", LineChangeKind::Added, 2),
                ("old.py", LineChangeKind::Removed, 1),
            ]
        );
    }

    #[test]
    fn the_rendering_numbers_every_line_of_every_file() {
        let (_, observation) = observation(CodingSetting::Mock, CodingStage::Code);
        let text = observation.render();
        assert!(text.contains("solution.py"));
        assert!(text.contains("1 | def total(values):"));
        assert!(text.contains("2 |     result = 0"));
        assert!(text.contains("3 |     return result"));
    }

    #[test]
    fn the_rendering_holds_no_internal_id() {
        let problem = problem();
        let mut session = session(&problem, CodingSetting::Mock, CodingStage::Test);
        session.runs.push(failing_run());
        let observation = observe(&problem, &session, None);
        let text = observation.render();
        assert!(text.contains("an empty list"));
        for id in observation.internal_ids() {
            assert!(!text.contains(&id), "the rendering holds the id {id}");
        }
    }

    #[test]
    fn the_rendering_lists_the_techniques_without_instructions() {
        let (_, observation) = observation(CodingSetting::Coached, CodingStage::Plan);
        let text = observation.render();
        let names = observation.pattern_names.join(", ");
        assert!(text.contains(&format!("Techniques the problem is built around: {names}\n")));
        assert!(!text.contains("never name"));
    }

    #[test]
    fn the_rendering_carries_the_transcript_and_the_clarifications() {
        let problem = problem();
        let mut session = session(&problem, CodingSetting::Coached, CodingStage::Understand);
        session.turns.push(LearningSimulationTurnDto {
            id: "turn-99999999".to_owned(),
            ordinal: 0,
            speaker: LearningSimulationSpeaker::Learner,
            content: "What does it return for no values?".to_owned(),
            citations: Vec::new(),
            model_name: None,
            created_at: 5,
        });
        let observation = observe(&problem, &session, None);
        let text = observation.render();
        assert!(text.contains("What does it return for no values?"));
        assert!(!text.contains("turn-99999999"));
        assert!(text.contains("Can the list be empty?"));
        assert!(text.contains("O(n) time, O(1) space"));
        assert!(text.contains("Return the sum of the values."));
    }

    #[test]
    fn the_line_count_is_by_file() {
        let (_, observation) = observation(CodingSetting::Mock, CodingStage::Code);
        assert_eq!(observation.line_count("solution.py"), Some(3));
        assert_eq!(observation.line_count("other.py"), None);
    }
}
