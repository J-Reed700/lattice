use super::stages::{lesson_metrics, phase_metrics};
use super::*;

fn recipe(server: Value, settings: Value) -> Result<CourseRecipe> {
    CourseRecipe::parse(
        &json!({
            "schema": "course-eval-recipe/1", "name": "test", "results_model": "test",
            "requests": "evals/courses/course-eval-v1.json", "lessons": ["m1.l1"],
            "embedding_gguf": "~/.cache/lattice/models/qwen3-embedding-0.6b/Qwen3-Embedding-0.6B-f16.gguf",
            "embedding_model_id": "qwen3-embedding-0.6b",
            "server": server, "settings": settings,
            "web": {"mode": "replay", "traces": "artifacts/course-eval/traces"},
        })
        .to_string(),
    )
}

fn local() -> Value {
    json!({"url": "http://127.0.0.1:8089", "model": "model.gguf", "gguf": "~/model.gguf", "gguf_sha256": "ab12",
           "launch": "llama-server -m model.gguf", "expected_ctx": 32768, "expected_slots": 1, "thinking": "off"})
}

fn remote() -> Value {
    json!({"url_env": "COURSE_URL", "auth_env": "COURSE_AUTH", "model": "big", "model_revision": "tag-1",
           "expected_ctx": 131072, "expected_slots": 1, "thinking": "off"})
}

fn with(mut base: Value, patch: Value) -> Value {
    merge(&mut base, &patch);
    base
}

fn without(mut base: Value, field: &str) -> Value {
    base.as_object_mut().unwrap().remove(field);
    base
}

#[test]
fn a_recipe_names_its_server_exactly_one_way() {
    assert!(recipe(local(), json!({})).is_ok());
    assert!(recipe(remote(), json!({})).is_ok());
    assert!(recipe(with(local(), json!({"url_env": "COURSE_URL"})), json!({})).is_err());
    assert!(recipe(without(local(), "url"), json!({})).is_err());
}

#[test]
fn a_recipe_without_an_embedding_model_is_refused() {
    let recipe = |embedding: Value| {
        let mut text = json!({
            "schema": "course-eval-recipe/1", "lessons": [], "server": local(),
            "web": {"traces": "artifacts/course-eval/traces"},
        });
        merge(&mut text, &embedding);
        CourseRecipe::parse(&text.to_string())
    };
    let refused = recipe(json!({"embedding_model_id": "qwen3-embedding-0.6b"})).unwrap_err();
    assert!(refused.to_string().contains("embedding_gguf"));
    assert!(recipe(json!({"embedding_gguf": "~/m"})).is_err());
    assert!(recipe(json!({"embedding_gguf": " ", "embedding_model_id": "m"})).is_err());
    assert!(recipe(json!({"embedding_gguf": "~/m", "embedding_model_id": "m"})).is_ok());
}

#[test]
fn a_recipe_without_weights_needs_a_revision() {
    assert!(recipe(without(remote(), "model_revision"), json!({})).is_err());
    assert!(recipe(with(remote(), json!({"model_revision": "  "})), json!({})).is_err());
    assert!(recipe(without(local(), "gguf_sha256"), json!({})).is_err());
}

#[test]
fn recipe_settings_merge_over_the_shipped_defaults() {
    let patched = recipe(
        local(),
        json!({"llm": {"contextWindow": 16384, "temperature": 0.25}}),
    )
    .unwrap();
    let settings = patched.llm_settings("http://127.0.0.1:8089", None).unwrap();
    assert!(matches!(settings.provider, LLMProvider::Llamacpp));
    assert_eq!(settings.context_window, 16384);
    assert_eq!(settings.llama_cpp.url, "http://127.0.0.1:8089");
    assert_eq!(settings.llama_cpp.model, "model.gguf");
    assert!((settings.temperature - 0.25).abs() < f32::EPSILON);
    let unpatched = recipe(local(), json!({})).unwrap();
    assert_eq!(
        unpatched
            .llm_settings("http://h", None)
            .unwrap()
            .context_window,
        32768
    );
}

#[test]
fn an_unknown_or_harness_owned_setting_is_refused() {
    for settings in [
        json!({"llm": {"contextWindw": 1}}),
        json!({"llm": {"verification": {"noSuchField": true}}}),
        json!({"llm": {"llamaCpp": {"url": "http://elsewhere"}}}),
        json!({"llm": {"provider": "ollama"}}),
        json!({"chat": {}}),
        json!([]),
    ] {
        let recipe = recipe(local(), settings.clone()).unwrap();
        assert!(recipe.llm_settings("http://h", None).is_err(), "{settings}");
    }
}

