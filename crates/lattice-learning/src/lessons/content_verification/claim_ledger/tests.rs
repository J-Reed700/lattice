use super::*;

#[test]
fn a_saved_repair_from_before_narrowing_loads_as_not_narrowing() {
    let saved = serde_json::json!({
        "repairs": [{"claim_id": "c", "unit": 0, "attempt": 2, "escalated": true, "before": "A sentence."}],
    });

    let pending: Pending = serde_json::from_value(saved).unwrap();

    let record = &pending.repairs[0];
    assert!(!record.narrowing);
    assert_eq!(record.goal(), None);
    // Only a narrowing writes the flag, and it reads back as one.
    let written = serde_json::to_value(record).unwrap();
    assert!(written.get("narrowing").is_none(), "{written}");
    let narrowing = RepairRecord {
        narrowing: true,
        ..record.clone()
    };
    let written = serde_json::to_value(&narrowing).unwrap();
    assert_eq!(written["narrowing"], true);
    let read: RepairRecord = serde_json::from_value(written).unwrap();
    assert!(read.narrowing);
    assert_eq!(read.goal(), Some(Goal::KeepTheOthers));
}

#[test]
fn a_reference_change_found_at_publication_is_verified_again_not_an_error() {
    assert!(sources_changed(&AppError::InvalidInput(
        "A verified source is no longer active; verify this lesson again.".into()
    )));
    assert!(sources_changed(&AppError::InvalidInput(
        "Verification source content changed; verify this lesson again.".into()
    )));
    assert!(!sources_changed(&AppError::InvalidInput(
        "A complete verification report for this exact lesson revision is required before publication.".into()
    )));
}
