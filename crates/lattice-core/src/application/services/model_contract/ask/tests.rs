use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::*;
use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, CompletionResponse};
use crate::application::services::model_contract::shown::Drawn;
use crate::application::services::model_contract::ContractId;

const STOP: &str = "stop";
const LENGTH: &str = "length";
const REFUSAL: &str = "refusal";

const BAD_REASON: &str = "Bad is not a colour.";
const BAD_TOLD: &str = "Bad is not a colour. Answer again.";

#[derive(Debug, Clone, PartialEq)]
struct Tile(u32);

impl Kind for Tile {
    const PREFIX: &'static str = "t";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = u32;

    fn reference(&self) -> u32 {
        self.0
    }
}

fn tiles(n: u32) -> Vec<Tile> {
    (1..=n).map(Tile).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Input {
    open: Vec<u32>,
    settled: Vec<(u32, String)>,
    fills: bool,
    pad: usize,
}

impl Input {
    fn of(open: &[u32]) -> Self {
        Self {
            open: open.to_vec(),
            settled: Vec::new(),
            fills: false,
            pad: 0,
        }
    }
}

fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
    let things: Vec<Tile> = input.open.iter().map(|n| Tile(*n)).collect();
    let shown = showing.show(&things)?;
    let list = shown
        .iter()
        .map(|(handle, tile)| {
            format!(
                "{} is tile {}{}",
                handle.bracketed(),
                tile.0,
                "x".repeat(input.pad)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let facts = input
        .settled
        .iter()
        .map(|(n, word)| format!("tile {n} is {word}"))
        .collect::<Vec<_>>()
        .join("; ");
    let mut says = Says::new(
        "You judge tiles.",
        format!("Judge these tiles.\n{list}\nSettled so far: {facts}"),
    );
    says.output_tokens = 100;
    says.output_fills_window = input.fills;
    Ok(says)
}

fn refuse(statement: &str) -> std::result::Result<(), Refusal> {
    if statement == "bad" {
        Err(Refusal(BAD_REASON.into()))
    } else {
        Ok(())
    }
}

fn schema_of(pointers: Pointers) -> Option<Schema> {
    let word = match pointers {
        Pointers::Enum => "enum",
        Pointers::Plain => "plain",
    };
    Some(Schema::made(serde_json::json!({ "pointers": word })))
}

/// One word per tile, written `t1=red` on a line of its own.
struct Verdicts;

impl Shape for Verdicts {
    type Statement = String;

    fn schema(_showing: &Showing<'_>, pointers: Pointers) -> Option<Schema> {
        schema_of(pointers)
    }
}

impl PerItem for Verdicts {
    type Item = Tile;

    fn read(written: &Written<'_>, showing: &Showing<'_>) -> Vec<(Picked<Tile>, Settled<String>)> {
        let lines: Vec<(&str, &str)> = written
            .text
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        showing
            .shown::<Tile>()
            .into_iter()
            .map(|picked| {
                let name = picked.handle.written();
                let words: Vec<&str> = lines
                    .iter()
                    .filter(|(handle, _)| *handle == name)
                    .map(|(_, word)| *word)
                    .collect();
                let settled = match words.as_slice() {
                    [word] => Settled::Answered((*word).to_owned()),
                    [first, rest @ ..] if rest.iter().all(|word| word == first) => {
                        Settled::Answered((*first).to_owned())
                    }
                    [_, ..] => Settled::Unanswered(Why::Contradicted(format!(
                        "You gave {name} two different words."
                    ))),
                    [] => match written.ended {
                        Ended::Complete if lines.is_empty() => Settled::Unanswered(Why::NotRead),
                        Ended::Complete => Settled::Unanswered(Why::Missing),
                        Ended::CutOff => Settled::Unanswered(Why::CutOff),
                        Ended::Declined => Settled::Unanswered(Why::Declined),
                    },
                };
                (picked, settled)
            })
            .collect()
    }
}

/// The whole text is the one word.
struct Word;

impl Shape for Word {
    type Statement = String;

    fn schema(_showing: &Showing<'_>, pointers: Pointers) -> Option<Schema> {
        schema_of(pointers)
    }
}

impl Single for Word {
    fn read(written: &Written<'_>, _showing: &Showing<'_>) -> Settled<String> {
        let text = written.text.trim();
        match written.ended {
            Ended::Complete if text.is_empty() => Settled::Unanswered(Why::NotRead),
            Ended::Complete => Settled::Answered(text.to_owned()),
            Ended::CutOff => Settled::Unanswered(Why::CutOff),
            Ended::Declined => Settled::Unanswered(Why::Declined),
        }
    }
}

/// New words, written `+ red` on a line of their own.
struct Names;

impl Shape for Names {
    type Statement = String;

    fn schema(_showing: &Showing<'_>, pointers: Pointers) -> Option<Schema> {
        schema_of(pointers)
    }
}

impl Listed for Names {
    fn read(written: &Written<'_>, _showing: &Showing<'_>) -> Found<String> {
        let items: Vec<String> = written
            .text
            .lines()
            .filter_map(|line| line.strip_prefix("+ "))
            .map(str::to_owned)
            .collect();
        let left_out = written
            .text
            .lines()
            .filter(|line| line.starts_with("- "))
            .count();
        let rest = match written.ended {
            Ended::Complete if items.is_empty() && !written.text.trim().is_empty() => {
                Some(Why::NotRead)
            }
            Ended::Complete => None,
            Ended::CutOff => Some(Why::CutOff),
            Ended::Declined => Some(Why::Declined),
        };
        Found {
            items,
            rest,
            left_out,
        }
    }
}

macro_rules! contract {
    ($name:ident, $id:literal, $shape:ty, $alone:expr) => {
        struct $name;

        impl Contract for $name {
            const ID: ContractId = ContractId::new($id);
            type Input = Input;
        }

        impl Answered for $name {
            type Answer = $shape;
            const ALONE: Alone = $alone;

            fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
                says(input, showing)
            }

            fn accept(_input: &Input, statement: &String) -> std::result::Result<(), Refusal> {
                refuse(statement)
            }
        }
    };
}

contract!(Each, "test-each", Verdicts, Alone::Pending);
contract!(EachFails, "test-each-fails", Verdicts, Alone::Fails);
contract!(One, "test-one", Word, Alone::Pending);
contract!(OneFails, "test-one-fails", Word, Alone::Fails);
contract!(New, "test-new", Names, Alone::Pending);
contract!(NewFails, "test-new-fails", Names, Alone::Fails);

type Responder = Box<dyn Fn(&str) -> (String, &'static str) + Send + Sync>;

/// A model that returns queued replies, then the responder's, then the fallback, and records every request.
struct Script {
    window: usize,
    replies: Mutex<VecDeque<(String, &'static str)>>,
    responder: Option<Responder>,
    fallback: Option<(String, &'static str)>,
    seen: Mutex<Vec<CompletionRequest>>,
    enforcing: bool,
    fail_at: Option<usize>,
}

impl Script {
    fn new(replies: &[(&str, &'static str)]) -> Self {
        Self {
            window: 32_768,
            replies: Mutex::new(
                replies
                    .iter()
                    .map(|(text, finish)| ((*text).to_owned(), *finish))
                    .collect(),
            ),
            responder: None,
            fallback: None,
            seen: Mutex::new(Vec::new()),
            enforcing: false,
            fail_at: None,
        }
    }

    fn always(mut self, text: &str, finish: &'static str) -> Self {
        self.fallback = Some((text.to_owned(), finish));
        self
    }

    fn responding(
        mut self,
        responder: impl Fn(&str) -> (String, &'static str) + Send + Sync + 'static,
    ) -> Self {
        self.responder = Some(Box::new(responder));
        self
    }

    fn enforcing(mut self) -> Self {
        self.enforcing = true;
        self
    }

    fn failing_at(mut self, send: usize) -> Self {
        self.fail_at = Some(send);
        self
    }

    fn window(mut self, tokens: usize) -> Self {
        self.window = tokens;
        self
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.seen.lock().unwrap().clone()
    }

    fn sent(&self) -> usize {
        self.requests().len()
    }

    fn prompt(&self, index: usize) -> String {
        prompt_of(&self.requests()[index])
    }
}

fn prompt_of(request: &CompletionRequest) -> String {
    request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[async_trait]
impl LLMPort for Script {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let number = {
            let mut seen = self.seen.lock().unwrap();
            seen.push(request.clone());
            seen.len()
        };
        if self.fail_at == Some(number) {
            return Err(AppError::Network("the model is unreachable".into()));
        }
        let queued = self.replies.lock().unwrap().pop_front();
        let (text, finish) = if let Some(reply) = queued {
            reply
        } else if let Some(responder) = &self.responder {
            responder(&prompt_of(request))
        } else if let Some((text, finish)) = &self.fallback {
            (text.clone(), *finish)
        } else {
            return Err(AppError::InternalError("the script ran out".into()));
        };
        Ok(CompletionResponse {
            finish_reason: finish.into(),
            ..CompletionResponse::from_text(text)
        })
    }

    fn model_name(&self) -> &str {
        "script"
    }

    fn max_context_tokens(&self) -> usize {
        self.window
    }

    fn enforces(&self, _schema: &Schema) -> bool {
        self.enforcing
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A transport that drafts first and then answers from the draft: two
/// requests per send, one per follow-up.
struct Drafting;

#[async_trait]
impl Transport for Drafting {
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        let draft = grounded_generation::generate(llm, request.draft_step("Think first.")).await?;
        let answer = request.answer_step(
            "Now answer.",
            "Answer from your draft.",
            "Think first.",
            &draft.text,
        );
        Plain.send(llm, answer).await
    }

    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent> {
        Plain.follow(llm, step).await
    }
}

fn rows(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(handle, word)| format!("{handle}={word}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Answers `ok` for every tile the prompt lists.
fn answer_all(prompt: &str) -> (String, &'static str) {
    let text = prompt
        .lines()
        .filter_map(|line| line.strip_prefix('['))
        .filter_map(|line| line.split_once(']'))
        .filter(|(_, tail)| tail.starts_with(" is tile"))
        .map(|(handle, _)| format!("{handle}=ok"))
        .collect::<Vec<_>>()
        .join("\n");
    (text, STOP)
}

fn listed(prompt: &str) -> usize {
    prompt.matches(" is tile ").count()
}

type Calls = Mutex<Vec<(Vec<u32>, Vec<(u32, String)>)>>;

fn answered(words: &[&str]) -> Vec<Settled<String>> {
    words
        .iter()
        .map(|word| Settled::Answered((*word).to_owned()))
        .collect()
}

macro_rules! each_runner {
    ($name:ident, $contract:ty) => {
        async fn $name(
            script: &Script,
            things: &[Tile],
            calls: &Calls,
            pad: usize,
            facts: bool,
        ) -> Result<Vec<Settled<String>>> {
            let mut scope = Scope::for_question();
            ask_each::<$contract>(
                script,
                &Plain,
                &mut scope,
                things,
                |open: &[&Tile], entries: &[(Picked<Tile>, String)]| {
                    let settled: Vec<(u32, String)> = entries
                        .iter()
                        .map(|(picked, word)| (picked.reference, word.clone()))
                        .collect();
                    let open: Vec<u32> = open.iter().map(|tile| tile.0).collect();
                    calls.lock().unwrap().push((open.clone(), settled.clone()));
                    Input {
                        open,
                        settled: if facts { settled } else { Vec::new() },
                        fills: false,
                        pad,
                    }
                },
                &CallOptions::default(),
            )
            .await
        }
    };
}

each_runner!(run_each, Each);
each_runner!(run_each_fails, EachFails);

macro_rules! one_runner {
    ($name:ident, $contract:ty) => {
        async fn $name(script: &Script, input: &Input) -> Result<Settled<String>> {
            let mut scope = Scope::for_question();
            ask::<$contract>(script, &Plain, &mut scope, input, &CallOptions::default()).await
        }
    };
}

one_runner!(run_one, One);
one_runner!(run_one_fails, OneFails);

macro_rules! new_runner {
    ($name:ident, $contract:ty) => {
        async fn $name(script: &Script, limit: usize) -> Result<Found<String>> {
            let mut scope = Scope::for_question();
            let asking_after = |standing: &[String]| Input {
                open: Vec::new(),
                settled: standing
                    .iter()
                    .enumerate()
                    .map(|(index, word)| (u32::try_from(index).unwrap_or(0), word.clone()))
                    .collect(),
                fills: false,
                pad: 0,
            };
            ask_new::<$contract>(
                script,
                &Plain,
                &mut scope,
                asking_after(&[]),
                |standing: &[String]| (standing.len() < limit).then(|| asking_after(standing)),
                &CallOptions::default(),
            )
            .await
        }
    };
}

new_runner!(run_new, New);
new_runner!(run_new_fails, NewFails);

fn no_calls() -> Calls {
    Mutex::new(Vec::new())
}

// One subject.

#[tokio::test]
async fn an_answered_single_settles_in_one_tagged_send() {
    let script = Script::new(&[("ok", STOP)]);
    let settled = run_one(&script, &Input::of(&[])).await.unwrap();
    assert_eq!(settled, Settled::Answered("ok".to_owned()));
    assert_eq!(script.sent(), 1);
    let request = &script.requests()[0];
    assert_eq!(request.contract().map(|call| call.id), Some(One::ID));
}

#[tokio::test]
async fn a_refusal_gets_one_correction_quoting_the_reason() {
    let script = Script::new(&[("bad", STOP), ("good", STOP)]);
    let settled = run_one(&script, &Input::of(&[])).await.unwrap();
    assert_eq!(settled, Settled::Answered("good".to_owned()));
    assert_eq!(script.sent(), 2);
    let correction = script.prompt(1);
    assert!(correction.contains(BAD_TOLD), "{correction}");
    assert!(
        correction.contains("bad"),
        "the refused answer is in the history"
    );
}

#[tokio::test]
async fn a_second_refusal_settles_unanswered_with_the_last_reason() {
    let script = Script::new(&[("bad", STOP), ("bad", STOP)]);
    let settled = run_one(&script, &Input::of(&[])).await.unwrap();
    assert_eq!(
        settled,
        Settled::Unanswered(Why::Refused(BAD_REASON.to_owned()))
    );
    assert_eq!(script.sent(), 2);
}

#[tokio::test]
async fn an_unreadable_answer_is_corrected_once() {
    let script = Script::new(&[("", STOP), ("ok", STOP)]);
    let settled = run_one(&script, &Input::of(&[])).await.unwrap();
    assert_eq!(settled, Settled::Answered("ok".to_owned()));
    assert!(script.prompt(1).contains(NOT_READ));
}

#[tokio::test]
async fn a_cut_off_answer_with_room_gets_exactly_one_larger_send() {
    let script = Script::new(&[("", LENGTH), ("fine", STOP)]);
    let settled = run_one(&script, &Input::of(&[])).await.unwrap();
    assert_eq!(settled, Settled::Answered("fine".to_owned()));
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    let first = requests[0].max_output_tokens().unwrap();
    let second = requests[1].max_output_tokens().unwrap();
    assert!(second > first, "{second} should exceed {first}");
    assert!(
        !script.prompt(1).contains(CUT_OFF),
        "a larger send is not a correction"
    );
}

#[tokio::test]
async fn an_answer_cut_off_again_gets_one_correction_and_no_more() {
    let script = Script::new(&[]).always("", LENGTH);
    let settled = run_one(&script, &Input::of(&[])).await.unwrap();
    assert_eq!(settled, Settled::Unanswered(Why::CutOff));
    assert_eq!(script.sent(), 3);
    assert!(script.prompt(2).contains(CUT_OFF));
}

#[tokio::test]
async fn a_cut_off_answer_that_fills_the_window_gets_no_larger_send() {
    let script = Script::new(&[("", LENGTH), ("fine", STOP)]);
    let input = Input {
        fills: true,
        ..Input::of(&[])
    };
    let settled = run_one(&script, &input).await.unwrap();
    assert_eq!(settled, Settled::Answered("fine".to_owned()));
    assert_eq!(script.sent(), 2);
    assert!(
        script.prompt(1).contains(CUT_OFF),
        "the second send is the correction"
    );
}

#[tokio::test]
async fn a_declined_answer_gets_no_correction() {
    let script = Script::new(&[("", REFUSAL), ("never asked", STOP)]);
    let settled = run_one(&script, &Input::of(&[])).await.unwrap();
    assert_eq!(settled, Settled::Unanswered(Why::Declined));
    assert_eq!(script.sent(), 1);
}

#[tokio::test]
async fn the_schema_is_the_enum_form_only_where_the_backend_enforces_it() {
    let enforcing = Script::new(&[("ok", STOP)]).enforcing();
    run_one(&enforcing, &Input::of(&[])).await.unwrap();
    let plain = Script::new(&[("ok", STOP)]);
    run_one(&plain, &Input::of(&[])).await.unwrap();
    let pointers = |script: &Script| {
        script.requests()[0]
            .schema()
            .map(|schema| schema.as_json()["pointers"].clone())
    };
    assert_eq!(pointers(&enforcing), Some(serde_json::json!("enum")));
    assert_eq!(pointers(&plain), Some(serde_json::json!("plain")));
}

#[tokio::test]
async fn a_correction_continues_the_answer_step_of_a_transport_that_drafts_first() {
    let script = Script::new(&[("my thinking", STOP), ("bad", STOP), ("good", STOP)]);
    let mut scope = Scope::for_question();
    let settled = ask::<One>(
        &script,
        &Drafting,
        &mut scope,
        &Input::of(&[]),
        &CallOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(settled, Settled::Answered("good".to_owned()));
    let requests = script.requests();
    assert_eq!(requests.len(), 3, "no second draft");
    assert!(requests[0].schema().is_none(), "a draft step has no schema");
    assert!(requests[1].schema().is_some());
    assert!(script.prompt(1).contains("my thinking"));
    assert!(requests[2].schema().is_some());
    let correction = script.prompt(2);
    let draft = correction.find("my thinking").expect("the draft");
    let refused = correction.find("bad").expect("the refused answer");
    let told = correction.find(BAD_TOLD).expect("the reason");
    assert!(draft < refused && refused < told, "{correction}");
}

#[tokio::test]
async fn a_larger_send_repeats_the_answer_step_of_a_transport_that_drafts_first() {
    let script = Script::new(&[("my thinking", STOP), ("", LENGTH), ("good", STOP)]);
    let mut scope = Scope::for_question();
    let settled = ask::<One>(
        &script,
        &Drafting,
        &mut scope,
        &Input::of(&[]),
        &CallOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(settled, Settled::Answered("good".to_owned()));
    let requests = script.requests();
    assert_eq!(requests.len(), 3, "no second draft");
    assert_eq!(script.prompt(2), script.prompt(1));
    let first = requests[1].max_output_tokens().unwrap();
    let second = requests[2].max_output_tokens().unwrap();
    assert!(second > first, "{second} should exceed {first}");
}

// A set of items.

#[tokio::test]
async fn an_empty_set_sends_nothing() {
    let script = Script::new(&[]);
    let settled = run_each(&script, &[], &no_calls(), 0, true).await.unwrap();
    assert!(settled.is_empty());
    assert_eq!(script.sent(), 0);
}

#[tokio::test]
async fn every_item_answered_in_one_send() {
    let script = Script::new(&[(
        &*rows(&[("t1", "red"), ("t2", "blue"), ("t3", "green")]),
        STOP,
    )]);
    let calls = no_calls();
    let settled = run_each(&script, &tiles(3), &calls, 0, true).await.unwrap();
    assert_eq!(settled, answered(&["red", "blue", "green"]));
    assert_eq!(script.sent(), 1);
    assert_eq!(listed(&script.prompt(0)), 3);
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_cut_off_reply_keeps_what_closed_and_halves_the_rest() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a"), ("t2", "b")]), LENGTH),
        (
            &*rows(&[("t3", "c"), ("t4", "d"), ("t5", "e"), ("t6", "f")]),
            STOP,
        ),
        (&*rows(&[("t7", "g"), ("t8", "h")]), STOP),
    ]);
    let calls = no_calls();
    let settled = run_each(&script, &tiles(8), &calls, 0, true).await.unwrap();
    assert_eq!(settled, answered(&["a", "b", "c", "d", "e", "f", "g", "h"]));
    assert_eq!(script.sent(), 3);
    assert_eq!(listed(&script.prompt(1)), 4, "six left, in batches of four");
    assert_eq!(listed(&script.prompt(2)), 2);
    let first_two = vec![(1, "a".to_owned()), (2, "b".to_owned())];
    let first_six: Vec<(u32, String)> = first_two
        .iter()
        .cloned()
        .chain(
            ["c", "d", "e", "f"]
                .iter()
                .enumerate()
                .map(|(index, word)| (u32::try_from(index).unwrap() + 3, (*word).to_owned())),
        )
        .collect();
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            ((1..=8).collect::<Vec<u32>>(), Vec::new()),
            (vec![3, 4, 5, 6], first_two),
            (vec![7, 8], first_six),
        ],
        "input_for is given exactly the entries settled so far"
    );
}

#[tokio::test]
async fn a_complete_reply_missing_two_items_asks_for_them_as_one_batch() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a"), ("t2", "b"), ("t3", "c")]), STOP),
        (&*rows(&[("t4", "d"), ("t5", "e")]), STOP),
    ]);
    let calls = no_calls();
    let settled = run_each(&script, &tiles(5), &calls, 0, true).await.unwrap();
    assert_eq!(settled, answered(&["a", "b", "c", "d", "e"]));
    assert_eq!(script.sent(), 2);
    assert_eq!(listed(&script.prompt(1)), 2);
    assert_eq!(calls.lock().unwrap()[1].0, vec![4, 5]);
}

#[tokio::test]
async fn an_item_with_differing_entries_is_asked_alone() {
    let script = Script::new(&[
        (
            &*rows(&[("t1", "a"), ("t2", "b"), ("t2", "c"), ("t3", "d")]),
            STOP,
        ),
        (&*rows(&[("t2", "b")]), STOP),
    ]);
    let calls = no_calls();
    let settled = run_each(&script, &tiles(3), &calls, 0, true).await.unwrap();
    assert_eq!(settled, answered(&["a", "b", "d"]));
    assert_eq!(script.sent(), 2);
    assert_eq!(listed(&script.prompt(1)), 1);
    assert_eq!(calls.lock().unwrap()[1].0, vec![2]);
}

#[tokio::test]
async fn an_item_still_contradicted_alone_is_corrected_once_with_what_contradicts() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a"), ("t2", "b"), ("t2", "c")]), STOP),
        (&*rows(&[("t2", "b"), ("t2", "c")]), STOP),
        (&*rows(&[("t2", "b")]), STOP),
    ]);
    let settled = run_each(&script, &tiles(2), &no_calls(), 0, true)
        .await
        .unwrap();
    assert_eq!(settled, answered(&["a", "b"]));
    assert_eq!(script.sent(), 3);
    assert!(script
        .prompt(2)
        .contains("You gave t2 two different words. Answer again and say one thing."));
}

#[tokio::test]
async fn a_missing_item_alone_is_told_its_handle() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a")]), STOP),
        (&*rows(&[("t9", "z")]), STOP),
        (&*rows(&[("t2", "b")]), STOP),
    ]);
    let settled = run_each(&script, &tiles(2), &no_calls(), 0, true)
        .await
        .unwrap();
    assert_eq!(settled, answered(&["a", "b"]));
    assert!(script
        .prompt(2)
        .contains("Your answer said nothing about t2. Answer for it."));
}

