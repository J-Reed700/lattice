//! Stage 3: one mutation for each target pattern, as an exact edit the code applies.

use super::support::{
    baseline, build, idle_grader, last, literals_mutation, messages, operator_mutation,
    plant_answer, spec, system, FakeRunner, ScriptedModel, LITERALS, OPERATOR, SUITE,
};
use crate::features::learning::drills::build::Verification;
use crate::features::learning::tests::pool;
use lattice_core::domain::learning::drills::DrillFile;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

/// [`plant_answer`] with `change` made to its list of mutations.
fn answer_with(change: impl FnOnce(&mut Vec<Value>)) -> String {
    let mut answer: Value = serde_json::from_str(&plant_answer()).unwrap();
    change(answer["mutations"].as_array_mut().unwrap());
    answer.to_string()
}

#[tokio::test]
async fn one_mutation_for_each_target_comes_back_in_the_order_of_the_targets() -> Result<()> {
    let pool = pool().await?;
    let reversed = answer_with(|mutations| mutations.reverse());
    let model = ScriptedModel::new([reversed]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let planted = build(&pool, &model, Some(&runner), &grader)
        .plant(&spec(), &baseline())
        .await?;
    assert_eq!(
        planted.mutations,
        vec![operator_mutation(), literals_mutation()]
    );
    assert_eq!(planted.trail.calls.len(), 1);
    assert_eq!(planted.trail.calls[0].purpose, "plant");
    Ok(())
}

#[tokio::test]
async fn the_request_names_the_targets_the_runtime_and_the_suite() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([plant_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .plant(&spec(), &baseline())
        .await?;
    let request = &model.requests()[0];
    let schema = request.json_schema.clone().unwrap();
    let mutations = &schema["properties"]["mutations"];
    assert_eq!(mutations["minItems"], 2);
    assert_eq!(mutations["maxItems"], 2);
    let item = &mutations["items"];
    assert_eq!(
        item["properties"]["pattern"]["enum"],
        json!([OPERATOR, LITERALS])
    );
    assert_eq!(
        item["properties"]["path"]["enum"],
        json!(["src/limiter.py", "src/config.py"])
    );
    assert!(item["required"]
        .as_array()
        .unwrap()
        .contains(&json!("killingTest")));
    let system = system(request);
    assert!(system.contains("Fake contract"));
    assert!(system.contains("run as checks.py"));
    let sent = messages(request);
    let user = &sent[1].1;
    assert!(user.contains(OPERATOR));
    assert!(user.contains(LITERALS));
    assert!(user.contains("<suite>"));
    assert!(user.contains(SUITE));
    assert!(user.contains("<file path=\"src/config.py\">"));
    Ok(())
}

#[tokio::test]
async fn the_prompt_allows_the_name_the_comment_or_the_string_a_pattern_needs() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([plant_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .plant(&spec(), &baseline())
        .await?;
    let system = system(&model.requests()[0]);
    assert!(
        system.contains("Add no comment, name or string that points at the defect or admits it."),
        "{system}"
    );
    for forbidden in ["rename nothing", "write no string"] {
        assert!(
            !system.contains(forbidden),
            "a misleading name, comment or hard-coded string must be possible: {forbidden}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn without_a_runner_no_test_is_asked_for() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([plant_answer()]);
    let grader = idle_grader();
    let mut baseline = baseline();
    baseline.suite = None;
    let planted = build(&pool, &model, None, &grader)
        .plant(&spec(), &baseline)
        .await?;
    assert!(planted
        .mutations
        .iter()
        .all(|mutation| mutation.verification == Verification::NotAsked));
    let schema = model.requests()[0].json_schema.clone().unwrap();
    let item = &schema["properties"]["mutations"]["items"];
    assert!(item["properties"].get("killingTest").is_none());
    assert!(item["properties"].get("noTestBecause").is_none());
    assert!(!messages(&model.requests()[0])[1].1.contains("<suite>"));
    Ok(())
}

/// The first answer is `bad`, which has to be refused with a message holding `fragment`; the second is good.
async fn refused_then_accepted(bad: String, fragment: &str) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([bad, plant_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let planted = build(&pool, &model, Some(&runner), &grader)
        .plant(&spec(), &baseline())
        .await?;
    assert_eq!(planted.mutations.len(), 2, "{fragment}");
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "{fragment}");
    let reask = last(&requests[1]);
    assert!(
        reask.starts_with("That answer could not be used:"),
        "{reask}"
    );
    assert!(reask.contains(fragment), "{fragment} in {reask}");
    Ok(())
}

#[tokio::test]
async fn a_pattern_given_wrongly_is_refused_by_name() -> Result<()> {
    let cases = [
        (
            answer_with(|mutations| mutations.truncate(1)),
            "There is no change for readability.magic_number",
        ),
        (
            answer_with(|mutations| {
                let again = mutations[0].clone();
                mutations.push(again);
            }),
            "There are 2 changes for correctness.wrong_operator",
        ),
        (
            answer_with(|mutations| {
                let mut stray = mutations[0].clone();
                stray["pattern"] = json!("security.sql_injection");
                mutations.push(stray);
            }),
            "security.sql_injection is not a pattern asked for",
        ),
    ];
    for (bad, fragment) in cases {
        refused_then_accepted(bad, fragment).await?;
    }
    Ok(())
}

#[tokio::test]
async fn an_edit_that_does_not_apply_is_refused_naming_it_and_what_to_change() -> Result<()> {
    let first = |change: &dyn Fn(&mut Value)| answer_with(|mutations| change(&mut mutations[0]));
    let cases = [
        (
            first(&|mutation| mutation["before"] = json!("        if len(recent) >= other:\n")),
            "Edit for correctness.wrong_operator: `before` is not in src/limiter.py",
        ),
        (
            first(&|mutation| {
                mutation["before"] = json!("        self.hits[key] = recent\n");
                mutation["after"] = json!("        self.hits[key] = list(recent)\n");
            }),
            "`before` is in src/limiter.py 2 times",
        ),
        (
            first(&|mutation| mutation["before"] = json!("")),
            "Edit for correctness.wrong_operator: `before` is empty",
        ),
        (
            first(&|mutation| mutation["path"] = json!("src/other.py")),
            "src/other.py is not one of the files",
        ),
        (
            answer_with(|mutations| {
                mutations[1]["path"] = json!("src/limiter.py");
                mutations[1]["before"] = mutations[0]["before"].clone();
                mutations[1]["after"] = json!("        if len(recent) == self.limit:\n");
            }),
            "Edit for correctness.wrong_operator and Edit for readability.magic_number overlap",
        ),
        (
            first(&|mutation| mutation["after"] = mutation["before"].clone()),
            "`before` and `after` are the same",
        ),
        (
            first(&|mutation| mutation["after"] = json!("")),
            "`after` is empty. A deletion keeps a neighbouring line",
        ),
        (
            first(&|mutation| {
                mutation["before"] = json!("            return False\n");
                mutation["after"] = json!("        recent.append(now)\n");
            }),
            "cannot be undone with certainty",
        ),
        (
            first(&|mutation| mutation["consequence"] = json!("  ")),
            "give the consequence and the explanation",
        ),
    ];
    for (bad, fragment) in cases {
        refused_then_accepted(bad, fragment).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_behavioural_defect_comes_with_a_test_or_the_reason_for_none() -> Result<()> {
    let cases = [
        (
            answer_with(|mutations| {
                mutations[0]["killingTest"] = json!("");
                mutations[0]["noTestBecause"] = json!("");
            }),
            "give a killing test, or in `noTestBecause` the reason",
        ),
        (
            answer_with(|mutations| {
                mutations[0]["noTestBecause"] = json!("A race cannot be seen.");
            }),
            "either `killingTest` or `noTestBecause`, not both",
        ),
    ];
    for (bad, fragment) in cases {
        refused_then_accepted(bad, fragment).await?;
    }
    let pool = pool().await?;
    let reason = answer_with(|mutations| {
        mutations[0]["killingTest"] = json!("");
        mutations[0]["noTestBecause"] = json!(" No test can observe this. ");
    });
    let model = ScriptedModel::new([reason]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let planted = build(&pool, &model, Some(&runner), &grader)
        .plant(&spec(), &baseline())
        .await?;
    assert_eq!(
        planted.mutations[0].verification,
        Verification::Unobservable("No test can observe this.".into())
    );
    assert_eq!(planted.mutations[1].verification, Verification::NotAsked);
    Ok(())
}

#[tokio::test]
async fn a_mutation_refused_twice_fails_the_stage() -> Result<()> {
    let pool = pool().await?;
    let missing = || answer_with(|mutations| mutations.truncate(1));
    let model = ScriptedModel::new([missing(), missing()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .plant(&spec(), &baseline())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("readability.magic_number")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_testing_pattern_changes_a_test_file() -> Result<()> {
    let pool = pool().await?;
    let mut spec = spec();
    spec.patterns = vec!["testing.assertion_free_test".into()];
    let mut baseline = baseline();
    baseline.files.push(DrillFile {
        path: "tests/test_limiter.py".into(),
        text: "from src.limiter import RateLimiter\n\n\ndef test_it():\n    assert RateLimiter(1, 1, lambda: 0).allow(\"a\")\n".into(),
    });
    baseline.tests = vec!["tests/test_limiter.py".into()];
    let mutation = |path: &str, before: &str, after: &str| {
        json!({"mutations": [{
            "pattern": "testing.assertion_free_test", "path": path,
            "before": before, "after": after, "severity": "minor",
            "consequence": "The test passes whatever the limiter does.",
            "explanation": "The test calls the limiter and checks nothing. Assert on the result.",
            "killingTest": "", "noTestBecause": ""
        }]})
        .to_string()
    };
    let elsewhere = mutation(
        "src/limiter.py",
        "        self.limit = limit\n",
        "        self.limit = limit or 1\n",
    );
    let weakened = mutation(
        "tests/test_limiter.py",
        "    assert RateLimiter(1, 1, lambda: 0).allow(\"a\")\n",
        "    RateLimiter(1, 1, lambda: 0).allow(\"a\")\n",
    );
    let model = ScriptedModel::new([elsewhere, weakened]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let planted = build(&pool, &model, Some(&runner), &grader)
        .plant(&spec, &baseline)
        .await?;
    assert_eq!(planted.mutations[0].edit.path, "tests/test_limiter.py");
    assert_eq!(planted.mutations[0].verification, Verification::NotAsked);
    let requests = model.requests();
    assert!(system(&requests[0]).contains("edits a test file named in the request"));
    assert!(messages(&requests[0])[1]
        .1
        .contains("tests/test_limiter.py"));
    let reask = last(&requests[1]);
    assert!(
        reask.contains("a change for a testing pattern edits a test file (tests/test_limiter.py)"),
        "{reask}"
    );
    Ok(())
}
