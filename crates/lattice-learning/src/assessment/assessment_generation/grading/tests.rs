#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::course_generation_tests::RequestLog;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{
    tasks_in, ScriptedModel, Sent,
};
use std::sync::{Arc, Mutex};

const ANSWER: &str = "The kiln fires every bowl. The damper is shut before the glaze sets.";

const REFERENCE: &str = "A kiln fires bowls once the damper is shut. The glaze sets after the kiln cools. Tide tables are revised each spring.";

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn ended(text: &str, finish: &str) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse {
        finish_reason: finish.into(),
        ..CompletionResponse::from_text(text)
    })))
}

/// The last task a grading prompt carries: a re-ask restates it for the items still open.
fn task_of(prompt: &str) -> Value {
    tasks_in(prompt)
        .into_iter()
        .rfind(|task| task.get("items").is_some())
        .unwrap_or(Value::Null)
}

/// What a request shows about one item.
#[derive(Debug, Clone)]
struct Shows {
    item: String,
    criteria: Vec<String>,
    answer: Vec<(String, String)>,
    reference: Vec<(String, String)>,
}

impl Shows {
    /// The label of the `n`th sentence of the answer, from 0.
    fn sentence(&self, n: usize) -> String {
        self.answer
            .get(n)
            .map(|(label, _)| label.clone())
            .unwrap_or_default()
    }

    /// The label of the `n`th sentence of the reference, from 0.
    fn source(&self, n: usize) -> String {
        self.reference
            .get(n)
            .map(|(label, _)| label.clone())
            .unwrap_or_default()
    }
}

/// Rows `[label] sentence`, one to a line, as the label and the sentence.
fn rows_of(rows: &str) -> Vec<(String, String)> {
    rows.lines()
        .filter_map(|line| {
            let (label, text) = line.strip_prefix('[')?.split_once("] ")?;
            Some((label.to_owned(), text.to_owned()))
        })
        .collect()
}

fn shown(sent: &Sent) -> Vec<Shows> {
    let text = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_owned();
    task_of(&sent.prompt())["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|item| Shows {
            item: text(item, "itemId"),
            criteria: item["rubric"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|criterion| text(criterion, "id"))
                .collect(),
            answer: rows_of(&text(item, "answer")),
            reference: rows_of(&text(item, "reference")),
        })
        .collect()
}

/// A model that answers every request with the entries `decide` gives for the items it shows.
fn answering<C: Contract>(
    decide: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
) -> ScriptedModel {
    ScriptedModel::new().on_request::<C>(move |sent, _| {
        let entries: Vec<Value> = shown(sent).iter().filter_map(&decide).collect();
        reply(json!({ "entries": entries }).to_string())
    })
}

/// The criteria of one item as the grader writes them: every one judged `verdict`, resting on `words` and `source`.
fn marks(shows: &Shows, verdict: &str, words: &[String], source: &[String]) -> Vec<Value> {
    shows
        .criteria
        .iter()
        .map(|criterion| {
            json!({"of": criterion, "yourWords": words, "whatWasMissing": "The damper is not explained.",
                "fullCreditIncludes": "The damper keeps the heat in.", "sourceQuote": source, "verdict": verdict})
        })
        .collect()
}

fn entry(shows: &Shows, criteria: Vec<Value>, response: &str) -> Value {
    json!({"of": shows.item, "criteria": criteria, "summary": "The kiln is right.", "responseToNote": response})
}

fn criterion(id: &str) -> LearningRubricCriterion {
    LearningRubricCriterion {
        id: id.into(),
        title: format!("Title of {}", id.len()),
        description: "Explains it.".into(),
        max_points: 2,
    }
}

fn written(id: &str, note: Option<&str>) -> WrittenAnswer {
    WrittenAnswer {
        item_id: id.into(),
        prompt: "Why does the damper stay shut?".into(),
        rubric: vec![criterion("kiln-criterion"), criterion("damper-criterion")],
        answer: ANSWER.into(),
        reference: REFERENCE.into(),
        learner_note: note.map(str::to_owned),
    }
}

