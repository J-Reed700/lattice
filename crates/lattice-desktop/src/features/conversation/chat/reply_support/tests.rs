use super::*;
use crate::features::conversation::chat::verification::test_support::reply;
use lattice_core::application::contracts::replies::ReplyReferenceDto;
use lattice_core::application::contracts::sources::WebSnapshotDto;
use serde_json::json;

const RUNBOOK: &str = "Hold the mitigation for 20 seconds. Once the hold expires with the band \
    still broken, page Marisol Quenby and quote TCK-1836. Never raise a second ticket for the \
    same incident while the first one is open.";

fn chunk_source(content: &str, citation_id: u32) -> SourceDto {
    SourceDto {
        page_number: None,
        from_ocr: false,
        document_id: format!("doc-{citation_id}"),
        chunk_id: format!("chunk-{citation_id}"),
        content: content.to_string(),
        score: 0.9,
        path: None,
        position: None,
        file_name: format!("doc-{citation_id}.md"),
        file_path: format!("/vault/doc-{citation_id}.md"),
        mime_type: "text/markdown".to_string(),
        category: "Markdown".to_string(),
        file_size_bytes: 120,
        modified_at: "2026-01-01T00:00:00Z".to_string(),
        excerpt: Some(content.to_string()),
        highlights: None,
        section: None,
        chunk_index: Some(0),
        chunk_excerpts: None,
        citation_id: Some(citation_id),
        web_snapshot: None,
    }
}

fn web_source(url: &str, snippet: &str, full_text: &str, citation_id: u32) -> SourceDto {
    SourceDto {
        document_id: format!("web:{url}"),
        chunk_id: format!("web:{url}#0"),
        content: snippet.to_string(),
        excerpt: Some(snippet.to_string()),
        file_name: "Article".to_string(),
        file_path: url.to_string(),
        mime_type: "text/html".to_string(),
        category: "Web article".to_string(),
        web_snapshot: Some(WebSnapshotDto {
            url: url.to_string(),
            title: Some("Article".to_string()),
            text: full_text.to_string(),
            fetched_at: Some("2026-01-01T00:00:00Z".to_string()),
            truncated: false,
        }),
        ..chunk_source(snippet, citation_id)
    }
}

