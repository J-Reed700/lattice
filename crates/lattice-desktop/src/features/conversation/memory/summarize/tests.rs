use super::*;
use lattice_core::domain::conversation::memory::SourceRole;

fn message(sequence: i64, content: &str) -> SourceMessage {
    SourceMessage {
        id: format!("m{sequence}"),
        conversation_id: "c".to_string(),
        sequence,
        role: SourceRole::User,
        content: content.to_string(),
        content_digest: String::new(),
        status: "completed".to_string(),
        pictures: Vec::new(),
        references: Vec::new(),
        replayed: None,
    }
}

/// The summary model reads a question's attached pictures as caption
/// lines after its text.
#[test]
fn a_question_is_folded_with_its_attached_pictures_named() {
    let mut asked = message(1, "Which bird?");
    asked.pictures = vec!["heron.jpg".to_string()];
    let passages = [asked];
    let steps = plan_fold(&passages, 2_000);
    assert_eq!(
        steps[0][0].text,
        "Which bird?\n[Picture attached: heron.jpg]"
    );
}

/// A long message is folded in parts, and every character arrives.
#[test]
fn a_message_longer_than_a_step_is_folded_in_marked_parts_with_nothing_lost() {
    let long: String = (0..3_000).map(|index| format!("w{index} ")).collect();
    let passages = vec![message(1, "short"), message(2, &long)];
    let steps = plan_fold(&passages, 2_000);
    assert!(steps.len() > 1, "{} steps", steps.len());
    let pieces: Vec<&FoldPiece<'_>> = steps.iter().flatten().collect();
    assert_eq!(pieces[0].text, "short");
    let parts: Vec<&FoldPiece<'_>> = pieces.iter().copied().filter(|p| p.sequence == 2).collect();
    let total = parts.len();
    let mut rebuilt = String::new();
    for (index, part) in parts.iter().enumerate() {
        let mark = format!("[part {} of {total}] ", index + 1);
        assert!(part.text.starts_with(&mark), "{}", &part.text[..30]);
        assert!(part.text.chars().count() <= 2_000);
        rebuilt.push_str(&part.text[mark.len()..]);
    }
    assert_eq!(rebuilt, long);
    for step in &steps {
        let chars: usize = step.iter().map(|piece| piece.text.chars().count()).sum();
        assert!(chars <= 2_000, "{chars}");
    }
}

/// A short exchange is folded into a short summary, not one the size of
/// the whole pool; a long one grows toward the pool and stops there.
#[test]
fn the_target_follows_what_is_folded_and_the_pool_is_its_ceiling() {
    let pool = 4_900;
    assert_eq!(summary_target(0, 120, pool), MIN_FOLD_TOKENS);
    assert_eq!(summary_target(600, 2_000, pool), 1_100);
    assert_eq!(summary_target(3_000, 20_000, pool), pool);
    assert_eq!(
        summary_target(0, 120, 100),
        100,
        "a tiny pool still bounds it"
    );
}

struct Window;