#[test]
fn the_recipe_thinking_must_agree_with_the_settings() {
    let off = |settings: Value| {
        recipe(local(), settings)
            .unwrap()
            .llm_settings("http://h", None)
    };
    assert!(off(json!({})).is_ok());
    assert!(off(json!({"llm": {"thinkingOverride": "off"}})).is_ok());
    assert!(off(json!({"llm": {"thinkingOverride": "on"}})).is_err());
    assert!(off(json!({"llm": {"answerThinking": true}})).is_err());
    let on = |settings: Value| {
        recipe(with(local(), json!({"thinking": "on"})), settings)
            .unwrap()
            .llm_settings("http://h", None)
    };
    assert!(on(json!({})).is_err());
    assert!(on(json!({"llm": {"thinkingOverride": "on"}})).is_ok());
    assert!(recipe(with(local(), json!({"thinking": "as-sent"})), json!({})).is_err());
}

#[test]
fn a_remote_server_and_its_key_come_from_the_named_variables_and_stay_out_of_records() {
    let recipe = recipe(remote(), json!({})).unwrap();
    let env = |name: &str| match name {
        "COURSE_URL" => Some("https://models.example".to_owned()),
        "COURSE_AUTH" => Some("X-Key: secret-value".to_owned()),
        _ => None,
    };
    let (url, auth) = recipe.connection(&env).unwrap();
    assert_eq!(url, "https://models.example");
    assert_eq!(auth, Some(("X-Key".to_owned(), "secret-value".to_owned())));
    let settings = recipe.llm_settings(&url, auth.as_ref()).unwrap();
    assert_eq!(settings.llama_cpp.auth_header_name, "X-Key");
    let recorded = recorded_settings(&settings).unwrap().to_string();
    assert!(!recorded.contains("secret-value") && !recorded.contains("models.example"));
    assert!(recipe.connection(&|_| None).is_err());
    let malformed = |name: &str| match name {
        "COURSE_URL" => Some("https://models.example".to_owned()),
        "COURSE_AUTH" => Some("no separator".to_owned()),
        _ => None,
    };
    assert!(recipe.connection(&malformed).is_err());
}

#[test]
fn lesson_selectors_count_from_one() {
    assert_eq!(parse_selector("m1.l1").unwrap(), (0, 0));
    assert_eq!(parse_selector("m12.l3").unwrap(), (11, 2));
    for bad in [
        "m0.l1", "m1.l0", "m1", "l1.m1", "m1.l", "m1.lx", "M1.L1", "m1.l1.x", "m-1.l1", " m1.l1",
        "",
    ] {
        assert!(parse_selector(bad).is_err(), "{bad:?}");
    }
    assert!(parse_selectors("").unwrap().is_empty());
    assert_eq!(
        parse_selectors("m1.l1, m2.l1,m1.l1").unwrap(),
        vec!["m1.l1", "m2.l1"]
    );
    assert!(parse_selectors("m1.l1,lesson-2").is_err());
    assert!(recipe(local(), json!({})).is_ok());
    let bad_lessons = json!({
        "schema": "course-eval-recipe/1", "lessons": ["first"], "server": local(),
        "embedding_gguf": "~/m", "embedding_model_id": "m",
        "web": {"traces": "artifacts/course-eval/traces"},
    });
    assert!(CourseRecipe::parse(&bad_lessons.to_string()).is_err());
}

fn lesson(blocks: Value, questions: Value) -> LearningLessonDto {
    serde_json::from_value(json!({
        "id": "lesson-1", "title": "Reading plans", "objective": "Read a plan", "estimatedMinutes": 30,
        "preparation": "ready", "blocks": blocks, "questions": questions, "completed": false,
    }))
    .unwrap()
}

#[test]
fn lesson_metrics_count_the_report_and_catch_what_its_gates_should_have() {
    let lesson = lesson(
        json!([
            {"kind": "explanation", "title": "a", "body": "b", "sourceIds": ["s1", "s9"]},
            {"kind": "worked_example", "title": "c", "body": "d", "sourceIds": ["s1"]},
        ]),
        json!([{"id": "q1", "kind": "quiz", "prompt": "p", "options": ["x", "y"], "sourceIds": ["s1"]}]),
    );
    let passage = |text: &str| json!({"source_id": "s1", "text": text, "start_byte": 0, "end_byte": 1, "retrieval_kind": "lexical", "score": 1.0});
    let report = json!({
        "findings": [
            {"verdict": "supported", "supporting_quote": "indexes speed reads", "evidence": [passage("B-tree indexes speed reads of sorted keys.")]},
            {"verdict": "supported", "supporting_quote": "written nowhere", "evidence": [passage("Something else.")]},
            {"verdict": "supported", "supporting_quote": null, "evidence": []},
            {"verdict": "unsupported", "resolution": {"kind": "general_knowledge", "at": 1}, "evidence": []},
            {"verdict": "unsupported", "evidence": []},
            {"verdict": "contradicted", "evidence": []},
            {"verdict": "unverified", "evidence": []},
        ],
        "executions": [
            {"result": {"status": "passed"}}, {"result": {"status": "failed"}},
            {"result": {"status": "timed_out"}}, {"result": {"status": "runtime_unavailable"}},
        ],
        "issues": ["one open issue"],
    });
    let known: HashSet<String> = ["s1".to_owned()].into();
    let metrics = lesson_metrics(&lesson, Some(&report), &known);
    assert_eq!(
        metrics["claims"],
        json!({"total": 7, "supported": 3, "unsupported": 2, "contradicted": 1, "unverified": 1,
               "general_knowledge": 1, "quotes_unmatched": 1, "issues": 1})
    );
    assert_eq!(metrics["examples"], json!({"run": 3, "failed": 2}));
    assert_eq!(
        metrics["citations"],
        json!({"source_ids": 2, "unresolved": 1})
    );
    assert_eq!(
        metrics["shape"],
        json!({"blocks": 2, "kinds": ["explanation", "worked_example"], "questions": 1})
    );
    let unreported = lesson_metrics(&lesson, None, &known);
    assert!(unreported["claims"].is_null() && unreported["examples"].is_null());
}