#[tokio::test]
async fn a_refused_entry_is_corrected_alone_while_the_others_stand() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a"), ("t2", "bad"), ("t3", "c")]), STOP),
        (&*rows(&[("t2", "fixed")]), STOP),
    ]);
    let calls = no_calls();
    let settled = run_each(&script, &tiles(3), &calls, 0, true).await.unwrap();
    assert_eq!(settled, answered(&["a", "fixed", "c"]));
    assert_eq!(script.sent(), 2);
    let alone = script.prompt(1);
    assert!(alone.contains(BAD_TOLD), "{alone}");
    assert_eq!(listed(&alone), 1);
    assert_eq!(
        calls.lock().unwrap()[1],
        (vec![2], vec![(1, "a".to_owned()), (3, "c".to_owned())])
    );
}

struct EachByItem;

impl Contract for EachByItem {
    const ID: ContractId = ContractId::new("test-each-by-item");
    type Input = Input;
}

impl Answered for EachByItem {
    type Answer = Verdicts;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says(input, showing)
    }

    fn accept_for(
        _input: &Input,
        of: &Picked<Tile>,
        statement: &String,
    ) -> std::result::Result<(), Refusal> {
        if of.reference == 2 && statement == "x" {
            Err(Refusal(BAD_REASON.into()))
        } else {
            Ok(())
        }
    }
}