fn short(id: &str) -> ShortAnswer {
    ShortAnswer {
        item_id: id.into(),
        prompt: "When did the mill close?".into(),
        accepted: vec!["1887".into()],
        answer: "In 1887.".into(),
        reference: "The mill closed in 1887.".into(),
        learner_note: None,
    }
}

fn judged(shows: &Shows, verdict: &str, words: &[String], response: &str) -> Value {
    json!({"of": shows.item, "yourWords": words, "whatWasMissing": "",
        "fullCreditIncludes": "The mill closed in 1887.", "verdict": verdict,
        "summary": "Same year.", "responseToNote": response})
}

fn item_ids(raw: &[RawGrade]) -> Vec<&str> {
    let mut ids: Vec<&str> = raw.iter().map(|grade| grade.item_id.as_str()).collect();
    ids.sort_unstable();
    ids
}

#[tokio::test]
async fn a_grade_is_filled_from_the_apps_own_words() -> Result<()> {
    let model = answering::<GradeWritten>(|shows| {
        let first = marks(
            shows,
            "met",
            &[shows.sentence(1), shows.sentence(0)],
            &[shows.source(1)],
        );
        Some(entry(shows, first, ""))
    });
    let graded = grade_written(&model, &[written("q1", None)]).await?;
    assert_eq!(model.sent().len(), 1);
    let [grade] = graded.as_slice() else {
        panic!("one grade expected: {graded:?}");
    };
    assert_eq!(grade.item_id, "q1");
    assert_eq!(grade.response_to_note, None);
    assert_eq!(grade.item.summary, "The kiln is right.");
    let ids: Vec<&str> = grade
        .item
        .criteria
        .iter()
        .map(|c| c.criterion_id.as_str())
        .collect();
    assert_eq!(ids, ["kiln-criterion", "damper-criterion"]);
    for criterion in &grade.item.criteria {
        assert_eq!(
            criterion.your_words,
            [
                "The kiln fires every bowl.",
                "The damper is shut before the glaze sets."
            ],
            "the words are the app's own sentences, in the order of the answer"
        );
        assert_eq!(
            criterion.source_quote.as_deref(),
            Some("The glaze sets after the kiln cools.")
        );
        assert_eq!(criterion.verdict, GraderVerdict::Met);
    }
    Ok(())
}

