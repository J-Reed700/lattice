//! The calls of a synthesis: what the model is shown and what the app makes
//! of its answers, with the sources and entries carried by the app.
use super::notes::{
    merge_notes, units_of, write_brief, write_notes, BriefInput, EntryPart, MapInput, MapNotes,
    MergeInput, MergeNotes, Note, Section, WriteBrief,
};
use super::week_scope_tests::source;
use super::*;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::shown::Told;
use lattice_core::application::services::model_contract::test_support::snapshot::assert_requests;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use serde_json::json;

fn prompt_of(sent: &Sent) -> String {
    sent.request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The handles of `prefix` the prompt shows in square brackets, in order.
fn handles(prompt: &str, prefix: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = prompt;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            break;
        };
        let token = &after[..close];
        let digits = token.strip_prefix(prefix).unwrap_or_default();
        if !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && !found.iter().any(|seen| seen == token)
        {
            found.push(token.to_string());
        }
        rest = &after[close + 1..];
    }
    found
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn entry(title: &str, transcript: &str) -> JournalSynthesisEntry {
    JournalSynthesisEntry {
        conversation_id: title.to_string(),
        title: title.to_string(),
        updated_at: "2026-09-05T10:00:00.000Z".to_string(),
        message_count: 2,
        transcript: transcript.to_string(),
        kind: SynthesisEntryKind::Conversation,
        source_id: title.to_string(),
        sources: Vec::new(),
        messages: Vec::new(),
    }
}

fn part(index: usize, title: &str, transcript: &str, sources: Vec<SourceDto>) -> EntryPart {
    EntryPart::of(
        index,
        &entry(title, transcript),
        1,
        1,
        transcript.to_string(),
        &sources,
    )
}

fn entries_answer(notes: &[(&str, &str, Vec<&str>)]) -> String {
    let entries: Vec<serde_json::Value> = notes
        .iter()
        .map(|(of, text, sources)| {
            json!({
                "of": of,
                "highlights": [{ "text": text, "sources": sources }],
                "practical_details": [],
                "decisions_constraints": [],
                "open_questions_risks": [],
                "evidence_notes": []
            })
        })
        .collect();
    json!({ "entries": entries }).to_string()
}

fn two_parts() -> Vec<EntryPart> {
    vec![
        part(
            0,
            "Harbour log",
            "ASSISTANT: high tide is at six [3]",
            vec![source("tide-table", "c1", "Tide table passage", 3)],
        ),
        part(1, "Kiln notes", "USER: the kiln holds heat", Vec::new()),
    ]
}

fn note(section: Section, entries: &[usize], text: &str, sources: &[u32]) -> Note {
    Note {
        section,
        entries: entries.to_vec(),
        text: text.to_string(),
        sources: sources.to_vec(),
    }
}

fn titles() -> Vec<String> {
    vec!["Harbour log".to_string(), "Kiln notes".to_string()]
}

fn chunks() -> Vec<Vec<Note>> {
    vec![
        vec![note(Section::Highlights, &[0], "High tide is at six", &[3])],
        vec![note(Section::Highlights, &[1], "The kiln holds heat", &[5])],
    ]
}

#[test]
fn the_map_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let title = "Harbour log";
    let transcript = "USER: when is high tide";
    for told in [
        title,
        transcript,
        "tide-table.pdf",
        "Tide table passage",
        "Spring tides",
    ] {
        marks.told(told.to_string());
    }
    marks.told("2026-09-05T10:00:00.000Z".to_string());
    let index = marks.internal(7_000_123usize);
    let id = marks.internal(7_000_456u32);
    let mut cited = source("tide-table", "c1", "Tide table passage", *id.value());
    cited.section = Some("Spring tides".to_string());
    let input = MapInput {
        chunk: Told(1),
        chunks: Told(1),
        parts: vec![EntryPart::of(
            *index.value(),
            &entry(title, transcript),
            1,
            1,
            transcript.to_string(),
            &[cited],
        )],
    };
    holds::<MapNotes>(&input, &marks);
}

#[test]
fn the_merge_and_brief_requests_show_what_the_model_is_told() {
    let mut marks = Marks::new();
    marks.told("Harbour log".to_string());
    marks.told("High tide is at six".to_string());
    let units = units_of(&chunks()[..1], &titles());
    let merge = MergeInput {
        group: Told(1),
        groups: Told(1),
        chunks: 1,
        notes: units.clone(),
    };
    holds::<MergeNotes>(&merge, &marks);
    let brief = BriefInput {
        chunks: 1,
        notes: units,
    };
    holds::<WriteBrief>(&brief, &marks);
}

