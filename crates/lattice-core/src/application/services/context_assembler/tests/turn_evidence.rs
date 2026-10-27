//! Evidence the caller renders into the current message.
//!
//! The 2026-10-13 shipped-path run charged that evidence as part of the
//! message, which nothing may cut, so a turn whose evidence outgrew the room
//! left after the tools and the policy was refused before generation: 31 of
//! 34 turns at an 8,192-token window, and 3 per arm at 16,384. These hold the
//! fix: the evidence gives way, lowest-ranked first, and only the policy, the
//! tools and the bare message can refuse a turn.

use super::*;

const BARE: &str = "Question: What is the current budget?";

/// Tool schemas of a small local model's turn, in tokens.
const TOOL_TOKENS: usize = 1_000;

/// A system policy that, with the memory rules and the tools above, comes to
/// about 2,100 tokens.
fn policy() -> String {
    "rule ".repeat(540)
}

fn passage(rank: usize) -> String {
    format!(
        "[{rank}] Document: doc-{rank}.md\nContent: {}",
        "fact ".repeat(200)
    )
}

/// A message carrying `count` ranked passages, best first, and its renderer.
fn passages(count: usize) -> impl Fn(usize) -> String + Send + Sync {
    move |keep: usize| {
        let kept: Vec<String> = (1..=keep.min(count)).map(passage).collect();
        format!("{}\n\n{BARE}", kept.join("\n\n"))
    }
}

fn turn<'a>(
    policy: &'a str,
    capacity: usize,
    evidence: Option<InlineEvidence<'a>>,
) -> ContextRequest<'a> {
    let mut request = request(None, &[], capacity);
    request.system_policy = policy;
    request.current_input = BARE;
    request.tool_schema_tokens = TOOL_TOKENS;
    request.inline_evidence = evidence;
    request
}

/// The message with its evidence, charged as fixed.
fn charged_whole(policy: &str, capacity: usize, message: &str) -> PromptBudgetExceeded {
    let mut request = turn(policy, capacity, None);
    request.current_input = message;
    match assembler().assemble(&request) {
        Err(AppError::InvalidInput(text)) => {
            let numbers: Vec<usize> = text
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|part| part.parse().ok())
                .collect();
            PromptBudgetExceeded {
                required: numbers.first().copied().unwrap_or_default(),
                available: numbers.get(1).copied().unwrap_or_default(),
            }
        }
        other => panic!("expected the old refusal, got {other:?}"),
    }
}

#[test]
fn an_8k_window_cuts_the_lowest_ranked_evidence_instead_of_refusing_the_turn() {
    let policy = policy();
    let render = passages(24);
    let fixed = assembler().fixed_parts(&policy, BARE, TOOL_TOKENS, day());
    assert!(
        (2_000..=2_200).contains(&(fixed.system + fixed.tools)),
        "policy and tools: {}",
        fixed.system + fixed.tools
    );

    // Charged whole, the message is refused.
    let refused = charged_whole(&policy, 8_192, &render(24));
    assert_eq!(refused.available, 5_734);
    assert!((5_847..=8_691).contains(&refused.required), "{refused:?}");

    let plan = assembler()
        .assemble(&turn(
            &policy,
            8_192,
            Some(InlineEvidence {
                items: 24,
                render: &render,
            }),
        ))
        .expect("the evidence gives way instead");
    let accounting = &plan.accounting;
    assert!(accounting.fits(), "{accounting:?}");
    assert_eq!(accounting.fixed_policy, fixed.system);
    assert_eq!(accounting.current_input, fixed.current_input);
    let kept = accounting.inline_evidence_kept;
    assert!((1..24).contains(&kept), "kept {kept}");
    // Cut to fit, not cut to nothing: one more passage would not have fit.
    assert!(
        accounting.input_budget - accounting.total_input < 300,
        "{} tokens left unused",
        accounting.input_budget - accounting.total_input
    );
    assert!(accounting
        .evicted
        .iter()
        .any(|name| name.starts_with("overflow:turn-evidence")));

    let (_, last) = last_message(&plan);
    assert!(last.contains("[1] Document: doc-1.md"), "the best is kept");
    assert!(
        !last.contains("[24] Document"),
        "the lowest-ranked goes first"
    );
    assert!(last.contains(&format!("[{kept}] Document")));
    assert!(!last.contains(&format!("[{}] Document", kept + 1)));
    assert!(last.ends_with(BARE));
}

