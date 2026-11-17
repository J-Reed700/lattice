//! The reading: the model chooses, for each numbered passage shown, the
//! sentences that bear on the question, and the app writes each note from its
//! own copy of the passage. One request shows every passage; the driver sends
//! the passages left over again, alone, and splits what the window cannot hold.

use super::super::clock::TurnClock;
use super::Passages;
use crate::features::conversation::chat::cancellation::cancelled_error;
use lattice_core::application::ports::llm_port::{CompletionRequest, SamplingOverride};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::CallOptions;
use lattice_core::application::services::model_contract::ask::{
    ask_each, Alone, Answered, Plain, Says, Settled,
};
use lattice_core::application::services::model_contract::shape::Keeps;
use lattice_core::application::services::model_contract::shown::{
    Drawn, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::{normalize_whitespace, sentences};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Opens the request that shows the passages.
pub(super) const READ_INSTRUCTION: &str =
    "Do not answer my question yet. First read these passages, each on its own.";

/// Closes it, after the passages and the question. The criterion must follow
/// the passages, and the question is said again because the passages put it
/// far back.
pub(super) const BEARING_INSTRUCTION: &str = "Answer with one line for each passage that has sentences bearing on my question, including any exception or condition: the passage's handle, then the handles of those sentences, as in `p2: u1 u4`. Leave out a passage that says nothing bearing on my question, however near its subject. When my question names a document, leave out every passage from any other document. If no passage bears on my question, answer `none`. Write nothing else.";

/// The call that chooses the sentences of each passage that bear on the question.
pub(super) struct PassageNotes;

impl Contract for PassageNotes {
    const ID: ContractId = ContractId::new("passage-notes");
    type Input = Input;
}

/// One numbered passage, as the model names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Excerpt {
    pub(super) label: u32,
    pub(super) document: Told<String>,
    pub(super) sentences: Vec<Sentence>,
}

impl Kind for Excerpt {
    const PREFIX: &'static str = "p";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = u32;

    fn reference(&self) -> u32 {
        self.label
    }
}

/// One sentence of a passage, named within it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Sentence {
    pub(super) at: usize,
    pub(super) text: Told<String>,
}

impl Kind for Sentence {
    const PREFIX: &'static str = "u";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What is asked about the passages still open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub(super) question: Told<String>,
    pub(super) passages: Vec<Excerpt>,
}

impl Answered for PassageNotes {
    type Answer = Keeps<Excerpt, Sentence>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let passages = showing.show(&input.passages)?;
        let mut task = String::new();
        for (handle, excerpt) in passages.iter() {
            if !task.is_empty() {
                task.push_str("\n\n");
            }
            task.push_str(&format!(
                "{} Document: {}",
                handle.bracketed(),
                excerpt.document.0
            ));
            let own = showing.under(handle, &excerpt.sentences)?;
            for (sentence, thing) in own.iter() {
                task.push_str(&format!("\n{} {}", sentence.bracketed(), thing.text.0));
            }
        }
        let mut says = Says::new(READ_INSTRUCTION, task);
        says.closing = format!(
            "My question is: \"{}\"\n\n{BEARING_INSTRUCTION}",
            input.question.0
        );
        Ok(says)
    }
}

impl Excerpt {
    pub(super) fn of(label: u32, document: &str, text: &str) -> Self {
        let sentences = sentences(text)
            .into_iter()
            .filter_map(|(start, stop)| text.get(start..stop))
            .map(normalize_whitespace)
            .filter(|sentence| !sentence.is_empty())
            .enumerate()
            .map(|(at, sentence)| Sentence {
                at,
                text: Told(sentence),
            })
            .collect();
        Self {
            label,
            document: Told(normalize_whitespace(document)),
            sentences,
        }
    }

