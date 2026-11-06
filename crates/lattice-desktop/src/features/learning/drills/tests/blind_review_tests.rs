//! Stage 5: a fresh reviewer, told nothing of the key; the grader's judgement of its findings; the extras that join
//! the key; and how findable each planted issue was.

use super::support::{
    baseline, blind_answer, blind_assessment, build, entry_answer, files, idle_grader, last,
    messages, proved, system, FakeGrader, ScriptedModel, LITERALS, OPERATOR,
};
use crate::features::learning::drills::build::Proved;
use crate::features::learning::tests::pool;
use lattice_core::domain::learning::drills::{
    check_key, Assessment, Drill, FindingOutcome, IssueOutcome, LineSpan, Origin, Proof, Severity,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

fn span(line: u32) -> LineSpan {
    LineSpan {
        path: "src/limiter.py".into(),
        start: line,
        end: line,
    }
}

fn pop_finding() -> Value {
    json!({
        "path": "src/limiter.py", "quote": "self.hits.pop(key, None)",
        "comment": "Nothing calls forget, so the dictionary grows without bound.", "severity": "minor"
    })
}

#[tokio::test]
async fn the_reviewer_reads_the_drills_code_and_is_told_nothing_of_the_key() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([blind_answer(), entry_answer()]);
    let grader = FakeGrader::new(blind_assessment());
    build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await?;
    let request = &model.requests()[0];
    let system = system(request).to_lowercase();
    for word in ["plant", "mutat", "exercise", "key"] {
        assert!(!system.contains(word), "{word}");
    }
    let sent = messages(request);
    assert_eq!(sent.len(), 2);
    let user = &sent[1].1;
    assert!(
        user.contains("if len(recent) > self.limit:"),
        "the drill's code"
    );
    assert!(
        !user.contains("if len(recent) >= self.limit:"),
        "not the clean code"
    );
    assert!(!user.contains(OPERATOR));
    assert!(!user.contains(LITERALS));
    Ok(())
}

#[tokio::test]
async fn a_quote_gives_its_lines_unless_it_is_missing_or_ambiguous_and_the_grader_sees_them(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([blind_answer(), entry_answer()]);
    let grader = FakeGrader::new(blind_assessment());
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await?;
    let spans: Vec<Option<LineSpan>> = blind
        .findings
        .iter()
        .map(|finding| finding.span.clone())
        .collect();
    assert_eq!(spans, [Some(span(13)), None, Some(span(21)), None]);
    let seen = grader.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].issues, ["planted-1", "planted-2"]);
    assert_eq!(seen[0].files, proved().files);
    assert_eq!(seen[0].clean, files(), "the clean code is the baseline");
    let findings = &seen[0].findings;
    assert_eq!(findings.len(), 4);
    assert_eq!(findings[0].span, Some(span(13)));
    assert_eq!(
        findings[0].text,
        "This lets one call more than the limit through."
    );
    assert_eq!(findings[0].severity, Some(Severity::Major));
    assert_eq!(findings[1].span, None);
    assert_eq!(findings[3].span, None);
    Ok(())
}

