//! What the counterpart may read of a coding session.

use serde::Serialize;

use crate::lab_runtime::LearningLabFile;

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

pub(crate) fn changes_since(
    draft: &[LearningLabFile],
    seen: &[LearningLabFile],
) -> Vec<LineChange> {
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

#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::BTreeMap;

    use crate::lab_runtime::LearningLabFile;
    use crate::practice::coding::problem::{
        CaseCompare, CaseStatus, CaseVisibility, Misreading, Problem, ProblemCase,
        ProblemClarification, ProblemDifficulty, ProblemSignature, ProblemSpec, SignatureParam,
    };
    use crate::practice::kinds::program::dto::{
        CaseViewDto, CodingExampleDto, ProgramRunDto, PublicProblemDto,
    };
    use crate::practice::practical_dto::LearningSimulationTurnDto;
    use crate::practice::session::dto::{ClockDto, MisreadingCoverageDto, PracticeHintDto};
    use lattice_core::domain::learning::practice::work::{Form, Setting, Stage};

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

    /// What a program session holds of the work, as the observation tests read it.
    pub struct ObservedSession {
        pub session_id: String,
        pub form: Form,
        pub setting: Setting,
        pub stage: Stage,
        pub problem: PublicProblemDto,
        pub clock: ClockDto,
        pub examples: Vec<CodingExampleDto>,
        pub coverage: MisreadingCoverageDto,
        pub runs: Vec<ProgramRunDto>,
        pub hints: Vec<PracticeHintDto>,
        pub turns: Vec<LearningSimulationTurnDto>,
        pub draft: Vec<LearningLabFile>,
    }

    pub fn session(problem: &Problem, setting: Setting, stage: Stage) -> ObservedSession {
        ObservedSession {
            session_id: SESSION_ID.to_owned(),
            form: Form::Build,
            setting,
            stage,
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
            coverage: MisreadingCoverageDto {
                caught: 0,
                total: 1,
                uncaught: None,
            },
            runs: Vec::new(),
            hints: Vec::new(),
            turns: Vec::new(),
            draft: vec![LearningLabFile {
                path: "solution.py".to_owned(),
                content: "def total(values):\n    result = 0\n    return result\n".to_owned(),
            }],
        }
    }

    pub fn failing_run() -> ProgramRunDto {
        use crate::practice::kinds::program::dto::CodingRunScope;
        use crate::practice::practical_dto::LearningPracticalRunStatus;
        ProgramRunDto {
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
    use lattice_core::domain::learning::practice::work::{Setting, Stage};

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
    fn the_diff_runs_against_the_draft_last_seen() {
        let problem = problem();
        let session = session(&problem, Setting::Watched, Stage::Make);
        let seen = [file("def total(values):\n    pass\n")];
        let changes = changes_since(&session.draft, &seen);
        assert_eq!(
            changes,
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
        let mut session = session(&problem, Setting::Watched, Stage::Make);
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
        let changes = changes_since(&session.draft, &seen);
        let found: Vec<(&str, LineChangeKind, u32)> = changes
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
}
