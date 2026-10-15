//! Prefix stability across turns.
//!
//! llama-server reuses its KV cache up to the first token that differs from
//! the previous request on the same slot. On a hybrid model (Qwen 3.5) a
//! difference earlier than that rolls back to a checkpoint that may sit
//! thousands of tokens before it. So the property worth holding is
//! structural: the next turn's request begins with this turn's request, byte
//! for byte, up to the newest answer.

use super::*;
use crate::application::ports::conversation_memory::PreparedMemory;

const REQUIRE_APPROVAL: &str =
    r#"{"memory_id":"req-1","kind":"constraint","quote":"Do not deploy until I approve."}"#;
const TWO_REGIONS: &str =
    r#"{"memory_id":"req-2","kind":"fact","quote":"EU-West and US-East, in that order."}"#;

/// Three exchanges of one conversation, oldest first.
fn transcript() -> Vec<SourceMessage> {
    [
        "Do not deploy until I approve.",
        "Understood. Nothing goes out without your approval.",
        "Which regions does the rollout cover?",
        "EU-West and US-East, in that order.",
        "What does the canary measure?",
        "Error rate and p95 latency over 30 minutes.",
    ]
    .iter()
    .enumerate()
    .map(|(index, content)| {
        let role = if index % 2 == 0 {
            SourceRole::User
        } else {
            SourceRole::Assistant
        };
        message(index as i64 + 1, role, content)
    })
    .collect()
}

/// One turn's request: everything before `through` is history, and the
/// turn's own recall, memory, evidence and date differ from every other turn.
struct Turn<'a> {
    history: &'a [SourceMessage],
    question: &'a str,
    recalled: &'a str,
    evidence: &'a str,
    requirements: &'a [&'a str],
    shared: &'a str,
    today: chrono::NaiveDate,
}

fn plan(memory: &MemorySnapshot, turn: &Turn<'_>, reaches_start: bool) -> ContextPlan {
    let mut request = request(Some(memory), turn.history, 32_768);
    request.current_input = turn.question;
    request.recent_reaches_start = reaches_start;
    request.today = turn.today;
    request.recall_status = Some("Automatic retrieval found older passages.");
    request.recalled = vec![SelectedPassage {
        message_id: "m1".into(),
        sequence: 1,
        role: SourceRole::User,
        text: turn.recalled.into(),
    }];
    request.document_evidence = vec![RankedEvidence {
        label: "runbook.md".into(),
        content: turn.evidence.into(),
        rank: 0.8,
    }];
    let prepared = PreparedMemory {
        revisions: None,
        mandatory: turn.requirements.iter().map(|r| r.to_string()).collect(),
        optional: vec![turn.shared.into()],
    };
    assembler()
        .assemble_with_memory(&request, Some(&prepared))
        .expect("plan")
}

fn serialized(plan: &ContextPlan) -> Vec<String> {
    plan.messages
        .iter()
        .map(|message| serde_json::to_string(message).expect("serializable"))
        .collect()
}

/// Whether `next` repeats every message of `earlier` but its final one.
fn extends(earlier: &ContextPlan, next: &ContextPlan) -> bool {
    let earlier = serialized(earlier);
    let next = serialized(next);
    let shared = earlier.len() - 1;
    next.len() > shared && earlier[..shared] == next[..shared]
}

/// Turn N asks the third question; turn N+1, the next day, asks a fourth with
/// N's exchange now in the history. Recall, saved memory, document evidence,
/// the ledger's summary and the date all differ between them, as they do
/// between real turns.
fn two_turns(reaches_start: bool) -> (ContextPlan, ContextPlan) {
    let history = transcript();
    let n = Turn {
        history: &history[..4],
        question: "What does the canary measure?",
        recalled: "Do not deploy until I approve.",
        evidence: "Canary: 5% of traffic.",
        requirements: &[REQUIRE_APPROVAL],
        shared: r#"{"memory_id":"shared-a","fact":"pager rotation"}"#,
        today: day(),
    };
    let next = Turn {
        history: &history,
        question: "Can we start the EU-West canary now?",
        recalled: "EU-West and US-East, in that order.",
        evidence: "EU-West opens at 06:00 UTC.",
        requirements: &[REQUIRE_APPROVAL, TWO_REGIONS],
        shared: r#"{"memory_id":"shared-b","fact":"change freeze"}"#,
        today: day().succ_opt().expect("a next day"),
    };
    // Consolidation after N's answer records a second requirement and refolds
    // the summary, as it does after every answer.
    let before = snapshot(Vec::new(), Some("Planning a two-region rollout."), 4);
    let mut after = snapshot(
        Vec::new(),
        Some("Planning a two-region rollout with a canary."),
        6,
    );
    after.state.memory_revision = 4;
    (
        plan(&before, &n, reaches_start),
        plan(&after, &next, reaches_start),
    )
}