#[test]
fn phases_give_deferrals_rounds_and_seconds_per_phase() {
    let line = |at: i64, status: &str, phase: Option<&str>, page: Option<u32>| json!({"at": at, "selector": "m1.l1", "status": status, "phase": phase, "message": "", "deeper_page": page, "calls": 0});
    let lines = vec![
        line(0, "pending", None, None),
        line(1_000, "running", Some("references"), None),
        line(4_000, "running", Some("repair"), None),
        line(5_000, "pending", Some("repair"), None),
        line(9_000, "running", Some("repair"), None),
        line(10_000, "running", Some("evidence"), Some(1)),
        line(12_000, "running", Some("evidence"), Some(2)),
        line(13_000, "running", Some("repair"), Some(2)),
        line(15_000, "completed", Some("publishing"), None),
        json!({"at": 0, "selector": "m2.l1", "status": "pending"}),
    ];
    let metrics = phase_metrics(&lines, "m1.l1");
    assert_eq!(metrics["deferrals"], 1);
    assert_eq!(metrics["rounds"], json!({"repair": 2, "deepening": 2}));
    assert_eq!(
        metrics["phase_seconds"],
        json!({"references": 3.0, "repair": 4.0, "evidence": 3.0})
    );
}

#[test]
fn the_outline_resumes_from_what_is_saved() {
    use LearningOutlineReviewStatus as Review;
    use LearningProgramStatus as Program;
    assert_eq!(outline_step(None, false), OutlineStep::Generate);
    assert_eq!(outline_step(None, true), OutlineStep::Generate);
    for recorded in [false, true] {
        assert_eq!(
            outline_step(Some((&Program::Draft, Some(&Review::Passed))), recorded),
            OutlineStep::Accept
        );
        assert_eq!(
            outline_step(Some((&Program::Active, None)), recorded),
            OutlineStep::Accepted
        );
    }
    for review in [Some(&Review::Unchecked), Some(&Review::NeedsRepair), None] {
        assert_eq!(
            outline_step(Some((&Program::Draft, review)), false),
            OutlineStep::Repair
        );
        assert_eq!(
            outline_step(Some((&Program::Draft, review)), true),
            OutlineStep::Recorded
        );
    }
}

#[test]
fn a_lesson_resumes_from_its_job() {
    use LearningGenerationJobStatus as Status;
    let outage = AppError::Network("connection reset".into()).to_string();
    let busy = AppError::RateLimitExceeded("slow down".into()).to_string();
    let away = AppError::ServiceNotAvailable("llama.cpp: restarting".into()).to_string();
    let refused = AppError::ModelRejected("the request was refused".into()).to_string();
    assert_eq!(lesson_step(false, None), LessonStep::Start);
    assert_eq!(
        lesson_step(true, Some((Status::Running, None))),
        LessonStep::Recorded
    );
    for status in [Status::Pending, Status::Running] {
        assert_eq!(
            lesson_step(false, Some((status, None))),
            LessonStep::Dispatch
        );
    }
    assert_eq!(
        lesson_step(false, Some((Status::Interrupted, None))),
        LessonStep::Retry
    );
    for transient in [&outage, &busy, &away] {
        assert_eq!(
            lesson_step(false, Some((Status::Failed, Some(transient.as_str())))),
            LessonStep::Retry
        );
    }
    assert_eq!(
        lesson_step(false, Some((Status::Failed, Some(refused.as_str())))),
        LessonStep::Record
    );
    for status in [Status::Failed, Status::Completed, Status::Cancelled] {
        assert_eq!(lesson_step(false, Some((status, None))), LessonStep::Record);
    }
}