each_runner!(run_each_by_item, EachByItem);

#[tokio::test]
async fn a_rule_that_depends_on_the_item_refuses_that_entry_only() {
    let script = Script::new(&[
        (&*rows(&[("t1", "x"), ("t2", "x"), ("t3", "x")]), STOP),
        (&*rows(&[("t2", "y")]), STOP),
    ]);
    let settled = run_each_by_item(&script, &tiles(3), &no_calls(), 0, true)
        .await
        .unwrap();
    assert_eq!(settled, answered(&["x", "y", "x"]));
    assert_eq!(script.sent(), 2);
    let alone = script.prompt(1);
    assert!(alone.contains(BAD_TOLD), "{alone}");
    assert_eq!(listed(&alone), 1);
}

#[tokio::test]
async fn a_refused_entry_refused_again_settles_unanswered_without_a_third_send() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a"), ("t2", "bad")]), STOP),
        (&*rows(&[("t2", "bad")]), STOP),
    ]);
    let settled = run_each(&script, &tiles(2), &no_calls(), 0, true)
        .await
        .unwrap();
    assert_eq!(
        settled,
        vec![
            Settled::Answered("a".to_owned()),
            Settled::Unanswered(Why::Refused(BAD_REASON.to_owned()))
        ]
    );
    assert_eq!(script.sent(), 2);
}

