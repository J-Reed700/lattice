use super::*;
use crate::practice::coding::golden_support::{add_requests, assert_golden, Golden, Recorder};
use crate::practice::coding::proof::fakes::{Behaviour, FakeRunner};
use crate::practice::kinds::program::toolchain::FakeToolchains;
use crate::practice::kinds::program::{kind, KIND};
use crate::practice::session::kind::Kinds;
use lattice_core::application::services::model_contract::shape::{Held, Shape};
use std::collections::BTreeSet;
use std::sync::Arc;

fn kinds() -> Kinds {
    Kinds::new([kind(Arc::new(FakeToolchains::new(FakeRunner::new(
        |_, _| Behaviour::Fails,
    ))))])
    .expect("one kind registers")
}

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
    let open = open_of(&kinds(), &BTreeSet::from([KIND]));
    let input = Input {
        outline: Told(outline()),
        kinds: open.clone(),
    };
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let says = PracticeFit::says(&input, &mut showing).expect("the request says");
    let schema = <<PracticeFit as Answered>::Answer as Shape>::schema(&showing, Held::Enums)
        .expect("a schema");
    let mut golden = Golden::default();
    golden.add("system", &says.instructions);
    golden.add("prompt", &says.task);
    golden.add(
        "schema",
        &serde_json::to_string_pretty(schema.as_json()).expect("a schema writes out"),
    );
    let model = Recorder::new([json!({"entries": [
        {"of": "k1", "reason": "Sample reason.", "courseAsksForIt": "yes"}
    ]})
    .to_string()]);
    let judgement = ask(&model, &open, &outline()).await;
    assert!(judgement.is_ok(), "{:?}", judgement.err());
    add_requests(&mut golden, "the request", &model);
    assert_golden(
        "fit",
        include_str!("../coding/golden/fit.txt"),
        golden.text(),
    );
}