#[tokio::test]
async fn a_valid_extra_becomes_a_found_in_review_issue_with_its_span_from_its_fix() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([blind_answer(), entry_answer()]);
    let grader = FakeGrader::new(blind_assessment());
    let proved = proved();
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved)
        .await?;
    assert_eq!(blind.extras.len(), 1);
    let extra = &blind.extras[0];
    assert_eq!(extra.id, "review-1");
    assert_eq!(extra.pattern, "resources.unbounded_growth");
    assert_eq!(extra.severity, Severity::Minor);
    assert_eq!(extra.origin, Origin::FoundInReview);
    assert_eq!(
        extra.proof,
        Proof::Judged {
            reason: "The dictionary really does grow without bound.".into()
        }
    );
    assert_eq!(extra.span, span(21));
    assert_eq!(extra.fix.before, "        self.hits.pop(key, None)\n");
    let purposes: Vec<&str> = blind
        .trail
        .calls
        .iter()
        .map(|call| call.purpose.as_str())
        .collect();
    assert_eq!(purposes, ["blind review", "key entry"]);
    assert_eq!(model.purposes(), ["blind review", "key entry"]);

    let mut key = proved.planted_key();
    key.extend(blind.extras.clone());
    let drill = Drill {
        id: "d".into(),
        language: "python".into(),
        title: "A limiter".into(),
        files: proved.files.clone(),
        key,
    };
    assert_eq!(
        check_key(&files(), &drill),
        Ok(()),
        "an issue found in review may overlap a planted span"
    );

    let entry = &model.requests()[1];
    let sent = messages(entry);
    assert!(sent[1].1.contains("Nothing calls forget"));
    assert!(sent[1]
        .1
        .contains("The dictionary really does grow without bound."));
    assert!(sent[1]
        .1
        .contains("resources.unbounded_growth (Unbounded growth)"));
    let schema = entry.json_schema.clone().unwrap();
    let ids = schema["properties"]["pattern"]["enum"].as_array().unwrap();
    assert!(ids.contains(&json!("none")));
    assert!(
        ids.contains(&json!("correctness.mutable_default_argument")),
        "a Python pattern"
    );
    assert!(
        !ids.contains(&json!("errors.unwrap_on_fallible")),
        "a Rust pattern"
    );
    Ok(())
}

#[tokio::test]
async fn how_findable_each_planted_issue_was_is_kept() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([blind_answer(), entry_answer()]);
    let grader = FakeGrader::new(blind_assessment());
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await?;
    let findable: Vec<(&str, &str, IssueOutcome)> = blind
        .findability
        .iter()
        .map(|item| (item.issue.as_str(), item.pattern.as_str(), item.outcome))
        .collect();
    assert_eq!(
        findable,
        [
            ("planted-1", OPERATOR, IssueOutcome::Found { finding: 0 }),
            ("planted-2", LITERALS, IssueOutcome::Missed)
        ]
    );
    assert_eq!(
        blind.findability[0].reason,
        "The comment names the operator."
    );
    assert_eq!(blind.assessment, Some(blind_assessment()));
    Ok(())
}

fn entry(pattern: &str, before: &str) -> String {
    json!({
        "pattern": pattern,
        "severity": "minor",
        "consequence": "Every key ever seen stays in memory.",
        "fix": {"path": "src/limiter.py", "before": before, "after": "        self.hits.clear()\n"},
        "explanation": "The dictionary only grows. Drop keys whose window has passed."
    })
    .to_string()
}

#[tokio::test]
async fn none_adds_nothing_to_the_key() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([blind_answer(), entry("none", "-")]);
    let grader = FakeGrader::new(blind_assessment());
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await?;
    assert!(blind.extras.is_empty());
    assert_eq!(model.purposes(), ["blind review", "key entry"]);
    Ok(())
}

#[tokio::test]
async fn an_entry_that_does_not_fit_the_code_is_refused_with_what_to_change() -> Result<()> {
    let cases = [
        (
            entry(
                "resources.unbounded_growth",
                "        self.hits.nothing()\n",
            ),
            "The fix: `before` is not in src/limiter.py",
        ),
        (
            entry(
                "resources.unbounded_growth",
                "        self.hits[key] = recent\n",
            ),
            "`before` is in src/limiter.py 2 times",
        ),
        (
            entry(
                "errors.unwrap_on_fallible",
                "        self.hits.pop(key, None)\n",
            ),
            "errors.unwrap_on_fallible is not one of the patterns listed",
        ),
    ];
    for (bad, fragment) in cases {
        let pool = pool().await?;
        let model = ScriptedModel::new([blind_answer(), bad, entry_answer()]);
        let grader = FakeGrader::new(blind_assessment());
        let blind = build(&pool, &model, None, &grader)
            .blind_review(&super::support::spec(), &baseline(), &proved())
            .await?;
        assert_eq!(blind.extras.len(), 1, "{fragment}");
        let reask = last(&model.requests()[2]);
        assert!(
            reask.starts_with("That answer could not be used:"),
            "{reask}"
        );
        assert!(reask.contains(fragment), "{fragment} in {reask}");
    }
    Ok(())
}