#[tokio::test]
async fn an_item_keeps_its_handle_and_a_settled_handle_resolves_to_nothing_later() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a")]), STOP),
        (&*rows(&[("t1", "zzz"), ("t2", "b"), ("t3", "c")]), STOP),
    ]);
    let settled = run_each(&script, &tiles(3), &no_calls(), 0, true)
        .await
        .unwrap();
    assert_eq!(settled, answered(&["a", "b", "c"]));
    let second = script.prompt(1);
    assert!(second.contains("[t2] is tile 2"), "{second}");
    assert!(second.contains("[t3] is tile 3"), "{second}");
    assert!(!second.contains("[t1]"), "{second}");
}

#[tokio::test]
async fn a_batch_that_does_not_fit_is_split_before_any_send() {
    let pad = 800;
    let window = window_holding(4, pad).await;
    let script = Script::new(&[]).window(window).responding(answer_all);
    let settled = run_each(&script, &tiles(8), &no_calls(), pad, false)
        .await
        .unwrap();
    assert_eq!(settled, answered(&["ok"; 8]));
    let counts: Vec<usize> = (0..script.sent())
        .map(|i| listed(&script.prompt(i)))
        .collect();
    assert_eq!(counts, vec![4, 4]);
}

#[tokio::test]
async fn a_single_item_that_cannot_fit_is_a_fault_of_sending() {
    let script = Script::new(&[]).window(300).responding(answer_all);
    let result = run_each(&script, &tiles(1), &no_calls(), 4_000, false).await;
    assert!(result.is_err());
    assert_eq!(script.sent(), 0);
}

