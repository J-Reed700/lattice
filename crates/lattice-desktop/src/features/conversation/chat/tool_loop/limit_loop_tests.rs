//! Rounds cut short by the turn's deadline or by a prompt the window refuses,
//! run through the whole loop: the turn answers from what it gathered
//! instead of failing with it.

use super::super::final_round::{OUT_OF_TIME_INSTRUCTION, OUT_OF_TIME_RESULT};
use super::super::room::SHED_RESULT;
use super::*;

/// A turn with a deadline this close.
fn turn(time_budget: Duration) -> Turn<'static> {
    Turn {
        message: "When did the bridge open?",
        sources: Vec::new(),
        time_budget,
    }
}

fn on_web(web: &Arc<Web>) -> FakeRuntime {
    FakeRuntime {
        tools: Some(web.clone()),
        ..FakeRuntime::default()
    }
}

/// A page read cut off by the deadline is answered as not run, and the turn
/// answers from what the others found.
#[tokio::test]
async fn a_turn_out_of_time_in_its_reads_answers_from_what_it_found() {
    let model = Model::new([
        reads(&[PAGE_A, PAGE_B]),
        CompletionResponse::from_text("It opened in 1932 [1]."),
    ]);
    let web = Web::with_delays(&[(PAGE_B, 60_000)]);
    let run = run_turn(
        on_web(&web),
        &model,
        &[tool("fetch_url_content")],
        ROOMY,
        5,
        "out-of-time-reads",
        turn(Duration::from_millis(300)),
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932 [1].");
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    let answer = &requests[1];
    assert!(answer.no_time_limit, "the answering round runs past it");
    assert_eq!(answer.time_budget, None);
    assert_eq!(answer.tool_choice, Some(ToolChoice::None));
    assert!(
        last_text(answer).ends_with(OUT_OF_TIME_INSTRUCTION),
        "{}",
        last_text(answer)
    );
    assert_eq!(result_ids(answer).len(), 2, "every call has its answer");
    assert_eq!(results_mentioning(answer, OUT_OF_TIME_RESULT).len(), 1);
    assert_eq!(
        results_mentioning(answer, &format!("Text of {PAGE_A}")).len(),
        1
    );
}

/// A model still writing when the deadline passes is asked once more, for
/// the answer, with no deadline: Stop and the output limit end it.
#[tokio::test]
async fn a_round_still_writing_at_the_deadline_is_asked_for_the_answer() {
    let model = Model::new([
        Scripted::After(
            Duration::from_secs(60),
            CompletionResponse::from_text("never finished"),
        ),
        Scripted::Reply(CompletionResponse::from_text("It opened in 1932.")),
    ]);
    let web = Web::with_delays(&[]);
    let run = run_turn(
        on_web(&web),
        &model,
        &[tool("web_search")],
        ROOMY,
        5,
        "out-of-time-writing",
        turn(Duration::from_millis(300)),
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932.");
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    assert!(!requests[0].no_time_limit);
    assert!(requests[1].no_time_limit);
    assert_eq!(last_text(&requests[1]), OUT_OF_TIME_INSTRUCTION);
}

/// A prompt the window refuses sheds its newest result, not the turn: the
/// round is asked again as the answering one, with the older result kept.
#[tokio::test]
async fn a_refused_prompt_sheds_its_newest_result_and_answers() {
    let model = Model::new([
        Scripted::Reply(reads(&[PAGE_A, PAGE_B])),
        Scripted::Refuse(ROOMY + 1_000),
        Scripted::Reply(CompletionResponse::from_text("It opened in 1932 [1].")),
    ]);
    let web = Web::with_pages_of(2_000);
    let run = run_turn(
        on_web(&web),
        &model,
        &[tool("fetch_url_content")],
        ROOMY,
        5,
        "refused-prompt",
        turn(TURN_TIME),
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932 [1].");
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    let answer = &requests[2];
    assert_eq!(results_mentioning(answer, SHED_RESULT).len(), 1);
    assert_eq!(
        results_mentioning(answer, &format!("Text of {PAGE_A}")).len(),
        1
    );
    assert!(results_mentioning(answer, &format!("Text of {PAGE_B}")).is_empty());
    assert!(
        last_text(answer).ends_with(WINDOW_FULL_INSTRUCTION),
        "{}",
        last_text(answer)
    );
}

/// The answering round past the deadline has no deadline of its own, and
/// Stop still ends it.
#[tokio::test]
async fn stop_ends_the_answering_round_past_the_deadline() {
    let request_id = "stopped-past-the-deadline";
    let web = Arc::new(Web {
        hang: true,
        ..Web::default()
    });
    let runtime = on_web(&web);
    let turns = Arc::clone(&runtime.turn_cancellation);
    assert!(turns.begin_turn_within(request_id, CONVERSATION, None));
    // The read hangs past the deadline; the answer it asks for never comes.
    let model = Model::new([reads(&[PAGE_A])]);
    let tools = [tool("fetch_url_content")];
    let stop = async {
        while model.requests().len() < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(turns.request_cancel(CONVERSATION, Some(request_id)));
    };
    let (run, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            run_turn(
                runtime,
                &model,
                &tools,
                ROOMY,
                5,
                request_id,
                turn(Duration::from_millis(100)),
            ),
            stop
        )
    })
    .await
    .expect("stop ends the answering round");
    turns.finish_turn(request_id);

    let error = run.outcome.expect_err("a stopped turn has no answer");
    assert!(cancellation::is_cancellation(&error), "{error}");
    assert_eq!(model.requests().len(), 2, "one answering round, no more");
}
