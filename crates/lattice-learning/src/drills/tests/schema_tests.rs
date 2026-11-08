//! Nothing a model writes in a drill build is capped: no schema bounds the length of a text, and no answer is cut by
//! the code. A count is only what the build needs: one mutation for each target.

use super::support::{
    blind_answer, blind_assessment, clean_code_answer, entry_answer, judge_answer, plant_answer,
    review_answer, FakeGrader, FakeRunner, Harness, ScriptedModel,
};
use crate::practice::practice_tests::text_length_rules;
use lattice_core::shared::error::Result;

#[tokio::test]
async fn the_schemas_of_a_build_hold_no_text_length_cap() -> Result<()> {
    let (harness, spec) = Harness::lesson().await?;
    let model = ScriptedModel::new([
        clean_code_answer(),
        review_answer(vec![]),
        plant_answer(),
        judge_answer(true),
        blind_answer(),
        entry_answer(),
    ]);
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec)
        .await?;
    let requests = model.requests();
    assert_eq!(requests.len(), 6);
    for request in &requests {
        let schema = request
            .json_schema
            .as_ref()
            .expect("every call is structured");
        assert!(text_length_rules(schema).is_empty(), "{schema}");
        assert_eq!(schema["additionalProperties"], false);
    }
    Ok(())
}

#[tokio::test]
async fn an_answer_is_never_cut_down_by_the_code() -> Result<()> {
    // A long file, a long suite and a long comment are all kept whole.
    let (harness, spec) = Harness::lesson().await?;
    let long = "x = 1\n".repeat(20_000);
    let answer = serde_json::json!({
        "title": "A long file",
        "files": [{"path": "src/long.py", "kind": "source", "text": long}],
        "suite": long
    })
    .to_string();
    let model = ScriptedModel::new([answer]);
    let runner = FakeRunner::passing();
    let grader = FakeGrader::failing("unused");
    let code = harness
        .build(&model, Some(&runner), &grader)
        .clean_code(&spec)
        .await?;
    assert_eq!(code.files[0].text, long);
    assert_eq!(code.suite.as_deref(), Some(long.as_str()));
    Ok(())
}