/// The smallest window (in steps of 50 tokens) in which `n` tiles padded by `pad` fit.
async fn window_holding(n: u32, pad: usize) -> usize {
    let things: Vec<u32> = (1..=n).collect();
    for window in (1_000..400_000).step_by(50) {
        let script = Script::new(&[]).window(window);
        let mut scope = Scope::for_question();
        let input = Input {
            pad,
            ..Input::of(&things)
        };
        let (_, request) =
            render::<Each>(&script, &mut scope, &input, &CallOptions::default()).unwrap();
        if fits(&script, &request).await {
            return window;
        }
    }
    panic!("no window holds {n} tiles");
}

#[tokio::test]
async fn a_backend_that_never_answers_ends_within_four_n_minus_one_sends() {
    for (text, finish) in [("", STOP), ("", LENGTH), ("???", STOP)] {
        for n in [1_u32, 2, 5, 8] {
            let script = Script::new(&[]).always(text, finish);
            let settled = run_each(&script, &tiles(n), &no_calls(), 0, true)
                .await
                .unwrap();
            assert_eq!(settled.len(), n as usize);
            assert!(
                settled
                    .iter()
                    .all(|settled| matches!(settled, Settled::Unanswered(_))),
                "{settled:?}"
            );
            let bound = 4 * n as usize - 1;
            assert!(
                script.sent() <= bound,
                "{n} items took {} sends, the bound is {bound}",
                script.sent()
            );
        }
    }
}

#[tokio::test]
async fn a_send_fault_is_an_error_and_never_a_settlement() {
    let script = Script::new(&[("ok", STOP)]).failing_at(1);
    assert!(run_one(&script, &Input::of(&[])).await.is_err());
    let script = Script::new(&[]).failing_at(1);
    assert!(run_each(&script, &tiles(3), &no_calls(), 0, true)
        .await
        .is_err());
    let script = Script::new(&[(&*rows(&[("t1", "a")]), STOP)]).failing_at(2);
    assert!(run_each(&script, &tiles(3), &no_calls(), 0, true)
        .await
        .is_err());
    let script = Script::new(&[("+ a", LENGTH)]).failing_at(2);
    assert!(run_new(&script, 10).await.is_err());
}

#[tokio::test]
async fn nothing_about_content_is_an_error_when_unanswered_items_stay_pending() {
    for (text, finish) in [("", REFUSAL), ("", LENGTH), ("bad", STOP), ("", STOP)] {
        let script = Script::new(&[]).always(text, finish);
        assert!(run_one(&script, &Input::of(&[])).await.is_ok());
        let script = Script::new(&[]).always(text, finish);
        assert!(run_each(&script, &tiles(3), &no_calls(), 0, true)
            .await
            .is_ok());
        let script = Script::new(&[]).always(text, finish);
        assert!(run_new(&script, 10).await.is_ok());
    }
}

// Alone::Fails.

#[tokio::test]
async fn a_failing_contract_returns_the_error_of_its_reason() {
    let cut_off = Script::new(&[]).always("", LENGTH);
    let error = run_one_fails(&cut_off, &Input::of(&[])).await.unwrap_err();
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error:?}");

    let declined = Script::new(&[]).always("", REFUSAL);
    let error = run_one_fails(&declined, &Input::of(&[])).await.unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );

    let refused = Script::new(&[]).always("bad", STOP);
    let error = run_one_fails(&refused, &Input::of(&[])).await.unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(text) if text == BAD_REASON),
        "{error:?}"
    );

    let unanswered = Script::new(&[]).always("", LENGTH);
    let error = run_each_fails(&unanswered, &tiles(3), &no_calls(), 0, true)
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error:?}");

    let ok = Script::new(&[("ok", STOP)]);
    assert!(run_one_fails(&ok, &Input::of(&[])).await.is_ok());
}

#[test]
fn failed_names_its_reason_and_picks_the_variant_callers_retry_on() {
    assert!(matches!(failed(&Why::CutOff), AppError::AnswerCutOff(_)));
    for why in [
        Why::NotRead,
        Why::Missing,
        Why::Unplaced,
        Why::Declined,
        Why::Contradicted("It contradicts.".into()),
        Why::Refused("It is refused.".into()),
    ] {
        assert!(
            matches!(failed(&why), AppError::ModelAnswerUnusable(_)),
            "{why:?}"
        );
    }
    assert!(matches!(
        failed(&Why::Refused("It is refused.".into())),
        AppError::ModelAnswerUnusable(text) if text == "It is refused."
    ));
    assert!(matches!(
        failed(&Why::Contradicted("It contradicts.".into())),
        AppError::ModelAnswerUnusable(text) if text == "It contradicts."
    ));
}

// New things.

#[tokio::test]
async fn items_of_a_cut_off_reply_stand_and_the_next_request_is_built_from_them() {
    let script = Script::new(&[("+ a\n+ b", LENGTH), ("+ c", STOP)]);
    let found = run_new(&script, 10).await.unwrap();
    assert_eq!(
        found,
        Found {
            items: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            rest: None,
            left_out: 0
        }
    );
    assert_eq!(script.sent(), 2);
    assert!(script.prompt(1).contains("tile 0 is a; tile 1 is b"));
}

