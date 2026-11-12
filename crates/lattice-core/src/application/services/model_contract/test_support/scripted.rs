use super::RenderAnswer;
use crate::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, LLMPort,
};
use crate::application::services::model_contract::{Contract, ContractCall, ContractId};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The context window a scripted model reports until a test sets one.
pub const SCRIPTED_WINDOW: usize = 32_768;

const CUT_OFF_REASON: &str = "length";

/// How a scripted model answers one call.
pub enum Reply<A> {
    Answer(A),
    /// The answer, with a finish that says the length ran out.
    CutOff(A),
    /// Text no reader accepts, to test the correction path.
    Unreadable(String),
    Fail(AppError),
}

impl<A> Reply<A> {
    fn into_response<C: RenderAnswer<Answer = A>>(self) -> Result<CompletionResponse> {
        match self {
            Self::Answer(answer) => Ok(CompletionResponse::from_text(C::render(&answer))),
            Self::CutOff(answer) => Ok(CompletionResponse {
                finish_reason: CUT_OFF_REASON.into(),
                ..CompletionResponse::from_text(C::render(&answer))
            }),
            Self::Unreadable(text) => Ok(CompletionResponse::from_text(text)),
            Self::Fail(error) => Err(error),
        }
    }
}

/// A request a scripted model was sent, with the contract it names.
#[derive(Debug, Clone)]
pub struct Sent {
    pub call: ContractCall,
    pub request: CompletionRequest,
}

type Handler =
    Box<dyn Fn(&Sent, usize) -> BoxFuture<'static, Result<CompletionResponse>> + Send + Sync>;

type Outage = Box<dyn Fn(&Sent) -> AppError + Send + Sync>;

#[derive(Default)]
struct Record {
    sent: Vec<Sent>,
    calls: HashMap<ContractId, usize>,
    misses: Vec<String>,
}

/// An `LLMPort` that answers each request from the handler of its contract,
/// in the text that contract's own renderer writes.
pub struct ScriptedModel {
    name: String,
    window: usize,
    exact_tokens: bool,
    handlers: HashMap<ContractId, Handler>,
    outage: Option<Outage>,
    record: Mutex<Record>,
}

impl Default for ScriptedModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptedModel {
    pub fn new() -> Self {
        Self {
            name: "scripted-model".into(),
            window: SCRIPTED_WINDOW,
            exact_tokens: false,
            handlers: HashMap::new(),
            outage: None,
            record: Mutex::new(Record::default()),
        }
    }

    pub fn named(mut self, name: &str) -> Self {
        self.name = name.to_owned();
        self
    }

    pub fn window(mut self, tokens: usize) -> Self {
        self.window = tokens;
        self
    }

    pub fn exact_tokens(mut self, exact: bool) -> Self {
        self.exact_tokens = exact;
        self
    }

    /// Answer every call of `C` with `C::render` of the answer.
    pub fn on<C: RenderAnswer + 'static>(
        self,
        answer: impl Fn(&C::Input) -> C::Answer + Send + Sync + 'static,
    ) -> Self {
        self.on_nth::<C>(move |input, _| Reply::Answer(answer(input)))
    }

    /// Answer a call of `C` by how many calls of `C` came before it.
    pub fn on_nth<C: RenderAnswer + 'static>(
        mut self,
        reply: impl Fn(&C::Input, usize) -> Reply<C::Answer> + Send + Sync + 'static,
    ) -> Self {
        self.handlers.insert(
            C::ID,
            Box::new(move |sent, nth| {
                let result = match sent.call.input_as::<C>() {
                    Some(input) => reply(&input, nth).into_response::<C>(),
                    None => Err(AppError::InternalError(format!(
                        "the {} input could not be read back: {}",
                        C::ID.as_str(),
                        sent.call.input
                    ))),
                };
                Box::pin(std::future::ready(result))
            }),
        );
        self
    }

    /// Handle every call of `C` as a whole request, for transport tests.
    pub fn on_request<C: Contract>(
        mut self,
        handler: impl Fn(&Sent, usize) -> BoxFuture<'static, Result<CompletionResponse>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.handlers.insert(C::ID, Box::new(handler));
        self
    }

    /// The backend is down: every request gets the error `error` makes of it.
    pub fn failing_every_request(
        mut self,
        error: impl Fn(&Sent) -> AppError + Send + Sync + 'static,
    ) -> Self {
        self.outage = Some(Box::new(error));
        self
    }

    pub fn sent(&self) -> Vec<Sent> {
        self.record().sent.clone()
    }

    pub fn inputs<C: Contract>(&self) -> Vec<C::Input> {
        self.record()
            .sent
            .iter()
            .filter_map(|sent| sent.call.input_as::<C>())
            .collect()
    }

    pub fn count<C: Contract>(&self) -> usize {
        self.record()
            .sent
            .iter()
            .filter(|sent| sent.call.id == C::ID)
            .count()
    }

    fn record(&self) -> MutexGuard<'_, Record> {
        self.record.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn miss(&self, what: String) -> Result<CompletionResponse> {
        self.record().misses.push(what.clone());
        Err(AppError::InternalError(what))
    }

    async fn answer(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let Some(call) = request.contract.clone() else {
            return self.miss(format!(
                "a request names no contract: {}",
                first_line_of(request)
            ));
        };
        let (sent, nth) = {
            let mut record = self.record();
            let calls = record.calls.entry(call.id).or_default();
            let nth = *calls;
            *calls += 1;
            let sent = Sent {
                call,
                request: request.clone(),
            };
            record.sent.push(sent.clone());
            (sent, nth)
        };
        if let Some(outage) = &self.outage {
            return Err(outage(&sent));
        }
        let Some(handler) = self.handlers.get(&sent.call.id) else {
            return self.miss(format!(
                "no scripted answer for {} with input {}",
                sent.call.id.as_str(),
                sent.call.input
            ));
        };
        handler(&sent, nth).await
    }
}

