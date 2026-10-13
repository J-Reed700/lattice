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
    // Consolidation after N's answer records a second requirement. The
    // summary differs too, as it would after a refold: while the window holds
    // the whole conversation it plays no part.
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

/// Long turns, more than a small window holds, oldest first.
fn long_transcript(count: i64) -> Vec<SourceMessage> {
    (1..=count)
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
        .collect()
}

/// Memory whose summary ends at `through`, with extraction at `watermark`.
fn summarized(through: i64, watermark: i64) -> MemorySnapshot {
    let mut memory = snapshot(Vec::new(), Some("Earlier work summarized."), watermark);
    memory.summary_through_sequence = through;
    memory
}

/// One turn on a window that cannot hold the whole conversation.
fn small_plan(
    memory: &MemorySnapshot,
    recent: &[SourceMessage],
    question: &str,
    today: chrono::NaiveDate,
) -> ContextPlan {
    let mut request = request(Some(memory), recent, 8_192);
    request.recent_reaches_start = true;
    request.current_input = question;
    request.today = today;
    assembler().assemble(&request).expect("plan")
}

/// The sequence of the oldest turn carried verbatim.
fn window_start(plan: &ContextPlan) -> Option<i64> {
    text_of(plan).iter().find_map(|(_, content)| {
        content
            .strip_prefix("Turn ")?
            .split(':')
            .next()?
            .parse()
            .ok()
    })
}

/// Once older turns have left the window, the summary heads the history and
/// the window opens right after the last message it covers. Consolidation
/// leaves both alone while the window has room, so the next request repeats
/// this one up to its newest answer, summary included.
#[test]
fn consecutive_turns_past_the_window_share_every_message_up_to_the_newest_answer() {
    let transcript = long_transcript(40);
    let n = small_plan(
        &summarized(24, 38),
        &transcript[..38],
        "What did turn 39 decide?",
        day(),
    );
    // N's answer was extracted after it was saved, and the summary kept.
    let mut after = summarized(24, 40);
    after.state.memory_revision = 4;
    let next = small_plan(
        &after,
        &transcript,
        "And what follows from it?",
        day().succ_opt().expect("a next day"),
    );

    for plan in [&n, &next] {
        assert!(plan.accounting.fits());
        assert_eq!(
            window_start(plan),
            Some(25),
            "the window opens where the summary ends"
        );
        assert!(text_of(plan)[1].1.starts_with("[generated summary"));
        assert!(plan.summary_refold.is_none(), "{:?}", plan.summary_refold);
    }
    // There was room to reach further back. Without the anchor the window
    // takes it, and its start then moves with every turn added.
    let unanchored = small_plan(
        &summarized(0, 38),
        &transcript[..38],
        "What did turn 39 decide?",
        day(),
    );
    assert!(window_start(&unanchored).is_some_and(|start| start < 25));

    assert!(
        extends(&n, &next),
        "turn N+1 must begin with turn N's request up to its newest answer:\nN:   {:#?}\nN+1: {:#?}",
        text_of(&n),
        text_of(&next)
    );
    // Then N's exchange, verbatim, and the new final message.
    let rest = text_of(&next).split_off(n.messages.len() - 1);
    assert_eq!(rest.len(), 3, "{rest:#?}");
    assert!(rest[0].1.starts_with("Turn 39:"));
    assert!(rest[1].1.starts_with("Turn 40:"));
}

/// When the window no longer fits after the summary it has to start later,
/// and the turns in between are carried by nothing. The plan asks for the
/// summary to cover them, and a little more: the newest half of the window
/// stays verbatim, so the requests after the refold extend one another again.
#[test]
fn the_summary_refolds_when_the_window_start_moves() {
    let transcript = long_transcript(40);
    let moved = small_plan(&summarized(4, 40), &transcript, "Next question", day());
    let start = window_start(&moved).expect("a window");
    assert!(start > 5, "the anchored window no longer fits: {start}");
    let refold = moved
        .summary_refold
        .expect("turns between the summary and the window are carried by nothing");
    assert_eq!(refold.window_start_sequence, start);
    assert!(refold.through_sequence >= start - 1 && refold.through_sequence < 40);
    assert_eq!(
        refold.through_sequence % 2,
        0,
        "the next window opens on a user turn"
    );
    assert!(
        (40 - refold.through_sequence) * 2 <= 40 - start + 1,
        "at most half the window stays verbatim: {refold:?}"
    );

    // Consolidation refolds through that point. The next request opens right
    // after it, asks for nothing, and the one after extends it.
    let after = small_plan(
        &summarized(refold.through_sequence, 42),
        &long_transcript(42),
        "Another question",
        day(),
    );
    assert_eq!(window_start(&after), Some(refold.through_sequence + 1));
    assert!(after.summary_refold.is_none());
    let following = small_plan(
        &summarized(refold.through_sequence, 44),
        &long_transcript(44),
        "And another",
        day(),
    );
    assert!(following.summary_refold.is_none());
    assert!(extends(&after, &following));
}

/// A window that holds every turn after the summary but cannot reach back to
/// it, because the turns in between were never offered, still asks for the
/// refold: it would otherwise slide by one turn with every request.
#[test]
fn candidates_that_start_past_the_summary_ask_for_a_refold() {
    // Messages 31 to 40, the newest five short: half the window's tokens
    // would keep seven of them, half its messages keeps five.
    let mut transcript = long_transcript(40).split_off(30);
    for message in transcript.iter_mut().skip(5) {
        message.content = format!("Turn {}: short.", message.sequence);
    }
    let memory = summarized(4, 40);
    let mut request = request(Some(&memory), &transcript, 32_768);
    request.recent_reaches_start = false;
    let plan = assembler().assemble(&request).expect("plan");
    assert_eq!(window_start(&plan), Some(31));
    let refold = plan.summary_refold.expect("turns 5 to 30 were never read");
    assert_eq!(refold.window_start_sequence, 31);
    // The candidate list ended the window, so the room it leaves is counted
    // in messages too, and the next window opens on the user turn at 37.
    assert_eq!(refold.through_sequence, 36, "{refold:?}");
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
