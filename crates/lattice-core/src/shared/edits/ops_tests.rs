use serde_json::{json, Value};

use super::fake::note_form;
use super::ops::apply_ops;
use super::rules::form_problems;
use super::types::{EditOp, FieldProblem, FieldSpec, TextFormat};
use crate::shared::error::AppError;

fn sample_doc() -> Value {
    json!({
        "title": "Quillet guide",
        "body": "Paragraph one.\n\nParagraph two.",
        "tags": [
            { "text": "alpha", "main": true },
            { "text": "beta", "main": false },
            { "text": "gamma", "main": false }
        ],
        "level": 0,
        "count": 2
    })
}

#[test]
fn applying_operations_then_their_inverse_restores_the_document() {
    let form = note_form();
    let cases: Vec<(Value, Vec<EditOp>, Value)> = vec![
        (
            sample_doc(),
            vec![
                EditOp::Replace {
                    path: "/title".into(),
                    value: json!("Updated Quillet"),
                },
                EditOp::Replace {
                    path: "/body".into(),
                    value: json!("Line A\r\nLine B"),
                },
                EditOp::Replace {
                    path: "/level".into(),
                    value: json!(2),
                },
                EditOp::Replace {
                    path: "/count".into(),
                    value: json!(5),
                },
                EditOp::Replace {
                    path: "/tags/1/main".into(),
                    value: json!(true),
                },
                EditOp::Replace {
                    path: "/tags/0/main".into(),
                    value: json!(false),
                },
            ],
            json!({
                "title": "Updated Quillet",
                "body": "Line A\nLine B",
                "tags": [
                    { "text": "alpha", "main": false },
                    { "text": "beta", "main": true },
                    { "text": "gamma", "main": false }
                ],
                "level": 2,
                "count": 5
            }),
        ),
        (
            sample_doc(),
            vec![
                EditOp::Remove {
                    path: "/tags/0".into(),
                },
                EditOp::Replace {
                    path: "/tags/0/text".into(),
                    value: json!("beta-updated"),
                },
                EditOp::Add {
                    path: "/tags/-".into(),
                    value: json!({ "text": "delta", "main": false }),
                },
            ],
            json!({
                "title": "Quillet guide",
                "body": "Paragraph one.\n\nParagraph two.",
                "tags": [
                    { "text": "beta-updated", "main": false },
                    { "text": "gamma", "main": false },
                    { "text": "delta", "main": false }
                ],
                "level": 0,
                "count": 2
            }),
        ),
        (
            sample_doc(),
            vec![
                EditOp::Add {
                    path: "/tags/1".into(),
                    value: json!({ "text": "inserted", "main": false }),
                },
                EditOp::Remove {
                    path: "/tags/3".into(),
                },
                EditOp::Remove {
                    path: "/tags/2".into(),
                },
            ],
            json!({
                "title": "Quillet guide",
                "body": "Paragraph one.\n\nParagraph two.",
                "tags": [
                    { "text": "alpha", "main": true },
                    { "text": "inserted", "main": false }
                ],
                "level": 0,
                "count": 2
            }),
        ),
    ];

    for (doc, ops, expected) in cases {
        let applied = apply_ops(&doc, &form, &ops);
        assert!(applied.is_ok(), "{applied:?}");
        let (next, inverse) = applied.unwrap();
        assert_eq!(next, expected);

        let restored = apply_ops(&next, &form, &inverse);
        assert!(restored.is_ok(), "{restored:?}");
        assert_eq!(restored.unwrap().0, doc);
    }
}

