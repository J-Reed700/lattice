//! What the screen sees of a session, for every kind of work: the engine's record, cut by what the learner may see
//! now, and the kind's own part from its screen role.

use crate::practice::session::debrief::facts;
use crate::practice::session::dto::{
    ClockDto, DebriefFactsDto, MisreadingCoverageDto, MisreadingViewDto, PracticeCheckDto,
    PracticeDebriefDto, PracticeHintDto, PracticeSessionDto, RungCountDto, StageLabelDto,
    StageTimeDto, TechniqueRatingDto,
};
use crate::practice::session::hints::next_rung;
use crate::practice::session::kind::Kind;
use crate::practice::session::record::{Session, SessionCheck, SessionDebrief, SessionHint};
use crate::practice::session::seen::Seen;
use lattice_core::domain::learning::practice::work::{
    CaseStatus, Hidden, LearnerCaseVerdict, Misreading,
};
use lattice_core::shared::error::Result;

#[cfg(test)]
mod tests;

fn count(items: usize) -> u32 {
    u32::try_from(items).unwrap_or(u32::MAX)
}

fn check_view(check: &SessionCheck) -> PracticeCheckDto {
    let passed = check
        .report
        .results
        .iter()
        .filter(|result| result.status == CaseStatus::Passed)
        .count();
    PracticeCheckDto {
        check_id: check.id.clone(),
        scope: check.scope,
        status: check.status,
        passed: count(passed),
        total: count(check.report.results.len()),
        at: check.at,
    }
}

fn hint_view(hint: &SessionHint) -> PracticeHintDto {
    PracticeHintDto {
        ordinal: hint.ordinal,
        rung: hint.rung,
        text: hint.text.clone(),
        reason: hint.reason.clone(),
        stage: hint.stage,
        check_id: hint.check_id.clone(),
        in_role: hint.in_role,
        at: hint.at,
    }
}

fn caught_ids(session: &Session) -> Vec<&str> {
    session
        .learner_cases
        .iter()
        .filter(|case| case.verdict == LearnerCaseVerdict::Right)
        .flat_map(|case| case.catches.iter().map(String::as_str))
        .collect()
}

fn missed<'a>(session: &'a Session, caught: &[&str]) -> Vec<&'a Misreading> {
    session
        .exercise
        .misreadings
        .iter()
        .filter(|misreading| !caught.contains(&misreading.id.as_str()))
        .collect()
}

fn coverage(session: &Session, seen: &Seen) -> MisreadingCoverageDto {
    let missed = missed(session, &caught_ids(session));
    let total = session.exercise.misreadings.len();
    MisreadingCoverageDto {
        caught: count(total - missed.len()),
        total: count(total),
        uncaught: missed
            .first()
            .filter(|_| seen.uncaught_misreading)
            .map(|misreading| misreading.description.clone()),
    }
}

fn misreading_views(session: &Session) -> Vec<MisreadingViewDto> {
    let caught = caught_ids(session);
    session
        .exercise
        .misreadings
        .iter()
        .map(|misreading| MisreadingViewDto {
            description: misreading.description.clone(),
            caught: caught.contains(&misreading.id.as_str()),
        })
        .collect()
}

fn debrief_view(
    kind: &Kind,
    session: &Session,
    debrief: &SessionDebrief,
) -> Result<PracticeDebriefDto> {
    let counts = facts(kind, &session.counted());
    Ok(PracticeDebriefDto {
        scorecard: debrief.scorecard.clone(),
        facts: DebriefFactsDto {
            stage_ms: counts
                .stage_ms
                .into_iter()
                .map(|(stage, active_ms)| StageTimeDto { stage, active_ms })
                .collect(),
            checks: counts.checks,
            submits: counts.submits,
            hints: counts
                .hints
                .into_iter()
                .map(|(rung, count)| RungCountDto { rung, count })
                .collect(),
            settings: counts.settings,
        },
        misreadings: if kind.learner_cases.is_some() {
            misreading_views(session)
        } else {
            Vec::new()
        },
        techniques: debrief
            .techniques
            .iter()
            .map(|technique| TechniqueRatingDto {
                technique_id: technique.id.clone(),
                name: technique.name.clone(),
                family: technique.family.clone(),
                cue: technique.cue.clone(),
                rating: technique.rating,
                due_at: technique.due_at,
            })
            .collect(),
        model: debrief.model.clone(),
        work: kind.screen.debrief(session, debrief)?,
    })
}

/// Everything the screen shows of a session. A finished session shows all of it: the debrief keeps nothing back.
pub fn session_view(kind: &Kind, session: &Session) -> Result<PracticeSessionDto> {
    let row = &session.row;
    let seen = Seen::of(row);
    let submitted = row.submitted_at.is_some();
    let debrief = match (&session.debrief, row.finished_at) {
        (Some(debrief), Some(_)) => Some(debrief_view(kind, session, debrief)?),
        _ => None,
    };
    Ok(PracticeSessionDto {
        session_id: row.id.clone(),
        program_id: row.program_id.clone(),
        activity_id: row.activity_id.clone(),
        lesson_id: Some(session.lesson_id.clone()),
        kind: kind.id.as_str().to_owned(),
        title: session.exercise.brief.title.clone(),
        difficulty: session.exercise.brief.difficulty,
        form: row.form,
        setting: row.setting,
        settings_used: session.settings.clone(),
        stage: row.stage,
        stages: row
            .form
            .stages(kind.learner_cases.is_some())
            .into_iter()
            .map(|stage| StageLabelDto {
                stage,
                label: kind
                    .wording
                    .stage_label(&session.exercise.brief, row.form, stage),
            })
            .collect(),
        clock: ClockDto {
            active_ms: row.active_ms,
            open_since: row.opened_at,
            budget_seconds: row.budget_seconds,
        },
        checks: session.checks.iter().map(check_view).collect(),
        next_rung: next_rung(session.hints.iter().map(|hint| hint.rung)),
        hints: session.hints.iter().map(hint_view).collect(),
        turns: session.turns.clone(),
        cue_pending: row.pending_cue.is_some(),
        coverage: kind
            .learner_cases
            .is_some()
            .then(|| coverage(session, &seen)),
        submitted,
        solution_available: row.setting.discloses(Hidden::WorkedSolution, submitted),
        solution_shown: seen.reference,
        finished: row.finished_at.is_some(),
        revision: row.revision,
        work: kind.screen.work(session, &seen)?,
        debrief,
    })
}