#[tokio::test]
async fn an_entry_that_cannot_be_used_is_left_out_without_failing_the_build() -> Result<()> {
    let pool = pool().await?;
    let bad = || {
        entry(
            "resources.unbounded_growth",
            "        self.hits.nothing()\n",
        )
    };
    let model = ScriptedModel::new([blind_answer(), bad(), bad()]);
    let grader = FakeGrader::new(blind_assessment());
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await?;
    assert!(blind.extras.is_empty());
    let purposes: Vec<&str> = blind
        .trail
        .calls
        .iter()
        .map(|call| call.purpose.as_str())
        .collect();
    assert_eq!(purposes, ["blind review", "key entry (not used)"]);
    assert_eq!(blind.findings.len(), 4, "the review itself stands");
    Ok(())
}

#[tokio::test]
async fn two_findings_that_describe_one_fix_add_one_issue() -> Result<()> {
    let pool = pool().await?;
    let reviewer = json!({"findings": [pop_finding(), pop_finding()]}).to_string();
    let assessment = Assessment {
        issues: vec![
            (IssueOutcome::Missed, "Not mentioned.".into()),
            (IssueOutcome::Missed, "Not mentioned.".into()),
        ],
        findings: vec![
            (FindingOutcome::ValidExtra, "It grows.".into()),
            (FindingOutcome::ValidExtra, "It grows without bound.".into()),
        ],
    };
    let model = ScriptedModel::new([reviewer, entry_answer(), entry_answer()]);
    let grader = FakeGrader::new(assessment);
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await?;
    assert_eq!(blind.extras.len(), 1);
    assert_eq!(blind.extras[0].id, "review-1");
    assert_eq!(model.purposes(), ["blind review", "key entry", "key entry"]);
    Ok(())
}

#[tokio::test]
async fn a_review_that_found_nothing_is_still_judged_and_every_planted_issue_is_missed(
) -> Result<()> {
    let pool = pool().await?;
    let assessment = Assessment {
        issues: vec![
            (IssueOutcome::Missed, "No finding.".into()),
            (IssueOutcome::Missed, "No finding.".into()),
        ],
        findings: Vec::new(),
    };
    let model = ScriptedModel::new([json!({"findings": []}).to_string()]);
    let grader = FakeGrader::new(assessment);
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await?;
    assert!(blind.findings.is_empty());
    assert!(blind.extras.is_empty());
    assert!(blind
        .findability
        .iter()
        .all(|item| item.outcome == IssueOutcome::Missed));
    assert_eq!(grader.seen().len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_assessment_that_is_not_consistent_with_the_review_fails_the_stage() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([blind_answer()]);
    let grader = FakeGrader::new(Assessment {
        issues: Vec::new(),
        findings: Vec::new(),
    });
    let error = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidData(message) if message.contains("not consistent")),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_grader_that_cannot_grade_fails_the_stage_with_its_error() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([blind_answer()]);
    let grader = FakeGrader::failing("the judge is down");
    let error = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ServiceNotAvailable(message) if message == "the judge is down"),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_review_refused_twice_fails_the_stage() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new(["not json".to_owned(), "still not json".to_owned()]);
    let grader = idle_grader();
    let error = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &proved())
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2);
    assert!(grader.seen().is_empty());
    Ok(())
}

#[tokio::test]
async fn with_no_planted_issue_there_is_no_key_to_judge_against_and_nothing_is_asked() -> Result<()>
{
    let pool = pool().await?;
    let model = ScriptedModel::new([]);
    let grader = idle_grader();
    let nothing = Proved {
        proven: Vec::new(),
        dropped: Vec::new(),
        files: files(),
        spans: Vec::new(),
    };
    let blind = build(&pool, &model, None, &grader)
        .blind_review(&super::support::spec(), &baseline(), &nothing)
        .await?;
    assert_eq!(blind, super::support::quiet());
    assert!(model.requests().is_empty());
    assert!(grader.seen().is_empty());
    Ok(())
}