#[tokio::test]
async fn rest_is_none_only_after_a_reply_read_to_its_end() {
    let script = Script::new(&[("+ a", LENGTH), ("+ b", LENGTH)]);
    let found = run_new(&script, 2).await.unwrap();
    assert_eq!(found.items, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(found.rest, Some(Why::CutOff));

    let script = Script::new(&[("+ a", STOP)]);
    let found = run_new(&script, 10).await.unwrap();
    assert_eq!(found.rest, None);
    assert_eq!(script.sent(), 1);
}

#[tokio::test]
async fn no_input_stops_the_asking() {
    let script = Script::new(&[("+ a", LENGTH), ("+ never asked", STOP)]);
    let found = run_new(&script, 1).await.unwrap();
    assert_eq!(
        found,
        Found {
            items: vec!["a".to_owned()],
            rest: Some(Why::CutOff),
            left_out: 0
        }
    );
    assert_eq!(script.sent(), 1);
}

#[tokio::test]
async fn a_refused_item_is_left_out() {
    let script = Script::new(&[("+ a\n+ bad\n+ c", STOP)]);
    let found = run_new(&script, 10).await.unwrap();
    assert_eq!(found.items, vec!["a".to_owned(), "c".to_owned()]);
    assert_eq!(found.rest, None);
    assert_eq!(found.left_out, 1);
}

#[tokio::test]
async fn an_unread_item_is_counted_and_a_clean_answer_counts_none() {
    let script = Script::new(&[(
        "+ a
- x
+ b",
        STOP,
    )]);
    let found = run_new(&script, 10).await.unwrap();
    assert_eq!(found.items, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(found.left_out, 1);

    let script = Script::new(&[("+ a", STOP)]);
    assert_eq!(run_new(&script, 10).await.unwrap().left_out, 0);
}

#[tokio::test]
async fn an_item_left_in_a_cut_off_tail_is_not_counted() {
    let script = Script::new(&[(
        "+ a
+ b", LENGTH,
    )]);
    let found = run_new(&script, 1).await.unwrap();
    assert_eq!(found.rest, Some(Why::CutOff));
    assert_eq!(found.left_out, 0);
}

#[tokio::test]
async fn a_cut_off_reply_that_adds_nothing_gets_one_larger_send() {
    let script = Script::new(&[("", LENGTH), ("+ a", STOP)]);
    let found = run_new(&script, 10).await.unwrap();
    assert_eq!(found.items, vec!["a".to_owned()]);
    assert_eq!(found.rest, None);
    assert_eq!(script.sent(), 2);
    let requests = script.requests();
    assert!(requests[1].max_output_tokens() > requests[0].max_output_tokens());
}

#[tokio::test]
async fn nothing_read_and_nothing_standing_gets_one_correction() {
    let script = Script::new(&[("???", STOP), ("+ a", STOP)]);
    let found = run_new(&script, 10).await.unwrap();
    assert_eq!(found.items, vec!["a".to_owned()]);
    assert!(script.prompt(1).contains(NOT_READ));

    let script = Script::new(&[]).always("???", STOP);
    let found = run_new(&script, 10).await.unwrap();
    assert_eq!(
        found,
        Found {
            items: Vec::new(),
            rest: Some(Why::NotRead),
            left_out: 0
        }
    );
    assert_eq!(script.sent(), 2);
}

#[tokio::test]
async fn a_failing_contract_with_no_item_and_an_unread_answer_is_an_error() {
    let script = Script::new(&[]).always("???", STOP);
    let error = run_new_fails(&script, 10).await.unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );

    let script = Script::new(&[]).always("", LENGTH);
    let error = run_new_fails(&script, 10).await.unwrap_err();
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error:?}");

    let script = Script::new(&[("+ a", LENGTH)]);
    let found = run_new_fails(&script, 1).await.unwrap();
    assert_eq!(found.items, vec!["a".to_owned()]);
}

// The transport's notice of an unanswered request.

/// Plain sending that records the text of each request it is told was left unanswered.
struct Noting {
    told: Mutex<Vec<String>>,
}

impl Noting {
    fn new() -> Self {
        Self {
            told: Mutex::new(Vec::new()),
        }
    }

    fn told(&self) -> Vec<String> {
        self.told.lock().unwrap().clone()
    }
}

