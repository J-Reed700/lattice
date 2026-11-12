use super::test_support::{assert_round_trip, RenderAnswer};
use super::*;
use std::sync::Mutex;

struct TestContract;

impl Contract for TestContract {
    const ID: ContractId = ContractId::new("test-contract");
    type Input = PromptInput;
}

impl RenderAnswer for TestContract {
    type Answer = u32;

    fn render(answer: &u32) -> String {
        answer.to_string()
    }

    fn read(text: &str) -> Result<u32> {
        text.trim()
            .parse()
            .map_err(|_| AppError::InvalidInput(format!("`{text}` is not a number")))
    }
}

fn input() -> PromptInput {
    PromptInput {
        system: "system".into(),
        prompt: "prompt".into(),
    }
}

fn call() -> ContractCall {
    ContractCall::of::<TestContract>(&input()).unwrap()
}

fn reply(text: &str, finish_reason: &str) -> CompletionResponse {
    CompletionResponse {
        finish_reason: finish_reason.into(),
        ..CompletionResponse::from_text(text)
    }
}

/// Answers from a script and records every request it was sent.
struct Recorder {
    script: Mutex<Vec<Result<CompletionResponse>>>,
    sent: Mutex<Vec<CompletionRequest>>,
}

impl Recorder {
    fn new(script: Vec<Result<CompletionResponse>>) -> Self {
        Self {
            script: Mutex::new(script.into_iter().rev().collect()),
            sent: Mutex::new(Vec::new()),
        }
    }

    fn sent(&self) -> Vec<CompletionRequest> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl LLMPort for Recorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.sent.lock().unwrap().push(request.clone());
        self.script
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| Err(AppError::InternalError("script ran out".into())))
    }

    fn model_name(&self) -> &str {
        "recorder"
    }

    fn max_context_tokens(&self) -> usize {
        8192
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn plain() -> Plain {
    Plain {
        request: CompletionRequest::default(),
    }
}

async fn ask(llm: &Recorder) -> Result<u32> {
    let transport = plain();
    let request = transport.request.clone();
    answer(llm, &call(), &transport, request, TestContract::read).await
}

fn last_user_turn(request: &CompletionRequest) -> String {
    request
        .input
        .iter()
        .rev()
        .find_map(|item| match item {
            CompletionInput::Message { role, content } if role == "user" => Some(content.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn every_send_through_the_driver_carries_its_contract() {
    let llm = Recorder::new(vec![
        Ok(reply("not a number", "stop")),
        Ok(reply("7", "stop")),
    ]);
    assert_eq!(ask(&llm).await.unwrap(), 7);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    for request in &sent {
        assert_eq!(request.contract, Some(call()));
    }
}

#[tokio::test]
async fn a_reply_that_is_refused_gets_one_correction_quoting_why() {
    let llm = Recorder::new(vec![Ok(reply("abc", "stop")), Ok(reply("3", "stop"))]);
    assert_eq!(ask(&llm).await.unwrap(), 3);
    let sent = llm.sent();
    let correction = last_user_turn(&sent[1]);
    assert!(correction.starts_with(CORRECTION_LEAD), "{correction}");
    assert!(correction.contains("`abc` is not a number"), "{correction}");
}

#[tokio::test]
async fn a_reply_that_stays_refused_is_unusable_after_one_correction() {
    let llm = Recorder::new(vec![
        Ok(reply("abc", "stop")),
        Ok(reply("def", "stop")),
        Ok(reply("5", "stop")),
    ]);
    let error = ask(&llm).await.unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(why) if why.contains("`def`")),
        "{error:?}"
    );
    assert_eq!(llm.sent().len(), 2);
}

#[tokio::test]
async fn a_cut_off_reply_with_no_larger_send_gets_one_correction_saying_so() {
    let llm = Recorder::new(vec![Ok(reply("1", "length")), Ok(reply("12", "stop"))]);
    assert_eq!(ask(&llm).await.unwrap(), 12);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    let correction = last_user_turn(&sent[1]);
    assert!(correction.contains(CORRECTION_LEAD), "{correction}");
    assert!(correction.contains(CUT_OFF_REFUSAL), "{correction}");
}

#[tokio::test]
async fn a_reply_cut_off_again_after_its_correction_is_a_cut_off_answer() {
    let llm = Recorder::new(vec![
        Ok(reply("1", "length")),
        Ok(reply("1", "length")),
        Ok(reply("12", "stop")),
    ]);
    let error = ask(&llm).await.unwrap_err();
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error:?}");
    assert_eq!(llm.sent().len(), 2);
}

#[tokio::test]
async fn a_cut_off_reply_of_a_call_that_splits_is_a_cut_off_answer_after_one_send() {
    let llm = Recorder::new(vec![Ok(reply("1", "length")), Ok(reply("12", "stop"))]);
    let transport = plain();
    let request = transport.request.clone();
    let error = answer(
        &llm,
        &call().splitting(true),
        &transport,
        request,
        TestContract::read,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error:?}");
    assert_eq!(llm.sent().len(), 1);
}

#[tokio::test]
async fn a_cut_off_reply_of_a_call_that_splits_is_not_sent_larger() {
    let llm = Recorder::new(vec![Ok(reply("1", "length")), Ok(reply("9", "stop"))]);
    let error = answer(
        &llm,
        &call().splitting(true),
        &Growing,
        CompletionRequest::default(),
        TestContract::read,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error:?}");
    assert_eq!(llm.sent().len(), 1);
}

#[tokio::test]
async fn a_declined_reply_is_unusable_without_a_correction() {
    let llm = Recorder::new(vec![Ok(reply("", "refusal")), Ok(reply("1", "stop"))]);
    let error = ask(&llm).await.unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(why) if why.contains("declined")),
        "{error:?}"
    );
    assert_eq!(llm.sent().len(), 1);
}

