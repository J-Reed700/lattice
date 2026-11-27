//! Quiz grading-set hooks split out of `live_course_eval_quiz.rs`.

use super::super::*;
use super::{eval_candidate, eval_response, possible_points};
use crate::features::learning::assessment_engine::{
    self as engine, KeyedOutcome, LearningAssessmentCandidate, LearningAssessmentKey,
    LearningItemFormat, LearningRubricCriterion,
};
use crate::features::learning::assessment_generation::{
    self as generation, RawGrade, ShortAnswer, WrittenAnswer,
};
use crate::features::learning::grade_rationale::{self, Checked, Grader, GraderVerdict, Reference};

const GRADES_SCHEMA: &str = "quiz-grading-grades/1";
const GRADING_DIR: &str = "evals/courses/grading";
/// Section 7.2: each contested answer is regraded three times.
const REGRADES: usize = 3;

fn verdict_name(verdict: GraderVerdict) -> &'static str {
    match verdict {
        GraderVerdict::Met => "met",
        GraderVerdict::PartlyMet => "partly_met",
        GraderVerdict::Missed => "missed",
    }
}

/// The model's own answer before any check, in the rationale's shape, so the
/// scorer measures what the model did rather than what the checks repaired.
fn raw_json(raw: &RawGrade, rubric: &[LearningRubricCriterion], version: Option<&str>) -> Value {
    json!({
        "summary": raw.item.summary,
        "criteria": raw.item.criteria.iter().map(|c| {
            let criterion = rubric.iter().find(|r| r.id == c.criterion_id);
            json!({
                "criterion_id": c.criterion_id, "title": criterion.map(|r| r.title.as_str()),
                "verdict": verdict_name(c.verdict),
                "max_points": criterion.map(|r| r.max_points),
                "what_was_missing": c.what_was_missing, "your_words": c.your_words,
                "full_credit_includes": c.full_credit_includes,
                "source_quote": c.source_quote.as_deref().filter(|q| !q.trim().is_empty()),
                "source_version_id": version,
            })
        }).collect::<Vec<_>>(),
    })
}

fn shown_json(rationale: &LearningGradeRationaleDto) -> Value {
    json!({
        "summary": rationale.summary,
        "criteria": rationale.criteria.iter().map(|c| json!({
            "criterion_id": c.criterion_id, "title": c.title, "verdict": c.verdict, "points": c.points,
            "max_points": c.max_points, "what_was_missing": c.what_was_missing, "your_words": c.your_words,
            "full_credit_includes": c.full_credit_includes, "source_quote": c.source_quote,
            "source_version_id": c.source_version_id,
        })).collect::<Vec<_>>(),
        "grader_model": rationale.grader_model, "rubric_version": rationale.rubric_version,
        "graded_at": rationale.graded_at,
    })
}

/// One model grade, exactly as `quiz_bank_jobs::grade_written_answer` and
/// `judge_short_answer` give it (the model's grade, then
/// `grade_rationale::check`), unrolled so the raw output is kept.
async fn model_grade(
    llm: &dyn LLMPort,
    candidate: &LearningAssessmentCandidate,
    item: &Value,
    answer: &str,
    note: Option<&str>,
) -> Result<Value> {
    let quote = item["source_quote"].as_str().unwrap_or_default();
    let version = item["source_version_id"].as_str();
    let (raw, rubric, reference, version) = if candidate.format == LearningItemFormat::Explanation {
        let written = WrittenAnswer {
            item_id: candidate.id.clone(),
            prompt: candidate.prompt.clone(),
            rubric: candidate.rubric.clone(),
            answer: answer.to_owned(),
            reference: quote.to_owned(),
            learner_note: note.map(str::to_owned),
        };
        let raw = generation::grade_written(llm, std::slice::from_ref(&written)).await?;
        (raw, candidate.rubric.clone(), quote.to_owned(), version)
    } else {
        let LearningAssessmentKey::Text(key) = &candidate.key else {
            return Err(refuse(format!("{} is not model-graded", candidate.id)));
        };
        let reference = if quote.trim().is_empty() {
            candidate.explanation.clone()
        } else {
            quote.to_owned()
        };
        let short = ShortAnswer {
            item_id: candidate.id.clone(),
            prompt: candidate.prompt.clone(),
            accepted: key.variants.clone(),
            answer: answer.to_owned(),
            reference: reference.clone(),
            learner_note: note.map(str::to_owned),
        };
        let raw = generation::judge_short_answers(llm, std::slice::from_ref(&short)).await?;
        (raw, grade_rationale::equivalence_rubric(), reference, None)
    };
    let raw = raw.into_iter().find(|r| r.item_id == candidate.id);
    let checked = match &raw {
        None => Checked::Unexplained(grade_rationale::UNEXPLAINED.into()),
        Some(raw) => grade_rationale::check(
            &rubric,
            answer,
            (!reference.trim().is_empty()).then_some(Reference {
                text: &reference,
                version_id: version,
            }),
            raw.item.clone(),
            &Grader {
                model: llm.model_name(),
                graded_at: now_ms() / 1000,
            },
        ),
    };
    let possible = possible_points(candidate, item);
    let mut entry = match checked {
        Checked::Graded {
            rationale,
            fraction,
            status,
        } => json!({
            "status": "graded", "error": null, "points_earned": fraction * possible,
            "points_possible": possible, "grade_status": status, "rationale": shown_json(&rationale),
        }),
        Checked::Unexplained(reason) => json!({
            "status": "couldnt_grade", "error": reason, "points_earned": null, "points_possible": possible,
            "grade_status": "pending", "rationale": {},
        }),
    };
    entry["path"] = json!("model");
    entry["raw"] = raw
        .as_ref()
        .map_or(Value::Null, |r| raw_json(r, &rubric, version));
    entry["response_to_note"] = json!(raw.and_then(|r| r.response_to_note));
    Ok(entry)
}