/// What a request says: its history, then its task.
fn text_of(request: &GroundedRequest) -> String {
    request
        .history()
        .iter()
        .map(|item| item.content.as_str())
        .chain(std::iter::once(request.task()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[async_trait]
impl Transport for Noting {
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        Plain.send(llm, request).await
    }

    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent> {
        Plain.follow(llm, step).await
    }

    async fn unanswered(&self, _llm: &dyn LLMPort, request: &GroundedRequest) -> Result<()> {
        self.told.lock().unwrap().push(text_of(request));
        Ok(())
    }
}

async fn each_noted(script: &Script, noting: &Noting, n: u32) -> Result<Vec<Settled<String>>> {
    let mut scope = Scope::for_question();
    ask_each::<Each>(
        script,
        noting,
        &mut scope,
        &tiles(n),
        |open: &[&Tile], _entries: &[(Picked<Tile>, String)]| {
            Input::of(&open.iter().map(|tile| tile.0).collect::<Vec<u32>>())
        },
        &CallOptions::default(),
    )
    .await
}

#[tokio::test]
async fn an_answered_request_is_not_reported_unanswered() {
    let script = Script::new(&[("ok", STOP)]);
    let noting = Noting::new();
    let mut scope = Scope::for_question();
    let settled = ask::<One>(
        &script,
        &noting,
        &mut scope,
        &Input::of(&[]),
        &CallOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(settled, Settled::Answered("ok".to_owned()));
    assert!(noting.told().is_empty());
}

#[tokio::test]
async fn a_request_left_unanswered_is_reported_once_as_first_sent() {
    let script = Script::new(&[]).always("", STOP);
    let noting = Noting::new();
    let mut scope = Scope::for_question();
    let settled = ask::<One>(
        &script,
        &noting,
        &mut scope,
        &Input::of(&[]),
        &CallOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(settled, Settled::Unanswered(Why::NotRead));
    assert_eq!(script.sent(), 2, "the request and its one correction");
    assert!(script.prompt(1).contains(NOT_READ));
    let told = noting.told();
    assert_eq!(told.len(), 1);
    assert!(told[0].contains("Judge these tiles."), "{}", told[0]);
    assert!(!told[0].contains(NOT_READ), "{}", told[0]);
}

#[tokio::test]
async fn a_request_that_fails_alone_is_reported_before_it_fails() {
    let script = Script::new(&[]).always("", STOP);
    let noting = Noting::new();
    let mut scope = Scope::for_question();
    let result = ask::<OneFails>(
        &script,
        &noting,
        &mut scope,
        &Input::of(&[]),
        &CallOptions::default(),
    )
    .await;
    assert!(result.is_err());
    let told = noting.told();
    assert_eq!(told.len(), 1);
    assert!(!told[0].contains(NOT_READ), "{}", told[0]);
}

#[tokio::test]
async fn a_batch_answered_in_part_is_reported() {
    let script = Script::new(&[
        (&*rows(&[("t1", "a"), ("t2", "b")]), STOP),
        (&*rows(&[("t3", "c")]), STOP),
    ]);
    let noting = Noting::new();
    let settled = each_noted(&script, &noting, 3).await.unwrap();
    assert_eq!(settled, answered(&["a", "b", "c"]));
    let told = noting.told();
    assert_eq!(told.len(), 1, "the batch, not the item asked alone");
    assert!(told[0].contains("[t1] is tile 1"), "{}", told[0]);
    assert!(told[0].contains("[t3] is tile 3"), "{}", told[0]);
}

#[tokio::test]
async fn a_batch_answered_whole_is_not_reported() {
    let script = Script::new(&[(&*rows(&[("t1", "a"), ("t2", "b"), ("t3", "c")]), STOP)]);
    let noting = Noting::new();
    let settled = each_noted(&script, &noting, 3).await.unwrap();
    assert_eq!(settled, answered(&["a", "b", "c"]));
    assert!(noting.told().is_empty());
}

#[tokio::test]
async fn an_item_left_unanswered_alone_is_reported() {
    let script = Script::new(&[(&*rows(&[("t1", "a"), ("t2", "b")]), STOP)]).always("", STOP);
    let noting = Noting::new();
    let settled = each_noted(&script, &noting, 3).await.unwrap();
    assert_eq!(
        settled,
        vec![
            Settled::Answered("a".to_owned()),
            Settled::Answered("b".to_owned()),
            Settled::Unanswered(Why::NotRead)
        ]
    );
    let told = noting.told();
    assert_eq!(told.len(), 2, "the batch, then the item alone");
    assert!(told[0].contains("[t1] is tile 1"), "{}", told[0]);
    assert!(told[1].contains("[t3] is tile 3"), "{}", told[1]);
    assert!(!told[1].contains("[t1]"), "{}", told[1]);
}

#[tokio::test]
async fn new_things_cut_off_are_reported() {
    let script = Script::new(&[("+ a", LENGTH), ("+ b", STOP)]);
    let noting = Noting::new();
    let mut scope = Scope::for_question();
    let asking_after = |standing: &[String]| Input {
        settled: standing
            .iter()
            .enumerate()
            .map(|(index, word)| (u32::try_from(index).unwrap_or(0), word.clone()))
            .collect(),
        ..Input::of(&[])
    };
    let found = ask_new::<New>(
        &script,
        &noting,
        &mut scope,
        asking_after(&[]),
        |standing: &[String]| Some(asking_after(standing)),
        &CallOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(found.items, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(found.rest, None);
    let told = noting.told();
    assert_eq!(told.len(), 1, "only the cut-off round");
    assert!(!told[0].contains("tile 0 is a"), "{}", told[0]);
}

mod fitting {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::application::services::model_contract::packed::Packed;
    use crate::application::services::model_contract::shown::Told;
    use crate::application::services::model_contract::test_support::samples::{
        Finding, FitEachInput, FitInput, SampleFit, SampleFitEach, SamplePick, Tool,
    };
    use crate::application::services::model_contract::test_support::taint::Marks;
    use crate::application::services::model_contract::test_support::ScriptedModel;

    const WINDOW: usize = 16_384;
    const QUESTION: &str = "When is slack water at the harbour mouth?";
    const CITES_P3: &str = r#"{"text":"Slack water is at noon.","pages":["p3"]}"#;
    const CITES_P1_AND_P5: &str = r#"{"text":"Slack water is at noon.","pages":["p1","p5"]}"#;
    const CITES_P1: &str = r#"{"text":"Slack water is at noon.","pages":["p1"]}"#;
    const CITES_NONE: &str = r#"{"text":"No page was held.","pages":[]}"#;

    /// A model with a window of its own that counts exactly at `exact_percent` of the estimate, or cannot count.
    struct Fitting {
        inner: ScriptedModel,
        window: usize,
        exact_percent: Option<usize>,
        exact_calls: AtomicUsize,
    }

    impl Fitting {
        fn new<C: Contract>(window: usize, exact_percent: Option<usize>, reply: &str) -> Self {
            let reply = reply.to_owned();
            let inner = ScriptedModel::new().on_request::<C>(move |_, _| {
                let text = reply.clone();
                Box::pin(async move { Ok(CompletionResponse::from_text(text)) })
            });
            Self {
                inner,
                window,
                exact_percent,
                exact_calls: AtomicUsize::new(0),
            }
        }

        fn prompts(&self) -> Vec<String> {
            self.inner
                .sent()
                .iter()
                .map(|sent| prompt_of(&sent.request))
                .collect()
        }

        fn exact_calls(&self) -> usize {
            self.exact_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LLMPort for Fitting {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            self.inner.complete(request).await
        }

        fn model_name(&self) -> &str {
            self.inner.model_name()
        }

        fn max_context_tokens(&self) -> usize {
            self.window
        }

        fn counts_tokens_exactly(&self) -> bool {
            self.exact_percent.is_some()
        }

        async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
            self.exact_calls.fetch_add(1, Ordering::SeqCst);
            let estimate = self.inner.count_tokens(text);
            Ok((estimate * self.exact_percent.unwrap_or(100)).div_ceil(100))
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    /// The text of a page that the model estimates at `tokens`.
    fn page_text(model: &Fitting, number: usize, tokens: usize) -> String {
        let mut text = format!("Page {number} of the tide table:");
        while model.count_tokens(&text) < tokens {
            text.push_str(" tide");
        }
        text
    }

    /// One page for each of `tokens`, written at that size and costed at it.
    fn pages_of(model: &Fitting, tokens: &[usize]) -> Vec<(String, usize)> {
        tokens
            .iter()
            .enumerate()
            .map(|(at, tokens)| (page_text(model, at + 1, *tokens), *tokens))
            .collect()
    }

    /// The tokens the request leaves for pages once it holds none.
    fn space_of(model: &Fitting) -> usize {
        let skeleton = FitInput::of(QUESTION, &[]);
        room::<SampleFit>(model, &skeleton, &CallOptions::default()).unwrap()
    }

    /// The numbers, of the first `of` handles, that `prompt` shows.
    fn handles_in(prompt: &str, of: usize) -> Vec<usize> {
        (1..=of)
            .filter(|n| prompt.contains(&format!("[p{n}]")))
            .collect()
    }

    async fn ask_pages(model: &Fitting, input: &FitInput) -> Settled<Finding> {
        let mut scope = Scope::for_question();
        ask::<SampleFit>(model, &Plain, &mut scope, input, &CallOptions::default())
            .await
            .unwrap()
    }

    fn cited(settled: Settled<Finding>) -> Vec<usize> {
        let Settled::Answered(finding) = settled else {
            panic!("the answer did not settle: {settled:?}");
        };
        finding
            .pages
            .iter()
            .map(|picked| picked.reference)
            .collect()
    }

    #[tokio::test]
    async fn material_that_fits_is_all_shown() {
        let model = Fitting::new::<SampleFit>(WINDOW, None, CITES_P3);
        let space = space_of(&model);
        assert!(space > 1_000, "{space}");
        let pages = pages_of(&model, &[space / 5; 3]);
        let settled = ask_pages(&model, &FitInput::of(QUESTION, &pages)).await;
        let prompts = model.prompts();
        assert_eq!(prompts.len(), 1);
        assert_eq!(handles_in(&prompts[0], 3), [1, 2, 3]);
        assert_eq!(cited(settled), [3]);
    }

    #[tokio::test]
    async fn material_beyond_the_room_is_left_out() {
        let model = Fitting::new::<SampleFit>(WINDOW, None, CITES_P1_AND_P5);
        let space = space_of(&model);
        let pages = pages_of(&model, &[space * 3 / 10; 5]);
        let settled = ask_pages(&model, &FitInput::of(QUESTION, &pages)).await;
        let prompts = model.prompts();
        assert_eq!(prompts.len(), 1);
        assert_eq!(handles_in(&prompts[0], 5), [1, 2, 3]);
        assert_eq!(cited(settled), [1]);
    }

    #[tokio::test]
    async fn a_backend_that_counts_more_than_the_estimate_is_given_less() {
        let estimating = Fitting::new::<SampleFit>(WINDOW, None, CITES_P1);
        let counting = Fitting::new::<SampleFit>(WINDOW, Some(150), CITES_P1);
        let space = space_of(&estimating);
        let pages = pages_of(&estimating, &[space / 6; 5]);
        let input = FitInput::of(QUESTION, &pages);
        ask_pages(&estimating, &input).await;
        ask_pages(&counting, &input).await;
        assert_eq!(handles_in(&estimating.prompts()[0], 5), [1, 2, 3, 4, 5]);
        assert_eq!(estimating.exact_calls(), 0);
        let shown = handles_in(&counting.prompts()[0], 5);
        let opening: Vec<usize> = (1..=shown.len()).collect();
        assert_eq!(shown, opening);
        assert!(shown.len() < 5, "{shown:?}");
        assert!(counting.exact_calls() > 0);
    }

    #[tokio::test]
    async fn no_room_asks_without_material() {
        let model = Fitting::new::<SampleFit>(WINDOW, None, CITES_NONE);
        let space = space_of(&model);
        let pages = pages_of(&model, &[10, 10, 10]);
        let costs = [space + 1, 10, 10];
        let costed: Vec<(String, usize)> = pages
            .into_iter()
            .zip(costs)
            .map(|((text, _), cost)| (text, cost))
            .collect();
        let settled = ask_pages(&model, &FitInput::of(QUESTION, &costed)).await;
        let prompts = model.prompts();
        assert_eq!(prompts.len(), 1);
        assert!(handles_in(&prompts[0], 3).is_empty());
        assert!(cited(settled).is_empty());
    }

    #[tokio::test]
    async fn a_contract_without_material_is_not_measured() {
        let model = Fitting::new::<SamplePick>(WINDOW, Some(200), r#"{"pick":"t1"}"#);
        assert!(model.counts_tokens_exactly());
        let input = SamplePick::fixture(&mut Marks::new());
        let mut scope = Scope::for_question();
        let settled =
            ask::<SamplePick>(&model, &Plain, &mut scope, &input, &CallOptions::default())
                .await
                .unwrap();
        assert!(matches!(settled, Settled::Answered(Some(_))), "{settled:?}");
        assert_eq!(model.prompts().len(), 1);
        assert_eq!(model.exact_calls(), 0);
    }

    #[tokio::test]
    async fn each_batch_is_cut_to_the_room() {
        let reply = r#"{"entries":[{"of":"t1","mood":"calm","why":"dry"},{"of":"t2","mood":"tense","why":"damp"}]}"#;
        let model = Fitting::new::<SampleFitEach>(WINDOW, None, reply);
        let tools: Vec<Tool> = ["brass compass", "rope coil"]
            .iter()
            .enumerate()
            .map(|(at, name)| Tool {
                key: format!("tool-{at}"),
                name: Told((*name).to_owned()),
                slots: Vec::new(),
            })
            .collect();
        let skeleton = FitEachInput {
            tools: tools.clone(),
            pages: Packed::default(),
        };
        let space = room::<SampleFitEach>(&model, &skeleton, &CallOptions::default()).unwrap();
        let pages = FitInput::of(QUESTION, &pages_of(&model, &[space * 3 / 10; 5])).pages;
        let mut scope = Scope::for_question();
        let settled = ask_each::<SampleFitEach>(
            &model,
            &Plain,
            &mut scope,
            &tools,
            |open: &[&Tool], _: &[Entry<SampleFitEach>]| FitEachInput {
                tools: open.iter().map(|tool| (*tool).clone()).collect(),
                pages: pages.clone(),
            },
            &CallOptions::default(),
        )
        .await
        .unwrap();
        let prompts = model.prompts();
        assert_eq!(prompts.len(), 1);
        let shown: Vec<usize> = (1..=5)
            .filter(|n| prompts[0].contains(&format!("Page {n} of the tide table")))
            .collect();
        assert_eq!(shown, [1, 2, 3]);
        let whys: Vec<String> = settled
            .into_iter()
            .map(|one| match one {
                Settled::Answered(verdict) => verdict.why,
                Settled::Unanswered(why) => panic!("unanswered: {why:?}"),
            })
            .collect();
        assert_eq!(whys, ["dry", "damp"]);
    }

    #[test]
    fn room_is_what_the_skeleton_leaves() {
        let call = CallOptions::default();
        let skeleton = FitInput::of(QUESTION, &[]);
        let room_in = |window: usize| {
            let model = Fitting::new::<SampleFit>(window, None, "");
            room::<SampleFit>(&model, &skeleton, &call).unwrap()
        };
        let normal = room_in(WINDOW);
        let smaller = room_in(WINDOW / 4);
        assert!(smaller > 0, "{smaller}");
        assert!(normal > smaller, "{normal} {smaller}");
        assert_eq!(room_in(40), 0);
    }
}

mod metering {
    use super::*;

    /// A model that reports 7 input and 3 output tokens for every reply.
    struct Counting {
        replies: Mutex<VecDeque<&'static str>>,
    }

    impl Counting {
        fn new(replies: &[&'static str]) -> Self {
            Self {
                replies: Mutex::new(replies.iter().copied().collect()),
            }
        }
    }

    #[async_trait]
    impl LLMPort for Counting {
        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            let text = self.replies.lock().unwrap().pop_front().unwrap_or("red");
            Ok(CompletionResponse {
                input_tokens: 7,
                output_tokens: 3,
                finish_reason: STOP.into(),
                ..CompletionResponse::from_text(text)
            })
        }

        fn model_name(&self) -> &str {
            "counting"
        }

        fn max_context_tokens(&self) -> usize {
            32_768
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn a_plain_send_carries_the_tokens_the_model_reported() {
        let llm = Counting::new(&["red"]);
        let request = GroundedRequest::of("You judge tiles.".into(), "Say a word.".into());
        let sent = Plain.send(&llm, request).await.unwrap();
        assert_eq!(
            sent.usage,
            Usage {
                input: 7,
                output: 3
            }
        );
    }

    #[tokio::test]
    async fn a_metered_transport_adds_up_every_send_and_follow() {
        let llm = Counting::new(&["bad", "red"]);
        let plain = Plain;
        let metered = Metered::new(&plain);
        assert_eq!(metered.used(), Usage::default());
        let settled = ask::<One>(
            &llm,
            &metered,
            &mut Scope::for_question(),
            &Input::of(&[]),
            &CallOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(settled, Settled::Answered("red".to_owned()));
        assert_eq!(llm.replies.lock().unwrap().len(), 0);
        assert_eq!(
            metered.used(),
            Usage {
                input: 14,
                output: 6
            },
            "the send and its correction"
        );
    }
}