#[tokio::test]
async fn a_transport_error_passes_through_unchanged() {
    let llm = Recorder::new(vec![Err(AppError::Network("down".into()))]);
    let error = ask(&llm).await.unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert_eq!(llm.sent().len(), 1);
}

struct Growing;

#[async_trait]
impl Transport for Growing {
    type Request = CompletionRequest;

    async fn send(
        &self,
        llm: &dyn LLMPort,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse> {
        llm.complete(request).await
    }

    async fn correction(
        &self,
        _llm: &dyn LLMPort,
        request: &CompletionRequest,
        _previous: &CompletionResponse,
        _refusal: &str,
    ) -> Result<CompletionRequest> {
        Ok(request.clone())
    }

    async fn cut_off(
        &self,
        _llm: &dyn LLMPort,
        request: &CompletionRequest,
    ) -> CutOffRemedy<CompletionRequest> {
        if request.max_output_tokens.is_some() {
            return CutOffRemedy::Stop;
        }
        CutOffRemedy::Larger(CompletionRequest {
            max_output_tokens: Some(2048),
            ..request.clone()
        })
    }
}

#[tokio::test]
async fn a_cut_off_reply_is_sent_once_more_with_a_larger_allowance() {
    let llm = Recorder::new(vec![Ok(reply("1", "length")), Ok(reply("9", "stop"))]);
    let read = answer(
        &llm,
        &call(),
        &Growing,
        CompletionRequest::default(),
        TestContract::read,
    )
    .await
    .unwrap();
    assert_eq!(read, 9);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].max_output_tokens, Some(2048));
}

#[tokio::test]
async fn a_reply_still_cut_off_after_the_larger_send_is_a_cut_off_answer() {
    let llm = Recorder::new(vec![Ok(reply("1", "length")), Ok(reply("1", "length"))]);
    let error = answer(
        &llm,
        &call(),
        &Growing,
        CompletionRequest::default(),
        TestContract::read,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error:?}");
    assert_eq!(llm.sent().len(), 2);
}

#[test]
fn a_draft_call_keeps_the_id_and_input_and_changes_the_digest() {
    let call = call();
    let draft = call.draft();
    assert_eq!(draft.id, call.id);
    assert_eq!(draft.input, call.input);
    assert_eq!(draft.step, Step::Draft);
    assert_ne!(draft.digest(), call.digest());
    assert_eq!(call.digest(), self::call().digest());
}

#[test]
fn a_call_gives_back_its_typed_input_only_to_its_own_contract() {
    struct Other;
    impl Contract for Other {
        const ID: ContractId = ContractId::new("other");
        type Input = PromptInput;
    }
    let call = call();
    assert_eq!(call.input_as::<TestContract>().unwrap().prompt, "prompt");
    assert!(call.input_as::<Other>().is_none());
}

#[test]
fn the_test_contract_reads_what_it_renders() {
    assert_round_trip::<TestContract>(&42);
}

#[test]
fn a_refusal_is_the_errors_own_text() {
    assert_eq!(refusal(&AppError::InvalidInput("why".into())), "why");
    assert_eq!(refusal(&AppError::ModelAnswerUnusable("why".into())), "why");
    assert_eq!(refusal(&AppError::AnswerCutOff("why".into())), "why");
}