#[tokio::test]
async fn a_source_quote_runs_from_the_first_cited_sentence_to_the_last() -> Result<()> {
    let model = answering::<GradeWritten>(|shows| {
        let all = marks(shows, "met", &[], &[shows.source(2), shows.source(0)]);
        Some(entry(shows, all, ""))
    });
    let graded = grade_written(&model, &[written("q1", None)]).await?;
    for criterion in &graded[0].item.criteria {
        assert_eq!(criterion.source_quote.as_deref(), Some(REFERENCE));
        assert!(criterion.your_words.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn a_grade_that_cites_nothing_has_no_words_and_no_quote() -> Result<()> {
    let model = answering::<GradeWritten>(|shows| {
        let all = marks(shows, "missed", &[], &[]);
        Some(entry(shows, all, ""))
    });
    let graded = grade_written(&model, &[written("q1", None)]).await?;
    for criterion in &graded[0].item.criteria {
        assert!(criterion.your_words.is_empty());
        assert_eq!(criterion.source_quote, None);
        assert_eq!(criterion.verdict, GraderVerdict::Missed);
    }
    Ok(())
}

#[tokio::test]
async fn the_request_names_items_criteria_and_sentences_by_handle_and_asks_for_no_points(
) -> Result<()> {
    let model = answering::<GradeWritten>(|shows| {
        let all = marks(shows, "met", &[shows.sentence(0)], &[]);
        Some(entry(shows, all, ""))
    });
    grade_written(&model, &[written("question-id-100", None)]).await?;
    let sent = model.sent();
    let prompt = sent[0].prompt();
    for internal in ["question-id-100", "kiln-criterion", "damper-criterion"] {
        assert!(!prompt.contains(internal), "{internal} was shown: {prompt}");
    }
    let shows = shown(&sent[0]);
    assert_eq!(shows[0].criteria.len(), 2);
    assert_eq!(shows[0].answer.len(), 2);
    assert_eq!(shows[0].reference.len(), 3);
    let schema = sent[0]
        .request
        .schema()
        .map(|schema| schema.as_json().to_string())
        .unwrap_or_default();
    assert!(schema.contains("yourWords"), "{schema}");
    for gone in ["points", "criterionId", "itemId"] {
        assert!(!schema.contains(gone), "{gone} is in the schema: {schema}");
    }
    Ok(())
}

#[tokio::test]
async fn a_criterion_left_out_is_asked_for_again_with_its_item_alone() -> Result<()> {
    let model = ScriptedModel::new().on_request::<GradeWritten>(|sent, nth| {
        let entries: Vec<Value> = shown(sent)
            .iter()
            .map(|shows| {
                let mut criteria = marks(shows, "met", &[], &[]);
                if nth == 0 && shows.item == "i2" {
                    criteria.pop();
                }
                entry(shows, criteria, "")
            })
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let graded = grade_written(&model, &[written("q1", None), written("q2", None)]).await?;
    assert_eq!(item_ids(&graded), ["q1", "q2"]);
    for grade in &graded {
        assert_eq!(
            grade.item.criteria.len(),
            2,
            "{} has every criterion",
            grade.item_id
        );
    }
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    let again: Vec<String> = shown(&sent[1]).into_iter().map(|s| s.item).collect();
    assert_eq!(
        again,
        ["i2"],
        "only the item that was left out is asked again"
    );
    Ok(())
}

#[tokio::test]
async fn an_empty_reply_to_a_learners_note_is_corrected_for_its_item_alone() -> Result<()> {
    let model = ScriptedModel::new().on_request::<GradeWritten>(|sent, nth| {
        let entries: Vec<Value> = shown(sent)
            .iter()
            .map(|shows| {
                let response = if nth == 0 && shows.item == "i1" {
                    ""
                } else {
                    "The note is weighed."
                };
                entry(shows, marks(shows, "met", &[], &[]), response)
            })
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let note = "I named the damper.";
    let graded = grade_written(
        &model,
        &[written("q1", Some(note)), written("q2", Some(note))],
    )
    .await?;
    assert_eq!(item_ids(&graded), ["q1", "q2"]);
    assert!(graded
        .iter()
        .all(|grade| grade.response_to_note.as_deref() == Some("The note is weighed.")));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(shown(&sent[1]).len(), 1, "the other entry stands");
    assert!(
        sent[1]
            .prompt()
            .contains("Answer the learner's note directly in responseToNote."),
        "{}",
        sent[1].prompt()
    );
    Ok(())
}

#[tokio::test]
async fn a_response_to_a_note_is_kept_only_for_an_item_that_had_one() -> Result<()> {
    let model = answering::<GradeWritten>(|shows| {
        Some(entry(
            shows,
            marks(shows, "met", &[], &[]),
            "The note is weighed.",
        ))
    });
    let plain = grade_written(&model, &[written("q1", None)]).await?;
    assert_eq!(plain[0].response_to_note, None);
    let noted = grade_written(&model, &[written("q1", Some("I named the damper."))]).await?;
    assert_eq!(
        noted[0].response_to_note.as_deref(),
        Some("The note is weighed.")
    );
    Ok(())
}

#[tokio::test]
async fn a_cite_of_another_items_sentence_names_nothing() -> Result<()> {
    let mut brief = written("q2", None);
    brief.answer = "The kiln fires every bowl.".into();
    let model = answering::<GradeWritten>(|shows| {
        // Every item cites the second sentence of its answer; only the first item has one.
        let cited = marks(shows, "met", &["a2".to_owned()], &[]);
        Some(entry(shows, cited, ""))
    });
    let graded = grade_written(&model, &[written("q1", None), brief]).await?;
    assert_eq!(item_ids(&graded), ["q1", "q2"]);
    let words = |id: &str| {
        graded
            .iter()
            .find(|grade| grade.item_id == id)
            .map(|grade| grade.item.criteria[0].your_words.clone())
    };
    assert_eq!(
        words("q1"),
        Some(vec!["The damper is shut before the glaze sets.".to_owned()])
    );
    assert_eq!(words("q2"), Some(vec![]), "the cite names nothing here");
    assert_eq!(model.sent().len(), 1, "a lost cite costs no re-ask");
    Ok(())
}

#[tokio::test]
async fn an_item_the_grader_leaves_unanswered_alone_stays_pending() -> Result<()> {
    let sizes = Arc::new(Mutex::new(Vec::new()));
    let seen = sizes.clone();
    let model = ScriptedModel::new().on_request::<GradeWritten>(move |sent, _| {
        let items = shown(sent);
        seen.lock().unwrap().push(items.len());
        let entries: Vec<Value> = items
            .iter()
            .filter(|shows| shows.item != "i2")
            .map(|shows| entry(shows, marks(shows, "met", &[], &[]), ""))
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let graded = grade_written(&model, &[written("q1", None), written("q2", None)]).await?;
    assert_eq!(item_ids(&graded), ["q1"], "q2 stays pending");
    assert_eq!(sizes.lock().unwrap().clone(), [2, 1, 1]);
    Ok(())
}

#[tokio::test]
async fn nothing_is_graded_for_no_answers() -> Result<()> {
    let model = ScriptedModel::new();
    assert!(grade_written(&model, &[]).await?.is_empty());
    assert!(judge_short_answers(&model, &[]).await?.is_empty());
    assert!(model.sent().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_judgement_is_filled_from_the_apps_own_words() -> Result<()> {
    let model = answering::<JudgeShort>(|shows| {
        let words = [shows.sentence(0)];
        Some(judged(shows, "equivalent", &words, ""))
    });
    let graded = judge_short_answers(&model, &[short("a")]).await?;
    let [grade] = graded.as_slice() else {
        panic!("one grade expected: {graded:?}");
    };
    assert_eq!(grade.item_id, "a");
    assert_eq!(grade.response_to_note, None);
    let [criterion] = grade.item.criteria.as_slice() else {
        panic!("one criterion expected");
    };
    assert_eq!(
        criterion.criterion_id,
        crate::grade_rationale::EQUIVALENCE_CRITERION
    );
    assert_eq!(criterion.your_words, ["In 1887."]);
    assert_eq!(criterion.verdict, GraderVerdict::Met);
    assert_eq!(criterion.source_quote, None);
    Ok(())
}

#[tokio::test]
async fn a_judgement_of_not_equivalent_misses_the_equivalence_criterion() -> Result<()> {
    let model = answering::<JudgeShort>(|shows| Some(judged(shows, "not_equivalent", &[], "")));
    let graded = judge_short_answers(&model, &[short("a")]).await?;
    assert_eq!(graded[0].item.criteria[0].verdict, GraderVerdict::Missed);
    Ok(())
}

#[tokio::test]
async fn an_empty_reply_to_a_learners_note_on_a_short_answer_is_corrected() -> Result<()> {
    let model = ScriptedModel::new().on_request::<JudgeShort>(|sent, nth| {
        let response = if nth == 0 { "" } else { "The note is weighed." };
        let entries: Vec<Value> = shown(sent)
            .iter()
            .map(|shows| judged(shows, "equivalent", &[], response))
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let mut noted = short("a");
    noted.learner_note = Some("I meant the same thing.".into());
    let graded = judge_short_answers(&model, &[noted]).await?;
    assert_eq!(
        graded[0].response_to_note.as_deref(),
        Some("The note is weighed.")
    );
    assert_eq!(model.sent().len(), 2);
    Ok(())
}

/// Judges every short answer equivalent, except that a request holding more than `fits`
/// items, or the item `cut`, has its reply cut off at the output limit.
fn grader(fits: usize, cut: &'static str, sizes: Arc<Mutex<Vec<usize>>>) -> ScriptedModel {
    ScriptedModel::new().on_request::<JudgeShort>(move |sent, _| {
        let items = shown(sent);
        sizes.lock().unwrap().push(items.len());
        if items.len() > fits || items.iter().any(|shows| shows.item == cut) {
            return ended("{\"entries\":[{\"of\":", "length");
        }
        let entries: Vec<Value> = items
            .iter()
            .map(|shows| judged(shows, "equivalent", &[], ""))
            .collect();
        reply(json!({ "entries": entries }).to_string())
    })
}

#[tokio::test]
async fn a_grading_reply_cut_off_is_asked_again_in_halves_and_grades_every_item() -> Result<()> {
    let sizes = Arc::new(Mutex::new(Vec::new()));
    let model = grader(2, "", sizes.clone());
    let answers: Vec<ShortAnswer> = ["a", "b", "c", "d", "e"].map(short).to_vec();
    let graded = judge_short_answers(&model, &answers).await?;
    assert_eq!(
        item_ids(&graded),
        ["a", "b", "c", "d", "e"],
        "nothing was lost to the cut-off replies"
    );
    assert_eq!(
        sizes.lock().unwrap().clone(),
        [5, 2, 2, 1],
        "five cut off, then the rest at the smaller size"
    );
    Ok(())
}

#[tokio::test]
async fn an_item_whose_reply_is_cut_off_alone_stays_pending_and_the_rest_are_graded() -> Result<()>
{
    let model = grader(usize::MAX, "i2", Arc::new(Mutex::new(Vec::new())));
    let answers: Vec<ShortAnswer> = ["a", "b", "c"].map(short).to_vec();
    let graded = judge_short_answers(&model, &answers).await?;
    assert_eq!(item_ids(&graded), ["a", "c"], "[b] stays pending");
    Ok(())
}

#[tokio::test]
async fn an_item_the_model_declines_stays_pending_and_the_rest_are_graded() -> Result<()> {
    let sizes = Arc::new(Mutex::new(Vec::new()));
    let seen = sizes.clone();
    let model = ScriptedModel::new().on_request::<JudgeShort>(move |sent, _| {
        let items = shown(sent);
        seen.lock().unwrap().push(items.len());
        if items.iter().any(|shows| shows.item == "i2") {
            return ended("", "refusal");
        }
        let entries: Vec<Value> = items
            .iter()
            .map(|shows| judged(shows, "equivalent", &[], ""))
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let answers: Vec<ShortAnswer> = ["a", "b", "c"].map(short).to_vec();
    let graded = judge_short_answers(&model, &answers).await?;
    assert_eq!(item_ids(&graded), ["a", "c"], "[b] stays pending");
    assert_eq!(
        sizes.lock().unwrap().clone(),
        [3, 1, 1, 1],
        "the batch is declined and split; a and c are graded; b is declined alone"
    );
    Ok(())
}

#[tokio::test]
async fn a_single_grading_reply_cut_off_is_asked_again_with_twice_the_room() -> Result<()> {
    let budgets = Arc::new(Mutex::new(Vec::new()));
    let seen = budgets.clone();
    let model = ScriptedModel::new().on_request::<JudgeShort>(move |sent, _| {
        let mut budgets = seen.lock().unwrap();
        budgets.push(sent.request.max_output_tokens());
        if budgets.len() == 1 {
            return ended("{\"entries\":[{\"of\":", "length");
        }
        let entries: Vec<Value> = shown(sent)
            .iter()
            .map(|shows| judged(shows, "equivalent", &[], ""))
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let graded = judge_short_answers(&model, &[short("a")]).await?;
    assert_eq!(
        graded.len(),
        1,
        "the learner's answer is graded, not left pending"
    );
    let budgets = budgets.lock().unwrap().clone();
    let first = budgets.first().copied().flatten();
    assert!(first.is_some());
    assert_eq!(budgets.get(1).copied().flatten(), first.map(|b| b * 2));
    Ok(())
}

#[tokio::test]
async fn the_grading_schemas_ask_for_no_text_length() -> Result<()> {
    use crate::practice::practice_tests::text_length_rules;
    let schemas = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen = schemas.clone();
    let model = ScriptedModel::new()
        .on_request::<GradeWritten>({
            let seen = seen.clone();
            move |sent, _| {
                seen.lock()
                    .unwrap()
                    .extend(sent.request.schema().map(|schema| schema.as_json().clone()));
                reply("{\"entries\":[]}".to_owned())
            }
        })
        .on_request::<JudgeShort>(move |sent, _| {
            seen.lock()
                .unwrap()
                .extend(sent.request.schema().map(|schema| schema.as_json().clone()));
            reply("{\"entries\":[]}".to_owned())
        });
    grade_written(&model, &[written("q1", None)]).await?;
    grade_written(&model, &[written("q1", Some("I named the damper."))]).await?;
    judge_short_answers(&model, &[short("a")]).await?;
    let mut noted = short("b");
    noted.learner_note = Some("I meant the same thing.".into());
    judge_short_answers(&model, &[noted]).await?;
    let schemas = schemas.lock().unwrap();
    assert!(schemas.len() >= 4, "{} schemas were sent", schemas.len());
    for schema in schemas.iter() {
        let rules = text_length_rules(schema);
        assert!(rules.is_empty(), "{rules:?}");
    }
    Ok(())
}

#[test]
fn the_grading_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = GradeInput {
        items: vec![Written {
            item_id: marks.reference("question-100001-harbour".to_owned()),
            question: marks.told("Why does the damper stay shut?".to_owned()),
            rubric: vec![Criterion {
                id: marks.reference("criterion-100002-damper".to_owned()),
                title: marks.told("Names the damper".to_owned()),
                description: marks.told("Says why the damper is shut.".to_owned()),
                max_points: marks.told(2_u32),
            }],
            reference: vec![SourceSentence {
                span: marks.reference((100_003_usize, 100_050_usize)),
                text: marks.told("A kiln fires bowls once the damper is shut.".to_owned()),
            }],
            answer: vec![AnswerSentence {
                span: marks.reference((100_004_usize, 100_060_usize)),
                text: marks.told("The damper keeps the heat in the kiln.".to_owned()),
            }],
            note: Some(marks.told("I named the damper in my answer.".to_owned())),
        }],
    };
    holds::<GradeWritten>(&input, &marks);
}

#[test]
fn the_judging_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = JudgeInput {
        items: vec![Short {
            item_id: marks.reference("question-100005-harbour".to_owned()),
            question: marks.told("When did the mill close?".to_owned()),
            accepted: marks.told(vec!["The year 1887 exactly".to_owned()]),
            reference: marks.told("The mill closed in the year 1887.".to_owned()),
            answer: vec![AnswerSentence {
                span: marks.reference((100_006_usize, 100_040_usize)),
                text: marks.told("It closed in eighteen eighty-seven.".to_owned()),
            }],
            note: Some(marks.told("I meant the same year.".to_owned())),
        }],
    };
    holds::<JudgeShort>(&input, &marks);
}

#[tokio::test]
async fn prompt_snapshot_grade_written() -> Result<()> {
    let inner = answering::<GradeWritten>(|shows| {
        let words = [shows.sentence(0)];
        let source = [shows.source(0)];
        let mut criteria = marks(shows, "met", &words, &source);
        if let Some(last) = criteria.last_mut() {
            *last = json!({"of": shows.criteria.last(), "yourWords": [], "whatWasMissing": "No evidence is given.",
                "fullCreditIncludes": "A measured result.", "sourceQuote": [], "verdict": "missed"});
        }
        Some(entry(shows, criteria, "The note is weighed."))
    });
    let model = RequestLog::new(&inner);
    grade_written(
        &model,
        &[
            written("q1", None),
            written("q2", Some("I named the mechanism.")),
        ],
    )
    .await?;
    model.assert_snapshot::<GradeWritten>();
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_judge_short() -> Result<()> {
    let inner = answering::<JudgeShort>(|shows| {
        let words = [shows.sentence(0)];
        Some(judged(shows, "equivalent", &words, "The note is weighed."))
    });
    let model = RequestLog::new(&inner);
    let mut noted = short("b");
    noted.learner_note = Some("I meant the same thing.".into());
    judge_short_answers(&model, &[short("a"), noted]).await?;
    model.assert_snapshot::<JudgeShort>();
    Ok(())
}
