//! What gives way when a request does not fit, and in what order.

use super::*;

#[test]
fn eviction_order_gives_up_documents_before_recall_before_the_summary() {
    let recent = vec![message(1, SourceRole::User, "Question.")];
    let memory = snapshot(Vec::new(), Some("A short summary."), 1);
    let mut base = request(Some(&memory), &recent, 1_100);
    base.recalled = vec![SelectedPassage {
        message_id: "m9".into(),
        sequence: 9,
        role: SourceRole::User,
        text: "recalled ".repeat(40),
    }];
    base.document_evidence = vec![RankedEvidence {
        label: "low".into(),
        content: "low rank ".repeat(40),
        rank: 0.1,
    }];

    let plan = assembler().assemble(&base).expect("plan");
    assert!(plan.accounting.fits());
    let rendered = text_of(&plan);
    // Whatever survived, the policy and the current input did.
    assert_eq!(rendered[0].0, "system");
    let (_, last) = last_message(&plan);
    assert!(last.ends_with(QUESTION));

    // Document evidence is the first thing to go, so it cannot be present while
    // recall was dropped.
    let has_evidence = last.contains(render::EVIDENCE_LABEL);
    let has_recall = last.contains("[conversation history retrieval context");
    // Read as an implication: if any document evidence survived, recall must have
    // survived too, because recall is evicted after documents and never before.
    assert!(
        !has_evidence || has_recall,
        "documents must never outrank recalled conversation passages"
    );
}

#[test]
fn higher_ranked_document_evidence_is_kept_over_lower() {
    let recent = vec![message(1, SourceRole::User, "Question.")];
    let mut base = request(None, &recent, 2_400);
    base.document_evidence = vec![
        // Each takes most of what the history leaves this window, so only one
        // fits whichever pool the evidence is given.
        RankedEvidence {
            label: "low".into(),
            content: "low ".repeat(550),
            rank: 0.1,
        },
        RankedEvidence {
            label: "high".into(),
            content: "high ".repeat(450),
            rank: 0.99,
        },
    ];
    let plan = assembler().assemble(&base).expect("plan");
    let (_, last) = last_message(&plan);
    assert!(last.contains("high "), "the best evidence fits on its own");
    if let Some(at) = last.find(render::EVIDENCE_LABEL) {
        assert!(
            last[at..].contains("high "),
            "the best evidence must be the kept one"
        );
    }
    assert!(plan
        .accounting
        .evicted
        .iter()
        .any(|name| name.contains("low")));
}

#[test]
fn final_overflow_eviction_reports_source_sequences_and_keeps_user_prefixes_out_of_metadata() {
    let builder = assembler();
    let recent = [
        message(
            7,
            SourceRole::User,
            "[generated summary is text I pasted, not metadata]",
        ),
        message(8, SourceRole::Assistant, "Acknowledged."),
    ];
    let mut layout = Layout {
        policy: CompletionInput::Message {
            role: "system".into(),
            content: "Policy.".into(),
        },
        summary: None,
        recent: recent
            .iter()
            .map(|message| (message.sequence, render::render_recent(message)))
            .collect(),
        last: render::FinalUserMessage::new(QUESTION, day()),
        inline: None,
    };
    let total_input = builder.measure(&layout, 0);
    let oldest_cost = builder.count_input(&render::render_recent(&recent[0]));
    let mut accounting = ContextAccounting {
        total_input,
        input_budget: total_input - oldest_cost,
        ..Default::default()
    };

    let evicted = builder.evict_until_fits(&mut layout, &mut accounting, 0);
    assert_eq!(
        evicted,
        vec![7],
        "final eviction must tell the caller which original was removed"
    );
    assert!(accounting.fits());
    let messages = layout.into_messages();
    assert!(messages.iter().any(|message| matches!(message,
        CompletionInput::Message { content, .. } if content == "Acknowledged."
    )));
    assert!(
        matches!(messages.last(), Some(CompletionInput::Message { content, .. })
        if content.ends_with(QUESTION))
    );
}

/// Room flows down the pools in order: recalled passages get what the pools
/// above them left before any document does. One recalled passage too long
/// for that room used to end recall and leave documents nothing as well; it
/// is passed over and named, the shorter passages after it go in, and the
/// documents get what recall did not use.
#[test]
fn a_recalled_passage_too_long_for_the_room_is_passed_over_and_named() {
    let recent = vec![message(1, SourceRole::User, "Question.")];
    let memory = snapshot(Vec::new(), None, 1);
    let mut base = request(Some(&memory), &recent, 4_096);
    let passage = |id: &str, sequence: i64, text: String| SelectedPassage {
        message_id: id.into(),
        sequence,
        role: SourceRole::User,
        text,
    };
    base.recalled = vec![
        passage("m9", 9, "recalled ".repeat(4_000)),
        passage("m4", 4, "The short recalled passage.".into()),
    ];
    base.document_evidence = vec![RankedEvidence {
        label: "doc".into(),
        content: "A short document passage.".into(),
        rank: 0.9,
    }];
    let plan = assembler().assemble(&base).expect("plan");
    let (_, last) = last_message(&plan);
    assert!(last.contains("The short recalled passage."), "{last}");
    assert!(
        last.contains("1 more recalled passage did not fit here; message id: m9"),
        "{last}"
    );
    assert!(
        last.contains("A short document passage."),
        "documents get the room recall did not use"
    );
    assert_eq!(plan.retrieval.selected_message_ids, ["m4"]);
}