#[test]
fn verified_explicit_quote_produces_quotes_relation_without_double_wrapping() {
    let sources = [chunk_source(RUNBOOK, 1)];
    let reply = reply(
        "The runbook directs: \"page Marisol Quenby and quote TCK-1836\" [s1].",
        &sources,
    );
    let verification = json!({
        "claimVerdicts": [{
            "sentence": "The runbook directs: \"page Marisol Quenby and quote TCK-1836\".",
            "citationIds": [1],
            "verdict": "supported",
            "method": "judge",
            "evidenceQuote": "page Marisol Quenby and quote TCK-1836",
        }]
    });

    let out = pieces(&reply, &sources, Some(&verification));
    assert!(!out
        .iter()
        .any(|p| matches!(p, ReplyPieceDto::Quoted { .. })));

    let refs: Vec<_> = out
        .iter()
        .filter_map(|p| match p {
            ReplyPieceDto::Reference { reference } => Some(reference),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 1);
    let support = refs[0].support.as_ref().unwrap();
    assert_eq!(support.relation, RelationDto::Quotes);
    assert_eq!(
        support.anchors,
        vec![AnchorDto {
            exact: "page Marisol Quenby and quote TCK-1836".to_string(),
            span: None,
        }]
    );
}

#[test]
fn unquoted_exact_source_words_wrap_as_quoted_and_mark_quotes_with_snapshot_span() {
    let full = format!("# Guide\n\n{RUNBOOK}");
    let sources = [web_source("https://example.org/runbook", RUNBOOK, &full, 1)];
    let reply = reply(
        "The guide says page Marisol Quenby and quote TCK-1836 [s1].",
        &sources,
    );
    let verification = json!({
        "claimVerdicts": [{
            "sentence": "The guide says page Marisol Quenby and quote TCK-1836.",
            "citationIds": [1],
            "verdict": "supported",
            "method": "judge",
            "evidenceQuote": "Once the hold expires with the band still broken, page Marisol Quenby and quote TCK-1836.",
        }]
    });

    let out = pieces(&reply, &sources, Some(&verification));
    let expected_start = full.find("page Marisol Quenby and quote TCK-1836").unwrap();
    let expected_end = expected_start + "page Marisol Quenby and quote TCK-1836".len();
    assert_eq!(
        out,
        vec![
            ReplyPieceDto::Text {
                text: "The guide says ".to_string(),
            },
            ReplyPieceDto::Quoted {
                text: "page Marisol Quenby and quote TCK-1836".to_string(),
            },
            ReplyPieceDto::Text {
                text: " ".to_string(),
            },
            ReplyPieceDto::Reference {
                reference: ReplyReferenceDto {
                    written: "[s1]".to_string(),
                    kind: "source".to_string(),
                    number: 1,
                    lines: None,
                    name: None,
                    support: Some(SupportDto {
                        relation: RelationDto::Quotes,
                        anchors: vec![AnchorDto {
                            exact: "page Marisol Quenby and quote TCK-1836".to_string(),
                            span: Some(ByteSpanDto {
                                start: expected_start,
                                end: expected_end,
                            }),
                        }],
                    }),
                },
            },
            ReplyPieceDto::Text {
                text: ".".to_string(),
            },
        ]
    );
}

#[test]
fn supported_paraphrase_becomes_backs_with_optional_chunk_span() {
    let chunk_only = [chunk_source(RUNBOOK, 1)];
    let r1 = reply(
        "Escalate to Marisol when the mitigation hold ends and the issue persists [s1].",
        &chunk_only,
    );
    let verification = json!({
        "claimVerdicts": [{
            "sentence": "Escalate to Marisol when the mitigation hold ends and the issue persists.",
            "citationIds": [1],
            "verdict": "supported",
            "method": "judge",
            "evidenceQuote": "Once the hold expires with the band still broken, page Marisol Quenby and quote TCK-1836.",
        }]
    });

    let out_chunk = pieces(&r1, &chunk_only, Some(&verification));
    let ref_chunk = match &out_chunk[1] {
        ReplyPieceDto::Reference { reference } => reference,
        other => panic!("expected reference, got {other:?}"),
    };
    let support_chunk = ref_chunk.support.as_ref().unwrap();
    assert_eq!(support_chunk.relation, RelationDto::Backs);
    assert_eq!(support_chunk.anchors.len(), 1);
    assert_eq!(
        support_chunk.anchors[0].exact,
        "Once the hold expires with the band still broken, page Marisol Quenby and quote TCK-1836."
    );
    assert_eq!(support_chunk.anchors[0].span, None);

    let full = format!("# Intro\n\n{RUNBOOK}");
    let web = [web_source("https://example.org/runbook", RUNBOOK, &full, 1)];
    let r2 = reply(
        "Escalate to Marisol when the mitigation hold ends and the issue persists [s1].",
        &web,
    );
    let out_web = pieces(&r2, &web, Some(&verification));
    let ref_web = match &out_web[1] {
        ReplyPieceDto::Reference { reference } => reference,
        other => panic!("expected reference, got {other:?}"),
    };
    let support_web = ref_web.support.as_ref().unwrap();
    assert_eq!(support_web.relation, RelationDto::Backs);
    assert!(support_web.anchors[0].span.is_some());
}

#[test]
fn verbatim_method_promotes_supported_claim_to_quotes() {
    let sources = [chunk_source(RUNBOOK, 1)];
    let r = reply(
        "The rule states never raise a second ticket while the first is open [s1].",
        &sources,
    );
    let verification = json!({
        "claimVerdicts": [{
            "sentence": "The rule states never raise a second ticket while the first is open.",
            "citationIds": [1],
            "verdict": "supported",
            "method": "verbatim",
            "evidenceQuote": "Never raise a second ticket for the same incident while the first one is open.",
        }]
    });

    let out = pieces(&r, &sources, Some(&verification));
    let reference = out
        .iter()
        .find_map(|p| match p {
            ReplyPieceDto::Reference { reference } => Some(reference),
            _ => None,
        })
        .unwrap();
    let support = reference.support.as_ref().unwrap();
    assert_eq!(support.relation, RelationDto::Quotes);
    assert_eq!(support.anchors.len(), 1);
}

#[test]
fn unverified_unsupported_contradicted_and_multi_occurrence_claims_are_classified_per_chip() {
    let sources = [chunk_source(RUNBOOK, 1)];
    let r = reply(
        "Hold the mitigation for 20 seconds [s1]. Open a second ticket immediately [s1]. Why wait [s1]?",
        &sources,
    );
    let verification = json!({
        "claimVerdicts": [
            {
                "sentence": "Hold the mitigation for 20 seconds.",
                "citationIds": [1],
                "verdict": "supported",
                "method": "lexical",
                "evidenceQuote": null
            },
            {
                "sentence": "Open a second ticket immediately.",
                "citationIds": [1],
                "verdict": "contradicted",
                "method": "judge",
                "evidenceQuote": "Never raise a second ticket for the same incident while the first one is open."
            }
        ]
    });

    let out = pieces(&r, &sources, Some(&verification));
    let refs: Vec<_> = out
        .iter()
        .filter_map(|p| match p {
            ReplyPieceDto::Reference { reference } => Some(reference),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 3);
    assert_eq!(
        refs[0].support.as_ref().unwrap().relation,
        RelationDto::Quotes
    );
    assert_eq!(
        refs[1].support.as_ref().unwrap().relation,
        RelationDto::Cites
    );
    assert!(refs[1].support.as_ref().unwrap().anchors.is_empty());
    assert_eq!(
        refs[2].support.as_ref().unwrap().relation,
        RelationDto::Cites
    );
    assert!(refs[2].support.as_ref().unwrap().anchors.is_empty());

    // Pre-verification reply without verification metadata defaults all source chips to Cites.
    let pending_out = pieces(&r, &sources, None);
    let pending_relations: Vec<_> = pending_out
        .iter()
        .filter_map(|p| match p {
            ReplyPieceDto::Reference { reference } => {
                Some(reference.support.as_ref().unwrap().relation)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        pending_relations,
        vec![RelationDto::Cites, RelationDto::Cites, RelationDto::Cites]
    );
}

#[test]
fn explorer_file_references_keep_empty_support_while_source_chips_carry_support() {
    use crate::features::conversation::chat::reply_text::markers;
    use crate::features::conversation::chat::retrieval::source_names::name_sources;
    use crate::features::explorer::file_refs::FileNames;
    use lattice_core::application::services::model_contract::kept::TurnNames;
    use lattice_core::application::services::model_contract::reply::Place;
    use lattice_core::application::services::tool_handles::FolderFile;
    use std::sync::Arc;

    let sources = [chunk_source(RUNBOOK, 1)];
    let names = Arc::new(TurnNames::unstored());
    let mut named = sources.clone();
    name_sources(&names, &mut named);
    let file_names = FileNames::new(Arc::clone(&names));
    file_names.name("src/main.rs").unwrap();
    let reply_markers = markers(None).of_lines::<FolderFile>(Place::Inline);
    let built = ReplyText::read(
        &names,
        reply_markers,
        "Inspect [f1:12-20] first. Hold the mitigation for 20 seconds [s1].",
    );
    let verification = json!({
        "claimVerdicts": [{
            "sentence": "Hold the mitigation for 20 seconds.",
            "citationIds": [1],
            "verdict": "supported",
            "method": "judge",
            "evidenceQuote": "Hold the mitigation for 20 seconds.",
        }]
    });
    let out = pieces(&built, &named, Some(&verification));
    let file_refs: Vec<_> = out
        .iter()
        .filter_map(|p| match p {
            ReplyPieceDto::Reference { reference } if reference.kind == "file" => Some(reference),
            _ => None,
        })
        .collect();
    assert_eq!(file_refs.len(), 1);
    assert_eq!(file_refs[0].support, None);

    let source_refs: Vec<_> = out
        .iter()
        .filter_map(|p| match p {
            ReplyPieceDto::Reference { reference } if reference.kind == "source" => Some(reference),
            _ => None,
        })
        .collect();
    assert_eq!(source_refs.len(), 1);
    assert_eq!(
        source_refs[0].support.as_ref().unwrap().relation,
        RelationDto::Quotes
    );
}

#[test]
fn consecutive_chips_with_judge_verdict_both_quote_and_deduplicate_quoted_piece() {
    let other_runbook =
        "Recovery guide:\nHold the mitigation for 20 seconds before notifying on-call.\n";
    let sources = [chunk_source(RUNBOOK, 1), chunk_source(other_runbook, 2)];
    let r = reply("Hold the mitigation for 20 seconds [s1][s2].", &sources);
    let verification = json!({
        "claimVerdicts": [{
            "sentence": "Hold the mitigation for 20 seconds.",
            "citationIds": [1, 2],
            "verdict": "supported",
            "method": "judge",
            "evidenceQuote": "Hold the mitigation for 20 seconds.",
        }]
    });

    let out = pieces(&r, &sources, Some(&verification));
    let quoted: Vec<_> = out
        .iter()
        .filter_map(|p| match p {
            ReplyPieceDto::Quoted { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(quoted, vec!["Hold the mitigation for 20 seconds"]);

    let refs: Vec<_> = out
        .iter()
        .filter_map(|p| match p {
            ReplyPieceDto::Reference { reference } => Some(reference),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 2);
    for reference in refs {
        let support = reference.support.as_ref().unwrap();
        assert_eq!(support.relation, RelationDto::Quotes);
        assert_eq!(support.anchors[0].exact, "Hold the mitigation for 20 seconds");
    }
}
