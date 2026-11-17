//! The reply a shipped-path row keeps: the chat's own pieces for a live turn,
//! and the same reading of a saved answer when a row is rescored. The module is
//! the example's, compiled here because CI runs no example's tests.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

#[allow(dead_code)]
#[path = "../examples/shipped_path_eval/reply.rs"]
mod reply;

use lattice::features::conversation::dto::MessageDto;
use lattice_core::application::contracts::replies::{ReplyPieceDto, ReplyReferenceDto};
use serde_json::{json, Value};

fn message(role: &str, reply: Option<Vec<ReplyPieceDto>>) -> MessageDto {
    MessageDto {
        id: format!("{role}-id"),
        conversation_id: "c".into(),
        role: role.into(),
        content: String::new(),
        tokens: 0,
        created_at: String::new(),
        metadata: None,
        status: "completed".into(),
        reply,
    }
}

fn text(text: &str) -> ReplyPieceDto {
    ReplyPieceDto::Text { text: text.into() }
}

fn source(number: u32) -> ReplyPieceDto {
    ReplyPieceDto::Reference {
        reference: ReplyReferenceDto {
            written: format!("[s{number}]"),
            kind: "source".into(),
            number,
            lines: None,
            name: None,
        },
    }
}

fn text_json(text: &str) -> Value {
    json!({"type": "text", "text": text})
}

fn source_json(number: u32) -> Value {
    json!({"type": "reference", "reference": {"written": format!("[s{number}]"), "kind": "source", "number": number}})
}

#[test]
fn a_row_keeps_the_pieces_the_chat_drew_its_last_answer_from() {
    let messages = vec![
        message("user", None),
        message("assistant", Some(vec![text("Earlier.")])),
        message("user", None),
        message("assistant", Some(vec![text("Renews on 2025-10-14 "), source(2), text(".")])),
    ];
    assert_eq!(
        reply::shown(&messages),
        json!([text_json("Renews on 2025-10-14 "), source_json(2), text_json(".")])
    );
}

#[test]
fn an_answer_that_came_without_pieces_is_kept_as_null() {
    let messages = vec![message("user", None), message("assistant", None)];
    assert_eq!(reply::shown(&messages), Value::Null);
}

#[tokio::test]
async fn a_saved_answer_resolves_only_the_handles_its_conversation_named() {
    let pieces = reply::read_saved("A [s2] B [2] C [s01] D [s4] E [s3].", 3)
        .await
        .unwrap();
    assert_eq!(
        pieces,
        json!([
            text_json("A "),
            source_json(2),
            text_json(" B [2] C [s01] D [s4] E "),
            source_json(3),
            text_json("."),
        ])
    );
}

#[tokio::test]
async fn a_rescored_turn_reads_against_every_source_named_so_far() {
    let mut row = json!({"turns": [
        {"answer": "X [s2].", "evidence": [{"citation_id": 1}, {"citation_id": 2}]},
        {"answer": "Y [s1] [s3] [s4].", "evidence": [{"citation_id": 3}]},
        {"answer": null, "evidence": []},
    ]});
    reply::rescore_row(&mut row).await.unwrap();
    assert_eq!(
        row["turns"][0]["reply"],
        json!([text_json("X "), source_json(2), text_json(".")])
    );
    assert_eq!(
        row["turns"][1]["reply"],
        json!([
            text_json("Y "),
            source_json(1),
            text_json(" "),
            source_json(3),
            text_json(" [s4]."),
        ])
    );
    assert_eq!(row["turns"][2]["reply"], json!([]));
}