/// Every grade of one row: its first grade and, for a contested model-graded
/// answer, three regrades with the note given as data.
async fn grade_row(llm: &dyn LLMPort, row: &Value, item: &Value) -> Result<Vec<Value>> {
    let candidate = eval_candidate(item)?;
    let response = eval_response(&candidate.id, &row["response"]);
    let answer = response.text.clone().unwrap_or_default();
    let possible = possible_points(&candidate, item);
    let keyed = if candidate.format == LearningItemFormat::Explanation {
        // An empty written answer scores 0 in code, never at the model.
        (!response.is_answered()).then(|| (0.0, engine::NO_ANSWER.to_owned()))
    } else {
        match engine::grade_keyed(&candidate, &response)? {
            KeyedOutcome::Graded(grade) => Some((grade.fraction, grade.detail)),
            KeyedOutcome::NeedsJudge => None,
        }
    };
    let id = row["id"].clone();
    if let Some((fraction, detail)) = keyed {
        return Ok(vec![json!({
            "row_id": id, "pass": "first", "seed": 0, "path": "deterministic", "status": "graded",
            "error": null, "points_earned": fraction * possible, "points_possible": possible,
            "grade_status": "deterministic", "detail": detail, "rationale": {}, "raw": null,
            "response_to_note": null,
        })]);
    }
    let mut first = model_grade(llm, &candidate, item, &answer, None).await?;
    first["row_id"] = id.clone();
    first["pass"] = json!("first");
    first["seed"] = json!(0);
    let mut grades = vec![first];
    if let Some(note) = row["contest_note"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
    {
        for seed in 0..REGRADES {
            let mut regrade = model_grade(llm, &candidate, item, &answer, Some(note)).await?;
            regrade["row_id"] = id.clone();
            regrade["pass"] = json!("regrade");
            regrade["seed"] = json!(seed);
            grades.push(regrade);
        }
    }
    Ok(grades)
}

/// The rows and bank of one registered grading set, with their file hashes.
pub(in super::super) fn grading_set(name: &str) -> Result<(Vec<Value>, Value, String, String)> {
    let dir = repo_root().join(GRADING_DIR);
    let registry = read_json(&dir.join("sets.json"))?
        .ok_or_else(|| refuse("evals/courses/grading/sets.json is missing"))?;
    let entry = &registry["sets"][name];
    let (Some(rows), Some(bank)) = (entry["rows"].as_str(), entry["bank"].as_str()) else {
        return Err(refuse(format!("no grading set {name:?} in sets.json")));
    };
    let rows_path = dir.join(rows);
    let lines = std::fs::read_to_string(&rows_path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let bank_path = dir.join(bank);
    let bank = read_json(&bank_path)?.ok_or_else(|| refuse(format!("{bank} is missing")))?;
    Ok((
        lines,
        bank,
        file_sha256(&rows_path)?,
        file_sha256(&bank_path)?,
    ))
}

#[tokio::test]
#[ignore = "quiz grading set: requires LATTICE_COURSE_RECIPE and LATTICE_QUIZ_GRADING_RUN; makes real model requests"]
async fn live_quiz_grading_eval() -> Result<()> {
    let env = |name: &str| std::env::var(name).ok();
    let required = |name: &str| env(name).ok_or_else(|| refuse(format!("{name} is not set")));
    let recipe_text = std::fs::read_to_string(required("LATTICE_COURSE_RECIPE")?)?;
    let recipe = CourseRecipe::parse(&recipe_text)?;
    let set = env("LATTICE_QUIZ_GRADING_SET").unwrap_or_else(|| "quiz-grading-v1".into());
    let dir = PathBuf::from(required("LATTICE_QUIZ_GRADING_RUN")?);
    std::fs::create_dir_all(&dir)?;
    let (rows, bank, rows_sha, bank_sha) = grading_set(&set)?;
    let items: BTreeMap<String, Value> = bank["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(|id| (id.to_owned(), item.clone())))
        .collect();
    let (url, header) = recipe.connection(&env)?;
    let settings = recipe.llm_settings(&url, header.as_ref())?;
    let identity = probe(&settings, header.as_ref(), recipe.server.expected_slots).await?;
    let model =
        ObservedModel::with_header(&settings, header, dir.clone(), next_call(&dir)?).await?;
    let recipe_sha = sha256_hex(recipe_text.as_bytes());
    let previous = read_json(&dir.join("run.json"))?;
    if let Some(previous) = &previous {
        if previous["recipe_sha256"] != recipe_sha.as_str()
            || previous["rows_sha256"] != rows_sha.as_str()
        {
            return Err(refuse(
                "this run directory graded another recipe or set version",
            ));
        }
    }
    let mut manifest = json!({
        "schema": "quiz-grading-run/1", "set": set, "rows_sha256": rows_sha, "bank_sha256": bank_sha,
        "recipe_sha256": recipe_sha, "recipe": serde_json::from_str::<Value>(&recipe_text)?,
        "binary_sha256": file_sha256(&std::env::current_exe()?)?, "commit": git(&["rev-parse", "HEAD"]),
        "server": {"identity": identity, "url_sha256": sha256_hex(url.as_bytes()), "thinking": recipe.server.thinking},
        "settings": recorded_settings(&settings)?,
        "started_at": previous.as_ref().map_or(json!(chrono::Utc::now().to_rfc3339()), |p| p["started_at"].clone()),
        "finished_at": null,
    });
    write_json(&dir.join("run.json"), &manifest)?;

    // Resumable: a row's grades are appended together once all are in.
    let partial = dir.join("grades.partial.jsonl");
    let mut grades: Vec<Value> = match std::fs::read_to_string(&partial) {
        Ok(text) => text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<_, _>>()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let done: HashSet<String> = grades
        .iter()
        .filter_map(|g| g["row_id"].as_str().map(str::to_owned))
        .collect();
    for row in &rows {
        let id = row["id"].as_str().unwrap_or_default();
        if done.contains(id) {
            continue;
        }
        let item = items
            .get(row["item"].as_str().unwrap_or_default())
            .ok_or_else(|| refuse(format!("{id}: its item is not in the bank")))?;
        let entries = grade_row(&model, row, item).await?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial)?;
        for entry in &entries {
            use std::io::Write;
            writeln!(file, "{entry}")?;
        }
        grades.extend(entries);
    }
    write_json(
        &dir.join("grades.json"),
        &json!({
            "schema": GRADES_SCHEMA, "set": set, "rows_sha256": rows_sha, "bank_sha256": bank_sha,
            "grader": {"model": recipe.server.model,
                       "model_revision": recipe.server.gguf_sha256.clone().or(recipe.server.model_revision.clone()),
                       "rubric_version": "per grade (grade_rationale::rubric_version)",
                       "recipe_sha256": manifest["recipe_sha256"], "commit": manifest["commit"]},
            "grades": grades,
        }),
    )?;
    manifest["finished_at"] = json!(chrono::Utc::now().to_rfc3339());
    write_json(&dir.join("run.json"), &manifest)?;
    println!(
        "QUIZ GRADING DONE: {} rows graded into {}",
        rows.len(),
        dir.join("grades.json").display()
    );
    Ok(())
}