#[test]
fn a_16k_window_assembles_the_turns_the_baseline_refused() {
    let policy = policy();
    // About 11,600 and 13,250 tokens charged whole, against a budget of
    // 11,468.
    for count in [36, 42] {
        let render = passages(count);
        let refused = charged_whole(&policy, 16_384, &render(count));
        assert_eq!(refused.available, 11_468);
        assert!(
            (11_606..=13_250).contains(&refused.required),
            "{count}: {refused:?}"
        );

        let plan = assembler()
            .assemble(&turn(
                &policy,
                16_384,
                Some(InlineEvidence {
                    items: count,
                    render: &render,
                }),
            ))
            .expect("plan");
        assert!(plan.accounting.fits(), "{:?}", plan.accounting);
        assert!(plan.accounting.inline_evidence_kept < count);
        assert!(plan.accounting.inline_evidence_kept > count / 2);
    }
}

#[test]
fn evidence_that_fits_is_sent_whole_and_the_request_is_unchanged() {
    let recent = vec![
        message(1, SourceRole::User, "Which regions does the rollout cover?"),
        message(2, SourceRole::Assistant, "EU-West and US-East."),
    ];
    let memory = snapshot(Vec::new(), Some("Planning a two-region rollout."), 2);
    let render = passages(5);
    let whole = render(5);

    let mut inline = request(Some(&memory), &recent, 32_768);
    inline.current_input = BARE;
    inline.tool_schema_tokens = TOOL_TOKENS;
    inline.recall_status = Some("Automatic retrieval found older passages.");
    inline.recalled = vec![SelectedPassage {
        message_id: "m1".into(),
        sequence: 1,
        role: SourceRole::User,
        text: "Which regions does the rollout cover?".into(),
    }];
    inline.document_evidence = vec![RankedEvidence {
        label: "runbook.md".into(),
        content: "Canary: 5% of traffic.".into(),
        rank: 0.8,
    }];
    inline.inline_evidence = Some(InlineEvidence {
        items: 5,
        render: &render,
    });
    let with_parts = assembler().assemble(&inline).expect("plan");

    // The same turn as it was assembled before: the evidence inside the
    // message, charged with it.
    inline.inline_evidence = None;
    inline.current_input = &whole;
    let charged_whole = assembler().assemble(&inline).expect("plan");

    assert_eq!(text_of(&with_parts), text_of(&charged_whole));
    assert_eq!(
        with_parts.accounting.total_input,
        charged_whole.accounting.total_input
    );
    assert_eq!(with_parts.accounting.inline_evidence_kept, 5);
    assert!(with_parts.accounting.evicted.is_empty());
}

#[test]
fn the_turns_evidence_is_cut_before_recall_and_the_summary() {
    let recent = vec![message(1, SourceRole::User, "Question.")];
    let memory = snapshot(Vec::new(), Some("A short summary of earlier work."), 1);
    let render = passages(20);
    let mut base = request(Some(&memory), &recent, 4_096);
    base.current_input = BARE;
    base.recalled = vec![SelectedPassage {
        message_id: "m0".into(),
        sequence: 0,
        role: SourceRole::User,
        text: "recalled ".repeat(30),
    }];
    base.inline_evidence = Some(InlineEvidence {
        items: 20,
        render: &render,
    });

    let plan = assembler().assemble(&base).expect("plan");
    assert!(plan.accounting.fits());
    let kept = plan.accounting.inline_evidence_kept;
    assert!((1..20).contains(&kept), "kept {kept}");
    let rendered = text_of(&plan);
    assert!(rendered
        .iter()
        .any(|(_, content)| content.starts_with("[generated summary")));
    let (_, last) = last_message(&plan);
    assert!(last.contains("[conversation history retrieval context"));
    assert!(last.contains("[1] Document"));
}

#[test]
fn a_window_with_no_room_for_any_evidence_sends_the_bare_message() {
    // Leaves about 160 tokens after the policy, the tools and the message:
    // less than one passage.
    let policy = "rule ".repeat(960);
    let render = passages(4);
    let plan = assembler()
        .assemble(&turn(
            &policy,
            4_096,
            Some(InlineEvidence {
                items: 4,
                render: &render,
            }),
        ))
        .expect("the bare message fits");
    assert!(plan.accounting.fits(), "{:?}", plan.accounting);
    assert_eq!(plan.accounting.inline_evidence_kept, 0);
    let (_, last) = last_message(&plan);
    assert!(last.ends_with(&format!("[current message]\n{BARE}")));
    assert!(!last.contains("Document:"));
}

#[test]
fn the_policy_tools_and_bare_message_alone_still_refuse_a_turn_they_do_not_fit() {
    let policy = "policy ".repeat(20_000);
    let render = passages(4);
    let error = assembler()
        .assemble(&turn(
            &policy,
            8_192,
            Some(InlineEvidence {
                items: 4,
                render: &render,
            }),
        ))
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidInput(_)), "{error:?}");
    assert!(error.to_string().contains("current message"));
}
