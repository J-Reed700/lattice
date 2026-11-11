use super::*;
use lattice_core::domain::learning::practice::work::{Form, Piece, Setting, Stage};

fn row(setting: Setting, submitted: bool, solution_seen: bool, finished: bool) -> SessionRow {
    SessionRow {
        id: "session-1".to_owned(),
        program_id: "program-1".to_owned(),
        activity_id: "activity-1".to_owned(),
        form: Form::Build,
        setting,
        stage: Stage::Make,
        budget_seconds: None,
        active_ms: 0,
        opened_at: None,
        draft: Piece::new(serde_json::json!("draft")),
        pending_cue: None,
        submitted_at: submitted.then_some(5),
        solution_seen_at: solution_seen.then_some(6),
        finished_at: finished.then_some(7),
        revision: 1,
        created_at: 0,
    }
}

#[test]
fn the_setting_the_submit_the_reveal_and_the_end_decide_what_is_seen() {
    for setting in [Setting::Coached, Setting::Mock, Setting::Watched] {
        for submitted in [false, true] {
            for solution_seen in [false, true] {
                for finished in [false, true] {
                    let seen = Seen::of(&row(setting, submitted, solution_seen, finished));
                    let coached = setting == Setting::Coached;
                    let context = format!(
                        "{setting:?} submitted {submitted} revealed {solution_seen} finished {finished}"
                    );
                    assert_eq!(seen.failing_case, finished || coached, "{context}");
                    assert_eq!(seen.uncaught_misreading, finished || coached, "{context}");
                    assert_eq!(
                        seen.reference,
                        finished || (solution_seen && coached && submitted),
                        "{context}"
                    );
                }
            }
        }
    }
}