#[async_trait::async_trait]
impl LLMPort for Window {
    async fn complete(
        &self,
        _request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> lattice_core::shared::error::Result<
        lattice_core::application::ports::llm_port::CompletionResponse,
    > {
        unreachable!("sizing makes no call")
    }
    fn model_name(&self) -> &str {
        "window"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

/// The answer room and the running summary in the prompt are two
/// different things, each counted once: the summary at its target.
#[test]
fn a_fold_step_counts_the_answer_room_once() {
    let target = 1_000;
    let template = fold_task(&FoldInput::of(None, &[], &[], &[], target, false), None);
    let fixed = Window.count_tokens(&editor_system("previous_summary"))
        + Window.count_tokens(&template)
        + target;
    let room_tokens = 32_768 - summary_output_tokens(target) - 32_768 / 20 - fixed;
    assert_eq!(
        fold_room_chars(&Window, &[], &[], target),
        chars_within_tokens(room_tokens, Window.chars_per_token())
    );
}

/// Passages that fit one step are folded in one call, unmarked.
#[test]
fn passages_that_fit_are_one_step_as_written() {
    let passages = vec![message(1, "first"), message(2, "second")];
    let steps = plan_fold(&passages, 10_000);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0][0].text, "first");
    assert_eq!(steps[0][1].text, "second");
}

/// The summariser cannot see pictures: it reads each as its caption, and
/// the `img-N` ref, which means nothing outside its turn, never reaches it.
#[test]
fn a_picture_is_folded_as_its_caption() {
    let passages = vec![message(
        1,
        "It flew in 1969.\n\n![Saturn V lifting off](img-1)",
    )];
    let steps = plan_fold(&passages, 2_000);
    let text = &steps[0][0].text;
    assert!(text.contains("[Picture: Saturn V lifting off]"), "{text}");
    assert!(!text.contains("img-1"), "{text}");
}

use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

/// The request's user message: the payload the model reads.
fn payload_of(sent: &Sent) -> serde_json::Value {
    let text = sent
        .request
        .input()
        .iter()
        .filter_map(|input| match input {
            CompletionInput::Message { role, content } if role == "user" => Some(content.clone()),
            _ => None,
        })
        .next_back()
        .expect("a user message");
    serde_json::from_str(&text).expect("a JSON payload")
}

/// The handles of the rows a payload shows under `key`, in order.
fn handles_under(payload: &serde_json::Value, key: &str) -> Vec<String> {
    payload[key]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter_map(|row| row.strip_prefix('[')?.split_once(']'))
        .map(|(handle, _)| handle.to_owned())
        .collect()
}

type Replied =
    futures::future::BoxFuture<'static, lattice_core::shared::error::Result<CompletionResponse>>;

fn reply(text: String) -> Replied {
    Box::pin(async move { Ok(CompletionResponse::from_text(text)) })
}

fn edit(
    replace: serde_json::Value,
    insert: serde_json::Value,
    remove: serde_json::Value,
) -> String {
    serde_json::json!({
        "insert": insert,
        "move": [],
        "remove": remove,
        "replace": replace,
    })
    .to_string()
}

fn judge_by(restating: &'static str) -> impl Fn(&Sent, usize) -> Replied {
    move |sent: &Sent, _| {
        let payload = payload_of(sent);
        let entries: Vec<serde_json::Value> = payload["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .filter_map(|row| row.as_str()?.strip_prefix('[')?.split_once("] "))
            .map(|(handle, text)| {
                serde_json::json!({
                    "of": handle,
                    "verdict": if text.contains(restating) { "restates" } else { "clear" },
                })
            })
            .collect();
        reply(serde_json::json!({ "entries": entries }).to_string())
    }
}

const SUMMARY: &str = "Harbour opens at six.\nKiln fires on Tuesdays.\nTide tables are posted.";

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

async fn fold(
    model: &ScriptedModel,
    previous: Option<&str>,
    forgotten: &[&str],
    budget: usize,
) -> std::result::Result<FittedSummary, CompactionError> {
    let forgotten: Vec<String> = forgotten.iter().map(|label| (*label).to_owned()).collect();
    fold_summary(
        model,
        &Deadline::new(None, None),
        previous,
        &[message(7, "The kiln moves to Thursdays.")],
        &[],
        &forgotten,
        budget,
        &words,
    )
    .await
}

/// A line no edit names is the bytes it was.
#[tokio::test]
async fn an_edit_changes_only_the_lines_it_names() {
    let model = ScriptedModel::new().on_request::<MemoryFold>(|sent, _| {
        let handles = handles_under(&payload_of(sent), "previous_summary");
        assert_eq!(handles, ["u1", "u2", "u3"]);
        reply(edit(
            serde_json::json!([{ "at": "u2", "with": "Kiln fires on Thursdays." }]),
            serde_json::json!([]),
            serde_json::json!([]),
        ))
    });

    let fitted = fold(&model, Some(SUMMARY), &[], 1_000).await.unwrap();

    assert_eq!(
        fitted.text,
        "Harbour opens at six.\nKiln fires on Thursdays.\nTide tables are posted."
    );
    assert_eq!(model.count::<MemoryFold>(), 1);
    let payload = payload_of(&model.sent()[0]);
    assert_eq!(
        payload["previous_summary"],
        "[u1] Harbour opens at six.\n[u2] Kiln fires on Tuesdays.\n[u3] Tide tables are posted."
    );
    assert!(payload.get("forgotten_by_user").is_none());
}

#[tokio::test]
async fn the_first_fold_writes_its_lines_after_the_start() {
    let model = ScriptedModel::new().on_request::<MemoryFold>(|sent, _| {
        let payload = payload_of(sent);
        assert!(payload["previous_summary"].is_null());
        reply(edit(
            serde_json::json!([]),
            serde_json::json!([{ "after": "start", "with": "Kiln fires on Thursdays." }]),
            serde_json::json!([]),
        ))
    });

    let fitted = fold(&model, None, &[], 1_000).await.unwrap();

    assert_eq!(fitted.text, "Kiln fires on Thursdays.");
    assert!(!fitted.regenerated);
}

/// An edit that cannot be read leaves the summary as it was: the outcome
/// an empty answer had.
#[tokio::test]
async fn an_edit_that_cannot_be_read_leaves_the_summary_as_it_was() {
    let model =
        ScriptedModel::new().on_request::<MemoryFold>(|_, _| reply("not an edit".to_owned()));

    let fitted = fold(&model, Some(SUMMARY), &[], 1_000).await.unwrap();

    assert_eq!(fitted.text, SUMMARY);
}

#[tokio::test]
async fn a_line_that_restates_a_forgotten_fact_is_left_out_and_only_new_lines_are_checked() {
    let model = ScriptedModel::new()
        .on_request::<MemoryFold>(|_, _| {
            reply(edit(
                serde_json::json!([]),
                serde_json::json!([
                    { "after": "u3", "with": "The locker code was 4471." },
                    { "after": "u3", "with": "The harbour closes at nine." }
                ]),
                serde_json::json!([]),
            ))
        })
        .on_request::<MemoryForgetting>(judge_by("4471"));

    let fitted = fold(&model, Some(SUMMARY), &["The locker code is 4471"], 1_000)
        .await
        .unwrap();

    assert!(!fitted.text.contains("4471"), "{}", fitted.text);
    assert!(fitted.text.starts_with(SUMMARY), "{}", fitted.text);
    assert!(fitted.text.contains("The harbour closes at nine."));
    let checked = payload_of(&model.sent()[1]);
    let shown = checked["lines"].to_string();
    assert!(shown.contains("The locker code was 4471."));
    assert!(
        !shown.contains("Kiln fires"),
        "an earlier line was judged again"
    );
    assert_eq!(
        checked["forgotten_by_user"],
        serde_json::json!(["The locker code is 4471"])
    );
}

/// A line whose check stays unanswered is not kept.
#[tokio::test]
async fn a_line_whose_check_is_unanswered_is_left_out() {
    let model = ScriptedModel::new()
        .on_request::<MemoryFold>(|_, _| {
            reply(edit(
                serde_json::json!([]),
                serde_json::json!([{ "after": "u3", "with": "The harbour closes at nine." }]),
                serde_json::json!([]),
            ))
        })
        .on_request::<MemoryForgetting>(|_, _| reply("no verdict".to_owned()));

    let fitted = fold(&model, Some(SUMMARY), &["The locker code is 4471"], 1_000)
        .await
        .unwrap();

    assert_eq!(fitted.text, SUMMARY);
}

#[tokio::test]
async fn the_forgotten_facts_are_still_told_and_nothing_is_checked_without_them() {
    let model = ScriptedModel::new().on_request::<MemoryFold>(|_, _| {
        reply(edit(
            serde_json::json!([]),
            serde_json::json!([{ "after": "u3", "with": "The harbour closes at nine." }]),
            serde_json::json!([]),
        ))
    });
    fold(&model, Some(SUMMARY), &[], 1_000).await.unwrap();
    assert_eq!(model.count::<MemoryForgetting>(), 0);

    let model = ScriptedModel::new()
        .on_request::<MemoryFold>(|_, _| {
            reply(edit(
                serde_json::json!([]),
                serde_json::json!([]),
                serde_json::json!([]),
            ))
        })
        .on_request::<MemoryForgetting>(judge_by("4471"));
    fold(&model, Some(SUMMARY), &["The locker code is 4471"], 1_000)
        .await
        .unwrap();
    let told = payload_of(&model.sent()[0]);
    assert_eq!(
        told["forgotten_by_user"],
        serde_json::json!(["The locker code is 4471"])
    );
    assert!(told["forgotten_instructions"]
        .as_str()
        .is_some_and(|text| text.contains("Leave each of them out of the summary")));
    assert_eq!(
        model.count::<MemoryForgetting>(),
        0,
        "an edit that wrote no line has nothing to check"
    );
}

/// An oversized candidate is cut down by an edit of the lines it shows.
#[tokio::test]
async fn a_summary_over_its_budget_is_shrunk_by_an_edit_of_its_lines() {
    let model = ScriptedModel::new().on_request::<MemoryShrink>(|sent, _| {
        let payload = payload_of(sent);
        assert_eq!(payload["task"], "shrink_working_summary");
        assert_eq!(handles_under(&payload, "summary"), ["u1", "u2", "u3"]);
        reply(edit(
            serde_json::json!([]),
            serde_json::json!([]),
            serde_json::json!(["u2"]),
        ))
    });
    let passages: [SourceMessage; 0] = [];

    let fitted = fold_summary(
        &model,
        &Deadline::new(None, None),
        Some(SUMMARY),
        &passages,
        &[],
        &[],
        8,
        &words,
    )
    .await
    .unwrap();

    assert!(fitted.regenerated);
    assert_eq!(
        fitted.text,
        "Harbour opens at six.\nTide tables are posted."
    );
    assert_eq!(fitted.tokens, 8);
}

#[tokio::test]
async fn a_shrink_that_cannot_be_read_leaves_the_summary_too_large() {
    let model =
        ScriptedModel::new().on_request::<MemoryShrink>(|_, _| reply("not an edit".to_owned()));
    let passages: [SourceMessage; 0] = [];

    let result = fold_summary(
        &model,
        &Deadline::new(None, None),
        Some(SUMMARY),
        &passages,
        &[],
        &[],
        8,
        &words,
    )
    .await;

    assert!(matches!(
        result,
        Err(CompactionError::SummaryTooLarge { .. })
    ));
}

#[test]
fn the_fold_request_holds_no_identity_of_the_app() {
    let mut marks = Marks::new();
    let input = FoldInput {
        summary: Some(marks.told("Harbour opens at six.".to_owned())),
        passages: vec![FoldPassage {
            role: Told("user".to_owned()),
            sequence: Told(7),
            text: marks.told("The kiln moves to Thursdays.".to_owned()),
        }],
        requirements: vec![marks.told("Keep answers about the harbour".to_owned())],
        forgotten: vec![marks.told("The locker code is 4471".to_owned())],
        budget_tokens: Told(512),
        more_passages_follow: Told(false),
        output_tokens: marks.internal(7_340_219_usize),
    };
    holds::<MemoryFold>(&input, &marks);
}

#[test]
fn the_shrink_request_holds_no_identity_of_the_app() {
    let mut marks = Marks::new();
    let input = ShrinkInput {
        summary: marks.told("Harbour opens at six.".to_owned()),
        measured_tokens: Told(900),
        budget_tokens: Told(512),
        output_tokens: marks.internal(7_340_219_usize),
    };
    holds::<MemoryShrink>(&input, &marks);
}

#[test]
fn the_forgetting_check_holds_no_identity_of_the_app() {
    let mut marks = Marks::new();
    let input = ForgetInput {
        forgotten: vec![marks.told("The locker code is 4471".to_owned())],
        lines: vec![Line {
            position: marks.reference(7_340_219_usize),
            text: marks.told("The locker code was 4471.".to_owned()),
        }],
    };
    holds::<MemoryForgetting>(&input, &marks);
}

/// `text_without` leaves a line out with the break it owned.
#[test]
fn a_line_left_out_takes_its_own_break() {
    let units = Units::of("one\ntwo\nthree", Grain::Line);
    assert_eq!(units.text_without(&[1]), "one\nthree");
    assert_eq!(units.text_without(&[0, 2]), "two");
    assert_eq!(units.text_without(&[]), "one\ntwo\nthree");
}

/// Memory payloads go to the model as compact JSON with sorted keys:
/// indentation would spend window tokens on whitespace in every call.
#[test]
fn payloads_are_compact_json_with_sorted_keys() {
    let shrink = to_payload(&ShrinkTask {
        budget_tokens: &Told(80),
        instructions: "Shorten it.",
        measured_tokens: &Told(120),
        summary: Some("[u1] Zellteilung: Mitose läuft in vier Phasen ab."),
        task: "shrink_working_summary",
    });
    assert!(!shrink.contains('\n'), "{shrink}");
    assert!(
        shrink.starts_with("{\"budget_tokens\":80,\"instructions\":"),
        "{shrink}"
    );

    let fold = fold_task(
        &FoldInput::of(Some("one"), &[], &[], &["a fact".to_owned()], 80, true),
        Some("[u1] one"),
    );
    let payload: serde_json::Value = serde_json::from_str(&fold).unwrap();
    let keys: Vec<&String> = payload.as_object().unwrap().keys().collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "{fold}");
}