#[tokio::test]
async fn an_entry_is_noted_with_its_own_title_and_the_numbers_of_the_sources_it_cites() {
    let model = ScriptedModel::new().on_request::<MapNotes>(|sent, _| {
        let prompt = prompt_of(sent);
        let entries = handles(&prompt, "e");
        let sources = handles(&prompt, "s");
        reply(entries_answer(&[
            (
                entries[0].as_str(),
                "High tide is at six",
                vec![sources[0].as_str(), "s9"],
            ),
            (entries[1].as_str(), "The kiln holds heat", vec![]),
        ]))
    });

    let notes = write_notes(&model, &two_parts(), 0, 1).await.unwrap();

    assert_eq!(
        notes,
        [
            note(Section::Highlights, &[0], "High tide is at six", &[3]),
            note(Section::Highlights, &[1], "The kiln holds heat", &[]),
        ]
    );
}

#[tokio::test]
async fn the_transcript_markers_are_the_labels_the_model_is_shown() {
    let model = ScriptedModel::new().on_request::<MapNotes>(|sent, _| {
        let prompt = prompt_of(sent);
        let entries = handles(&prompt, "e");
        reply(entries_answer(&[
            (entries[0].as_str(), "High tide is at six", vec![]),
            (entries[1].as_str(), "The kiln holds heat", vec![]),
        ]))
    });

    write_notes(&model, &two_parts(), 0, 1).await.unwrap();

    let prompt = prompt_of(&model.sent()[0]);
    assert!(
        prompt.contains("ASSISTANT: high tide is at six [s1]"),
        "{prompt}"
    );
    assert!(prompt.contains("[s1] tide-table.pdf"), "{prompt}");
    assert!(!prompt.contains("[3]"), "{prompt}");
    assert!(!prompt.contains("[Entry:"), "{prompt}");
}

#[tokio::test]
async fn an_entry_left_out_is_asked_for_alone_and_the_others_stand() {
    let model = ScriptedModel::new().on_request::<MapNotes>(|sent, nth| {
        let prompt = prompt_of(sent);
        let entries = handles(&prompt, "e");
        if nth == 0 {
            reply(entries_answer(&[(
                entries[0].as_str(),
                "High tide is at six",
                vec![],
            )]))
        } else {
            reply(entries_answer(&[(
                entries[0].as_str(),
                "The kiln holds heat",
                vec![],
            )]))
        }
    });

    let notes = write_notes(&model, &two_parts(), 0, 1).await.unwrap();

    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    let second = prompt_of(&sent[1]);
    assert!(second.contains("Kiln notes") && !second.contains("Harbour log"));
    assert_eq!(
        notes
            .iter()
            .map(|note| note.entries.clone())
            .collect::<Vec<_>>(),
        [vec![0], vec![1]]
    );
}

#[tokio::test]
async fn an_entry_that_stays_unanswered_fails_the_part() {
    let model = ScriptedModel::new()
        .on_request::<MapNotes>(|_, _| reply(json!({ "entries": [] }).to_string()));

    assert!(write_notes(&model, &two_parts(), 0, 1).await.is_err());
}

#[tokio::test]
async fn a_merged_bullet_has_the_entries_and_sources_of_the_bullets_it_merges() {
    let model = ScriptedModel::new().on_request::<MergeNotes>(|sent, _| {
        let notes = handles(&prompt_of(sent), "n");
        reply(
            json!({
                "highlights": [{ "text": "Both logs agree", "from": [notes[1], notes[0]] }],
                "practical_details": [],
                "decisions_constraints": [],
                "open_questions_risks": [],
                "evidence_notes": []
            })
            .to_string(),
        )
    });

    let merged = merge_notes(&model, &chunks(), &titles(), 1, 1)
        .await
        .unwrap();

    assert_eq!(
        merged,
        [note(
            Section::Highlights,
            &[1, 0],
            "Both logs agree",
            &[5, 3]
        )]
    );
    let prompt = prompt_of(&model.sent()[0]);
    assert!(
        prompt.contains("[Entry: Harbour log] High tide is at six"),
        "{prompt}"
    );
    assert!(
        !prompt.contains("[3]") && !prompt.contains("[5]"),
        "{prompt}"
    );
}