fn first_line_of(request: &CompletionRequest) -> String {
    request
        .input
        .iter()
        .rev()
        .find_map(|input| match input {
            CompletionInput::Message { content, .. } => content.lines().next(),
            _ => None,
        })
        .unwrap_or_default()
        .to_owned()
}

impl Drop for ScriptedModel {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let misses = std::mem::take(&mut self.record().misses);
        assert!(
            misses.is_empty(),
            "the scripted model was sent {} request(s) nothing answers:\n{}",
            misses.len(),
            misses.join("\n")
        );
    }
}

#[async_trait]
impl LLMPort for ScriptedModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.answer(request).await
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let response = self.answer(request).await?;
        on_text(response.text.clone())?;
        Ok(response)
    }

    fn model_name(&self) -> &str {
        &self.name
    }

    fn max_context_tokens(&self) -> usize {
        self.window
    }

    fn counts_tokens_exactly(&self) -> bool {
        self.exact_tokens
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::llm_port::Finish;
    use crate::application::services::model_contract::{tagged, PromptInput};

    struct Number;

    impl Contract for Number {
        const ID: ContractId = ContractId::new("scripted-number");
        type Input = PromptInput;
    }

    impl RenderAnswer for Number {
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

    struct Word;

    impl Contract for Word {
        const ID: ContractId = ContractId::new("scripted-word");
        type Input = PromptInput;
    }

    impl RenderAnswer for Word {
        type Answer = String;

        fn render(answer: &String) -> String {
            answer.clone()
        }

        fn read(text: &str) -> Result<String> {
            Ok(text.to_owned())
        }
    }

    fn input(prompt: &str) -> PromptInput {
        PromptInput {
            system: "system".into(),
            prompt: prompt.into(),
        }
    }

    async fn ask<C: Contract<Input = PromptInput>>(
        model: &ScriptedModel,
        prompt: &str,
    ) -> Result<CompletionResponse> {
        let call = ContractCall::of::<C>(&input(prompt)).unwrap();
        tagged(model, &call)
            .complete(&CompletionRequest::default())
            .await
    }

    #[tokio::test]
    async fn a_call_is_answered_by_the_handler_of_its_contract() {
        let model = ScriptedModel::new()
            .on::<Number>(|input| u32::try_from(input.prompt.len()).unwrap())
            .on::<Word>(|input| input.prompt.to_uppercase());
        assert_eq!(ask::<Number>(&model, "four").await.unwrap().text, "4");
        assert_eq!(ask::<Word>(&model, "four").await.unwrap().text, "FOUR");
        assert_eq!(model.count::<Number>(), 1);
        assert_eq!(model.count::<Word>(), 1);
        assert_eq!(model.inputs::<Word>()[0].prompt, "four");
        let ids: Vec<_> = model.sent().iter().map(|sent| sent.call.id).collect();
        assert_eq!(ids, [Number::ID, Word::ID]);
    }

    #[tokio::test]
    async fn the_handler_is_told_how_many_calls_of_its_contract_came_before() {
        let model = ScriptedModel::new()
            .on_nth::<Number>(|_, nth| Reply::Answer(u32::try_from(nth).unwrap()))
            .on::<Word>(|_| "x".into());
        ask::<Number>(&model, "a").await.unwrap();
        ask::<Word>(&model, "a").await.unwrap();
        assert_eq!(ask::<Number>(&model, "a").await.unwrap().text, "1");
    }

    #[tokio::test]
    async fn a_cut_off_reply_says_the_length_ran_out() {
        let model = ScriptedModel::new().on_nth::<Number>(|_, nth| match nth {
            0 => Reply::CutOff(7),
            _ => Reply::Answer(7),
        });
        let first = ask::<Number>(&model, "a").await.unwrap();
        assert_eq!((first.text.as_str(), first.finish()), ("7", Finish::CutOff));
        let second = ask::<Number>(&model, "a").await.unwrap();
        assert_eq!(second.finish(), Finish::Complete);
    }

    #[tokio::test]
    async fn unreadable_text_and_failures_pass_through() {
        let model = ScriptedModel::new().on_nth::<Number>(|_, nth| match nth {
            0 => Reply::Unreadable("seven".into()),
            _ => Reply::Fail(AppError::ServiceNotAvailable("down".into())),
        });
        let text = ask::<Number>(&model, "a").await.unwrap().text;
        assert_eq!(text, "seven");
        assert!(Number::read(&text).is_err());
        let error = ask::<Number>(&model, "a").await.unwrap_err();
        assert!(
            matches!(error, AppError::ServiceNotAvailable(_)),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_whole_request_handler_gets_the_request_and_its_place() {
        let model = ScriptedModel::new().on_request::<Number>(|sent, nth| {
            let prompt = sent.call.input["prompt"].as_str().unwrap_or_default();
            let text = format!("{prompt}:{nth}");
            Box::pin(async move { Ok(CompletionResponse::from_text(text)) })
        });
        ask::<Number>(&model, "p").await.unwrap();
        assert_eq!(ask::<Number>(&model, "q").await.unwrap().text, "q:1");
    }

    #[tokio::test]
    async fn a_failing_backend_fails_every_request_with_the_error_it_makes() {
        let model = ScriptedModel::new().failing_every_request(|sent| {
            AppError::ServiceNotAvailable(format!("{} is down", sent.call.id.as_str()))
        });
        let error = ask::<Word>(&model, "a").await.unwrap_err();
        assert!(
            error.to_string().contains("scripted-word is down"),
            "{error}"
        );
        assert_eq!(model.count::<Word>(), 1);
    }

    #[tokio::test]
    async fn a_progress_callback_is_given_the_whole_reply_once() {
        let model = ScriptedModel::new().on::<Word>(|_| "the whole reply".into());
        let call = ContractCall::of::<Word>(&input("a")).unwrap();
        let shown = Mutex::new(Vec::new());
        let response = tagged(&model, &call)
            .complete_with_progress(&CompletionRequest::default(), &|text| {
                shown.lock().unwrap().push(text);
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(*shown.lock().unwrap(), [response.text]);
    }

    #[test]
    fn it_reports_what_it_was_told_to() {
        let model = ScriptedModel::new()
            .named("tiny")
            .window(900)
            .exact_tokens(true);
        assert_eq!(model.model_name(), "tiny");
        assert_eq!(model.max_context_tokens(), 900);
        assert!(model.counts_tokens_exactly());
        let default = ScriptedModel::new();
        assert_eq!(default.max_context_tokens(), SCRIPTED_WINDOW);
        assert!(!default.counts_tokens_exactly());
    }

    #[tokio::test]
    async fn a_request_nothing_answers_is_an_error_naming_it() {
        let model = ScriptedModel::new().on::<Word>(|_| "x".into());
        let error = ask::<Number>(&model, "a").await.unwrap_err();
        assert!(error.to_string().contains("scripted-number"), "{error}");
        assert!(error.to_string().contains("\"prompt\":\"a\""), "{error}");
        let bare = model
            .complete(&CompletionRequest {
                input: vec![CompletionInput::Message {
                    role: "user".into(),
                    content: "First line\nsecond".into(),
                }],
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert!(bare.to_string().contains("names no contract: First line"));
        assert_eq!(model.record().misses.len(), 2);
        model.record().misses.clear();
    }

    #[tokio::test]
    #[should_panic(expected = "nothing answers")]
    async fn a_model_dropped_after_a_missed_request_fails_the_test() {
        let model = ScriptedModel::new();
        let _ = ask::<Number>(&model, "a").await;
    }
}