#[test]
fn an_operation_the_form_does_not_describe_is_refused() {
    let form = note_form();
    let doc = sample_doc();
    let bad_ops = vec![
        (
            "/unknown",
            EditOp::Replace {
                path: "/unknown".into(),
                value: json!("x"),
            },
        ),
        (
            "title",
            EditOp::Replace {
                path: "title".into(),
                value: json!("x"),
            },
        ),
        (
            "/title",
            EditOp::Replace {
                path: "/title".into(),
                value: json!(123),
            },
        ),
        (
            "/level",
            EditOp::Replace {
                path: "/level".into(),
                value: json!(3),
            },
        ),
        (
            "/count",
            EditOp::Replace {
                path: "/count".into(),
                value: json!(-1),
            },
        ),
        (
            "/tags/1/main",
            EditOp::Replace {
                path: "/tags/1/main".into(),
                value: json!("yes"),
            },
        ),
        (
            "/tags/9/text",
            EditOp::Replace {
                path: "/tags/9/text".into(),
                value: json!("x"),
            },
        ),
        (
            "/title/-",
            EditOp::Add {
                path: "/title/-".into(),
                value: json!({ "text": "x", "main": false }),
            },
        ),
        (
            "/tags/5",
            EditOp::Add {
                path: "/tags/5".into(),
                value: json!({ "text": "x", "main": false }),
            },
        ),
        (
            "/tags/-",
            EditOp::Add {
                path: "/tags/-".into(),
                value: json!({ "text": "x" }),
            },
        ),
        (
            "/tags/-",
            EditOp::Add {
                path: "/tags/-".into(),
                value: json!({ "text": "x", "main": false, "extra": true }),
            },
        ),
        (
            "/tags/9",
            EditOp::Remove {
                path: "/tags/9".into(),
            },
        ),
        (
            "/tags/-",
            EditOp::Remove {
                path: "/tags/-".into(),
            },
        ),
    ];

    for (expected_path, op) in bad_ops {
        let res = apply_ops(&doc, &form, &[op]);
        assert!(
            matches!(res, Err(AppError::InvalidInput(ref msg)) if msg.contains(expected_path)),
            "expected InvalidInput naming {expected_path}, got {res:?}"
        );
    }
}

#[test]
fn a_list_is_held_to_its_bounds() {
    let form = note_form();
    let doc = sample_doc();

    let too_few_ops = vec![
        EditOp::Remove {
            path: "/tags/2".into(),
        },
        EditOp::Remove {
            path: "/tags/1".into(),
        },
        EditOp::Remove {
            path: "/tags/0".into(),
        },
    ];
    let res = apply_ops(&doc, &form, &too_few_ops);
    assert!(
        matches!(res, Err(AppError::InvalidInput(ref msg)) if msg.contains("/tags")),
        "{res:?}"
    );

    let too_many_ops = vec![
        EditOp::Add {
            path: "/tags/-".into(),
            value: json!({ "text": "delta", "main": false }),
        },
        EditOp::Add {
            path: "/tags/-".into(),
            value: json!({ "text": "epsilon", "main": false }),
        },
    ];
    let res = apply_ops(&doc, &form, &too_many_ops);
    assert!(
        matches!(res, Err(AppError::InvalidInput(ref msg)) if msg.contains("/tags")),
        "{res:?}"
    );

    let too_few_doc = json!({ "title": "T", "body": "B", "tags": [], "level": 0, "count": 0 });
    assert_eq!(
        form_problems(&form, &too_few_doc),
        vec![FieldProblem {
            path: Some("/tags".into()),
            message: "Tags needs at least 1.".into(),
        }]
    );

    let too_many_doc = json!({
        "title": "T",
        "body": "B",
        "tags": [
            { "text": "a", "main": true },
            { "text": "b", "main": false },
            { "text": "c", "main": false },
            { "text": "d", "main": false },
            { "text": "e", "main": false }
        ],
        "level": 0,
        "count": 0
    });
    assert_eq!(
        form_problems(&form, &too_many_doc),
        vec![FieldProblem {
            path: Some("/tags".into()),
            message: "Tags can have at most 4.".into(),
        }]
    );
}

#[test]
fn one_member_holds_an_exclusive_flag() {
    let form = note_form();
    let doc = json!({
        "title": "   ",
        "body": "ok",
        "tags": [
            { "text": "a", "main": true },
            { "text": "b", "main": true }
        ],
        "level": 0,
        "count": 0
    });
    assert_eq!(
        form_problems(&form, &doc),
        vec![
            FieldProblem {
                path: Some("/title".into()),
                message: "Title can't be empty.".into(),
            },
            FieldProblem {
                path: Some("/tags".into()),
                message: "Mark only one Tag as \"Main\".".into(),
            },
        ]
    );

    let line_form = vec![FieldSpec::Text {
        path: "/title".into(),
        label: "Title".into(),
        format: TextFormat::Line,
        required: true,
    }];
    let multiline = json!({ "title": "First\nSecond" });
    assert_eq!(
        form_problems(&line_form, &multiline),
        vec![FieldProblem {
            path: Some("/title".into()),
            message: "Title must be on one line.".into(),
        }]
    );
}