#[test]
fn consecutive_turns_share_every_message_up_to_the_newest_answer() {
    let (n, next) = two_turns(true);
    assert!(n.accounting.fits() && next.accounting.fits());
    assert!(
        extends(&n, &next),
        "turn N+1 must begin with turn N's request up to its newest answer:\nN:   {:#?}\nN+1: {:#?}",
        serialized(&n),
        serialized(&next)
    );

    // What follows the shared prefix is N's exchange, verbatim, then the new
    // final message: the only text the server has to read again.
    let shared = n.messages.len() - 1;
    let rest = text_of(&next).split_off(shared);
    assert_eq!(rest.len(), 3, "{rest:#?}");
    assert_eq!(
        rest[0],
        (
            "user".to_string(),
            "What does the canary measure?".to_string()
        )
    );
    assert_eq!(rest[1].0, "assistant");
    assert_eq!(rest[2].0, "user");

    // Everything that differs per turn is in the final message, and only there.
    for plan in [&n, &next] {
        let rendered = text_of(plan);
        let (before_last, last) = rendered.split_at(rendered.len() - 1);
        let last = &last[0].1;
        for marker in [
            "[conversation history retrieval context",
            "[recorded requirements",
            render::EVIDENCE_LABEL,
            "Today is ",
        ] {
            assert!(last.contains(marker), "missing {marker} in {last}");
            assert!(
                !before_last
                    .iter()
                    .any(|(_, content)| content.contains(marker)),
                "{marker} must not sit ahead of the history"
            );
        }
    }
    assert!(text_of(&next)
        .last()
        .is_some_and(|(_, last)| last.contains("Today is 2026-10-13, Tuesday.")));
}

#[test]
fn the_summary_stays_out_while_the_window_holds_the_whole_conversation() {
    let (n, next) = two_turns(true);
    for plan in [&n, &next] {
        assert!(
            !text_of(plan)
                .iter()
                .any(|(_, content)| content.starts_with("[generated summary")),
            "every message the summary covers is carried verbatim"
        );
        assert_eq!(plan.accounting.summary, 0);
    }
}

/// The mirror case, and the one change this layout cannot absorb: once older
/// turns have left the window the summary is carried, and consolidation
/// refolds it after every answer, so the request diverges at its second
/// message. Fixing that needs a summary that changes only when the window's
/// start moves (see the 2026-10-12 audit notes, track B1).
#[test]
fn a_refolded_summary_ahead_of_the_history_breaks_the_shared_prefix() {
    let (n, next) = two_turns(false);
    let (n_text, next_text) = (text_of(&n), text_of(&next));
    assert!(n_text[1].1.starts_with("[generated summary"));
    assert_eq!(n_text[0], next_text[0], "the system policy is stable");
    assert_ne!(n_text[1], next_text[1]);
    assert!(!extends(&n, &next));
}

#[test]
fn the_summary_returns_when_the_window_no_longer_reaches_the_first_message() {
    // Long turns on a small window: the oldest ones cannot all be carried.
    let recent: Vec<SourceMessage> = (1..=40)
        .map(|index| {
            let role = if index % 2 == 1 {
                SourceRole::User
            } else {
                SourceRole::Assistant
            };
            message(
                index,
                role,
                &format!("Turn {index}: {}", "detail ".repeat(60)),
            )
        })
        .collect();
    let memory = snapshot(Vec::new(), Some("Earlier work summarized."), 40);
    let mut base = request(Some(&memory), &recent, 8_192);
    base.recent_reaches_start = true;
    let plan = assembler().assemble(&base).expect("plan");

    let rendered = text_of(&plan);
    assert!(
        !rendered
            .iter()
            .any(|(_, content)| content.starts_with("Turn 1:")),
        "the window should have left the first turn out"
    );
    assert_eq!(rendered[1].0, "assistant");
    assert!(rendered[1].1.starts_with("[generated summary"));
    assert!(plan.accounting.summary > 0);
}