    /// The note the chosen sentences make, in the passage's own order, from the
    /// app's copy of them; `None` when none was chosen.
    fn note(&self, picked: &[Picked<Sentence>]) -> Option<String> {
        let mut at: Vec<usize> = picked.iter().map(|one| one.reference).collect();
        at.sort_unstable();
        at.dedup();
        let note = at
            .into_iter()
            .filter_map(|at| self.sentences.get(at))
            .map(|sentence| sentence.text.0.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        (!note.is_empty()).then_some(note)
    }
}

/// Each label's note (`None` when the model chose no sentence of it). A passage
/// whose answer did not settle has no entry: it was not judged.
fn lines_of(
    excerpts: &[Excerpt],
    settled: Vec<Settled<Vec<Picked<Sentence>>>>,
) -> BTreeMap<u32, Option<String>> {
    excerpts
        .iter()
        .zip(settled)
        .filter_map(|(excerpt, settled)| match settled {
            Settled::Answered(picked) => Some((excerpt.label, excerpt.note(&picked))),
            Settled::Unanswered(_) => None,
        })
        .collect()
}

/// Each label's note from the model's choice among the sentences of `passages`,
/// within the turn's time. A provider failure adds nothing; only Stop and the
/// turn's deadline are errors.
pub(super) async fn read(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    passages: &Passages,
    question: &str,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<BTreeMap<u32, Option<String>>> {
    let remaining = clock.remaining()?;
    let excerpts: Vec<Excerpt> = passages
        .0
        .iter()
        .map(|(label, passage)| Excerpt::of(*label, &passage.document, &passage.text))
        .collect();
    let question = Told(question.to_owned());
    let call = CallOptions {
        priority: request.priority,
        cancel: request.cancel.clone(),
        cache_key: request.cache_key.clone(),
        sampling: Some(SamplingOverride::deterministic()),
        time_budget: Some(remaining),
        // Thinking did not help Claude read the passages and took two to three times as long.
        reasoning_effort: Some("none".into()),
        ..Default::default()
    };
    let mut scope = Scope::for_question();
    let settled = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled_error()),
        result = timeout(
            remaining,
            ask_each::<PassageNotes>(
                llm,
                &Plain,
                &mut scope,
                &excerpts,
                |open, _| Input {
                    question: question.clone(),
                    passages: open.iter().map(|excerpt| (*excerpt).clone()).collect(),
                },
                &call,
            ),
        ) => result.map_err(|_| clock.exhausted())?,
    };
    match settled {
        Ok(settled) => Ok(lines_of(&excerpts, settled)),
        Err(_) if cancel.is_cancelled() => Err(cancelled_error()),
        Err(error) => {
            warn!(%error, "The model could not read the passages; answering without notes");
            Ok(BTreeMap::new())
        }
    }
}

/// The text a reading request shows the model.
#[cfg(test)]
pub(super) fn shown_in(request: &CompletionRequest) -> String {
    use lattice_core::application::ports::llm_port::CompletionInput;
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

/// An answer for the passages the prompt shows, as lines: one for each
/// passage whose document `answered` takes and that keeps a sentence `keep`
/// takes. A passage `answered` does not take gets a line naming a sentence
/// that does not exist, which the reading cannot place; `none` when no passage
/// has a line.
#[cfg(test)]
pub(super) fn scripted_answer(
    prompt: &str,
    answered: impl Fn(&str) -> bool,
    keep: impl Fn(&str) -> bool,
) -> String {
    type Shown = (String, String, Vec<(String, String)>);
    let mut shown: Vec<Shown> = Vec::new();
    for line in prompt.lines() {
        let Some((handle, rest)) = line.strip_prefix('[').and_then(|l| l.split_once("] ")) else {
            continue;
        };
        if handle.starts_with('p') {
            shown.push((handle.into(), rest.into(), Vec::new()));
        } else if handle.starts_with('u') {
            if let Some(last) = shown.last_mut() {
                last.2.push((handle.into(), rest.into()));
            }
        }
    }
    let lines: Vec<String> = shown
        .iter()
        .filter_map(|(handle, document, sentences)| {
            if !answered(document) {
                return Some(format!("{handle}: u0"));
            }
            let kept: Vec<&str> = sentences
                .iter()
                .filter(|(_, text)| keep(text))
                .map(|(handle, _)| handle.as_str())
                .collect();
            (!kept.is_empty()).then(|| format!("{handle}: {}", kept.join(" ")))
        })
        .collect();
    if lines.is_empty() {
        "none".to_owned()
    } else {
        lines.join("\n")
    }
}
