//! Shared builders for the claim-review tests.

mod ledger_tests;
mod rules_tests;
mod trust_tests;

use super::*;

const MODEL: &str = "qwen3.5-27b";
const WINDOW: usize = 32_768;
const QUOTE: &str = "Water boils at 100 °C at sea level.";
const FIELD_TEXT: &str = "Heat it. Water boils at 100 °C at sea level. Then cool it.";

fn machine(at: i64) -> EventProvenance {
    EventProvenance {
        actor: Actor::Machine,
        at,
        job_id: Some("job-1".into()),
        app_version: "1.0.0".into(),
        checker_model: Some(MODEL.into()),
    }
}

fn user(at: i64) -> EventProvenance {
    EventProvenance {
        actor: Actor::User,
        job_id: None,
        ..machine(at)
    }
}

fn location(quote: &str) -> ClaimLocation {
    ClaimLocation::locate(0, UnitKind::Block, "body", quote, FIELD_TEXT)
}

fn claim(id: &str) -> ClaimLedgerEntry {
    ClaimLedgerEntry::extracted(
        ClaimId(id.into()),
        machine(1),
        location(QUOTE),
        "Water boils at 100 °C at sea level.".into(),
    )
    .unwrap()
}

fn seen(version: &str) -> EvidenceSeen {
    EvidenceSeen {
        source_version_id: version.into(),
        start_byte: 0,
        end_byte: 40,
        text_sha256: text_sha256(version),
    }
}

/// A local check against `version`; `kind: None` is Supported.
fn local_check(kind: Option<VerdictKind>, version: &str) -> Check {
    let evidence = vec![seen(version)];
    Check {
        checker: Checker::Local {
            model: MODEL.into(),
        },
        result: match kind {
            None => CheckResult::Supported {
                source_version_id: version.into(),
                supporting_quote: "boils at 100 °C".into(),
            },
            Some(kind) => CheckResult::Failed { kind },
        },
        reason: format!("compared {version}"),
        fingerprint: EvidenceFingerprint::of("strict-v1", MODEL, &evidence),
        evidence,
        window_tokens: WINDOW,
    }
}

fn checked(kind: Option<VerdictKind>, version: &str) -> ClaimEvent {
    ClaimEvent::Checked(local_check(kind, version))
}

fn verifier(kind: Option<VerdictKind>) -> ClaimEvent {
    let mut check = local_check(kind, "v1");
    check.checker = Checker::Verifier {
        model: "claude-verifier".into(),
    };
    ClaimEvent::Checked(check)
}

fn rewrite(attempt: u8, escalated: bool, outcome: RepairOutcome) -> ClaimEvent {
    ClaimEvent::RepairAttempted {
        attempt,
        escalated,
        before: QUOTE.into(),
        after: (outcome == RepairOutcome::Rewritten).then(|| "Water boils at 100 °C.".into()),
        outcome,
    }
}

fn act(action: UserAction, note: Option<&str>) -> ClaimEvent {
    ClaimEvent::UserAction {
        action,
        note: note.map(str::to_string),
    }
}

fn confirm() -> UserAction {
    UserAction::Confirm {
        bound: HumanBinding::new(QUOTE, vec!["v1".into()]),
    }
}

/// A claim that failed twice on the same evidence and waits for the user.
fn queued_claim(id: &str, kind: VerdictKind) -> ClaimLedgerEntry {
    let mut entry = claim(id);
    entry
        .append(machine(2), checked(Some(kind.clone()), "v1"))
        .unwrap();
    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    entry
        .append(machine(4), checked(Some(kind.clone()), "v1"))
        .unwrap();
    entry
        .append(machine(5), rewrite(2, true, RepairOutcome::Rewritten))
        .unwrap();
    entry.append(machine(6), checked(Some(kind), "v1")).unwrap();
    entry
}

fn verified_claim(id: &str) -> ClaimLedgerEntry {
    let mut entry = claim(id);
    entry.append(machine(2), checked(None, "v1")).unwrap();
    entry
}