#[tokio::test]
async fn a_merged_bullet_that_names_no_bullet_is_corrected() {
    let model = ScriptedModel::new().on_request::<MergeNotes>(|sent, nth| {
        let notes = handles(&prompt_of(sent), "n");
        let from: Vec<&String> = if nth == 0 {
            Vec::new()
        } else {
            vec![&notes[0]]
        };
        reply(
            json!({
                "highlights": [{ "text": "Both logs agree", "from": from }],
                "practical_details": [],
                "decisions_constraints": [],
                "open_questions_risks": [],
                "evidence_notes": []
            })
            .to_string(),
        )
    });

    let merged = merge_notes(&model, &chunks(), &titles(), 1, 1)
        .await
        .unwrap();

    assert_eq!(model.sent().len(), 2);
    assert_eq!(
        merged,
        [note(Section::Highlights, &[0], "Both logs agree", &[3])]
    );
}

#[tokio::test]
async fn the_brief_cites_by_bullet_and_the_app_writes_the_source_numbers() {
    let model = ScriptedModel::new().on_request::<WriteBrief>(|sent, _| {
        let notes = handles(&prompt_of(sent), "n");
        reply(
            json!({
                "brief": format!(
                    "## Executive Summary\nHigh tide is at six [{}]. Unknown [n9]. Code `[{}]`.",
                    notes[0], notes[0]
                )
            })
            .to_string(),
        )
    });

    let brief = write_brief(&model, &chunks(), &titles()).await.unwrap();

    assert_eq!(
        brief,
        "## Executive Summary\nHigh tide is at six [3]. Unknown [n9]. Code `[n1]`."
    );
    let prompt = prompt_of(&model.sent()[0]);
    assert!(
        prompt.contains("- [n1] [Entry: Harbour log] High tide is at six"),
        "{prompt}"
    );
    assert!(prompt.contains("CHUNK SYNTHESIS 2"), "{prompt}");
}

#[tokio::test]
async fn a_section_without_bullets_says_none_identified() {
    let model = ScriptedModel::new()
        .on_request::<WriteBrief>(|_, _| reply(json!({ "brief": "A brief" }).to_string()));

    write_brief(&model, &chunks(), &titles()).await.unwrap();

    let prompt = prompt_of(&model.sent()[0]);
    assert!(
        prompt.contains("### Practical Details\n- None identified."),
        "{prompt}"
    );
}

#[tokio::test]
async fn prompt_snapshot_journal_map() {
    let model = ScriptedModel::new().on_request::<MapNotes>(|sent, _| {
        let prompt = prompt_of(sent);
        let entries = handles(&prompt, "e");
        reply(entries_answer(&[
            (entries[0].as_str(), "High tide is at six", vec!["s1"]),
            (entries[1].as_str(), "The kiln holds heat", vec![]),
        ]))
    });
    write_notes(&model, &two_parts(), 0, 1).await.unwrap();
    let requests: Vec<_> = model.sent().into_iter().map(|sent| sent.request).collect();
    assert_requests(env!("CARGO_MANIFEST_DIR"), "journal-map", &requests);
}

#[tokio::test]
async fn prompt_snapshot_journal_merge() {
    let model = ScriptedModel::new().on_request::<MergeNotes>(|sent, _| {
        let notes = handles(&prompt_of(sent), "n");
        reply(
            json!({
                "highlights": [{ "text": "Both logs agree", "from": [notes[0], notes[1]] }],
                "practical_details": [],
                "decisions_constraints": [],
                "open_questions_risks": [],
                "evidence_notes": []
            })
            .to_string(),
        )
    });
    merge_notes(&model, &chunks(), &titles(), 1, 1)
        .await
        .unwrap();
    let requests: Vec<_> = model.sent().into_iter().map(|sent| sent.request).collect();
    assert_requests(env!("CARGO_MANIFEST_DIR"), "journal-merge", &requests);
}

#[tokio::test]
async fn prompt_snapshot_journal_brief() {
    let model = ScriptedModel::new().on_request::<WriteBrief>(|sent, _| {
        let notes = handles(&prompt_of(sent), "n");
        reply(json!({ "brief": format!("High tide is at six [{}].", notes[0]) }).to_string())
    });
    write_brief(&model, &chunks(), &titles()).await.unwrap();
    let requests: Vec<_> = model.sent().into_iter().map(|sent| sent.request).collect();
    assert_requests(env!("CARGO_MANIFEST_DIR"), "journal-brief", &requests);
}
