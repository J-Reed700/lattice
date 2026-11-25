use super::*;

fn ticket(board: &Arc<WarmupBoard>, role: WarmupRole) -> WarmupTicket {
    WarmupTicket::begin(Arc::clone(board), None, role)
}

#[test]
fn a_warm_up_dropped_without_an_outcome_is_recorded_as_skipped() {
    let board = Arc::new(WarmupBoard::default());
    drop(ticket(&board, WarmupRole::Chat));

    let snapshot = board.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].role, WarmupRole::Chat);
    assert_eq!(snapshot[0].phase, WarmupPhase::Skipped);
    assert_eq!(snapshot[0].revision, 2);
}

#[test]
fn each_record_takes_a_later_revision_and_replaces_the_role() {
    let board = Arc::new(WarmupBoard::default());
    ticket(&board, WarmupRole::Chat).finish(WarmupOutcome::Ready { degraded: None });
    ticket(&board, WarmupRole::Utility).finish(WarmupOutcome::Failed("x".to_owned()));

    let snapshot = board.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].role, WarmupRole::Chat);
    assert_eq!(snapshot[0].phase, WarmupPhase::Ready);
    assert_eq!(snapshot[0].revision, 2);
    assert_eq!(snapshot[1].role, WarmupRole::Utility);
    assert_eq!(snapshot[1].phase, WarmupPhase::Failed);
    assert_eq!(snapshot[1].error.as_deref(), Some("x"));
    assert_eq!(snapshot[1].revision, 4);
}

#[test]
fn a_warm_up_that_ends_after_a_newer_one_of_its_role_began_leaves_the_newer_record() {
    let board = Arc::new(WarmupBoard::default());
    let boot = ticket(&board, WarmupRole::Chat);
    let settings = ticket(&board, WarmupRole::Chat);

    boot.finish(WarmupOutcome::Ready { degraded: None });
    let snapshot = board.snapshot();
    assert_eq!(snapshot[0].phase, WarmupPhase::Started);
    assert_eq!(snapshot[0].revision, 2);

    settings.finish(WarmupOutcome::Failed("x".to_owned()));
    let snapshot = board.snapshot();
    assert_eq!(snapshot[0].phase, WarmupPhase::Failed);
    assert_eq!(snapshot[0].revision, 3);
}

#[test]
fn a_load_error_is_classified_once_for_every_warm_up() {
    assert_eq!(
        WarmupOutcome::from_load_error(AppError::AiModelsNotInstalled("none".to_owned())),
        WarmupOutcome::Skipped
    );
    assert_eq!(
        WarmupOutcome::from_load_error(AppError::ModelLoadFailed("m".to_owned())),
        WarmupOutcome::Failed("m".to_owned())
    );
}
