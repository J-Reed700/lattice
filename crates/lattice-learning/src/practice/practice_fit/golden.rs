use super::*;
use crate::practice::coding::golden_support::{add_requests, assert_golden, Golden, Recorder};

fn outline() -> CourseOutline {
    CourseOutline {
        title: "Sample course".into(),
        modules: vec![
            OutlineModule {
                title: "First module".into(),
                lessons: vec![
                    OutlineLesson {
                        title: "Lesson one".into(),
                        objective: "Say what the first lesson teaches.".into(),
                    },
                    OutlineLesson {
                        title: "Lesson two".into(),
                        objective: "Say what the second lesson teaches.".into(),
                    },
                ],
            },
            OutlineModule {
                title: "Second module".into(),
                lessons: vec![OutlineLesson {
                    title: "Lesson three".into(),
                    objective: "Say what the third lesson teaches.".into(),
                }],
            },
        ],
    }
}

#[tokio::test]
async fn golden_fit() {
    let mut golden = Golden::default();
    golden.add("system", SYSTEM);
    golden.add("prompt", &prompt(&outline()));
    golden.add(
        "schema",
        &serde_json::to_string_pretty(&schema()).expect("a schema writes out"),
    );
    let model = Recorder::new([
        json!({"reason": "Sample reason.", "courseTeachesProgramCode": true}).to_string(),
    ]);
    let judgement = ask(&model, &outline()).await;
    assert!(judgement.is_ok(), "{:?}", judgement.err());
    add_requests(&mut golden, "the request", &model);
    assert_golden(
        "fit",
        include_str!("../coding/golden/fit.txt"),
        golden.text(),
    );
}
