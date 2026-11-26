//! The fixtures of the part-wise reading (claim classes design §3.3): a
//! statement is read part by part against the passages its sentence cites,
//! and a page retrieved from the course is read by the plain strict judge.
use super::evidence_paging_tests::{inventory, marker_collection, reference, CLAIM};
use super::readings_tests::paged;
use super::*;
use lattice_core::application::services::claim_verification::BRIDGE_SYSTEM;
use std::collections::VecDeque;

/// A statement of the shape the design's third section takes from a real
/// lesson: its source states the first half, and what a lexer does is common
/// knowledge.
pub(super) const STATEMENT: &str = "Quillet is a lexer that turns characters into tokens.";
pub(super) const STATED: &str = "Quillet is a lexer shipped with the toolchain.";

/// Answers a part-wise check with the next of its replies and any other check
/// as the strict judge that finds nothing, and records the system prompt of
/// every request.
pub(super) struct Reading {
    replies: Mutex<VecDeque<String>>,
    systems: Mutex<Vec<String>>,
}

impl Reading {
    pub(super) fn new(replies: Vec<String>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            systems: Mutex::new(Vec::new()),
        }
    }

    /// How many requests were part-wise checks, and how many were not.
    pub(super) fn requests(&self) -> (usize, usize) {
        let systems = self.systems.lock().unwrap();
        let part_wise = systems
            .iter()
            .filter(|system| *system == BRIDGE_SYSTEM)
            .count();
        (part_wise, systems.len() - part_wise)
    }
}

#[async_trait::async_trait]
impl LLMPort for Reading {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = fixture_messages(request);
        self.systems.lock().unwrap().push(system.to_owned());
        if system == BRIDGE_SYSTEM {
            let reply = self.replies.lock().unwrap().pop_front().unwrap_or_default();
            return Ok(CompletionResponse::from_text(resolved_parts(&reply, user)));
        }
        Ok(CompletionResponse::from_text(
            "Reason: The passages do not state it.\nSource passage: none\nVerdict: unsupported",
        ))
    }
    fn model_name(&self) -> &str {
        "reading-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

/// The one source the statement cites: nothing is left for a second page.
pub(super) fn sources() -> Vec<LearningSourceDto> {
    vec![reference("quillet".into(), STATED.into())]
}

/// Unit 0 cites the passage that states half of the statement.
pub(super) fn citing(references: &ReferenceCollection<'_>) -> Result<Citations> {
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[STATED])?)]),
        HashMap::new(),
    ))
}

/// The part-wise judge's reply for `parts`, each written with the `passage`
/// (`passage-N` or null) of the script's place it rests on. The reply names that
/// place in `from`; [`resolved_parts`] gives it as the sentences the judge is
/// shown once the request is known. The verdict a script names is not asked of
/// the judge any more.
pub(super) fn parts_json(parts: Value, reason: &str) -> String {
    let parts: Vec<Value> = parts
        .as_array()
        .into_iter()
        .flatten()
        .map(|part| {
            json!({
                "words": part["part"],
                "kind": part["kind"],
                "from": part["passage"].as_str().into_iter().collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({"parts": parts, "reason": reason}).to_string()
}

/// The passages of a part-wise request, each with its text as shown.
fn shown_chunks(user: &str) -> Vec<&str> {
    user.strip_prefix("Source passages:\n")
        .and_then(|rest| rest.split("\n\nClaim: ").next())
        .map(|listed| listed.split("\n\n").collect())
        .unwrap_or_default()
}

/// The sentence handles (`s1`) `chunk` carries, in order, and the chunk with
/// their marks taken out.
fn marks_of(chunk: &str) -> (Vec<String>, String) {
    let (mut handles, mut plain, mut rest) = (Vec::new(), String::new(), chunk);
    while let Some(open) = rest.find("[s") {
        let digits: String = rest
            .get(open + 2..)
            .unwrap_or_default()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let close = open + 2 + digits.len();
        if !digits.is_empty() && rest.get(close..).is_some_and(|tail| tail.starts_with("] ")) {
            handles.push(format!("s{digits}"));
            plain.push_str(rest.get(..open).unwrap_or_default());
            rest = rest.get(close + 2..).unwrap_or_default();
        } else {
            plain.push_str(rest.get(..open + 2).unwrap_or_default());
            rest = rest.get(open + 2..).unwrap_or_default();
        }
    }
    plain.push_str(rest);
    (handles, plain)
}

/// A part-wise request as the fixtures read it: each passage under the label
/// `[passage-N]` its place gives, without the sentence marks. Any other
/// request is returned as written.
pub(super) fn passages_by_place(user: &str) -> String {
    let chunks = shown_chunks(user);
    if !chunks.iter().any(|chunk| chunk.contains("[s1] ")) {
        return user.to_owned();
    }
    let passages = chunks
        .iter()
        .enumerate()
        .map(|(place, chunk)| format!("[passage-{place}]\n{}", marks_of(chunk).1))
        .collect::<Vec<_>>()
        .join("\n\n");
    let claim = user.split("\n\nClaim: ").nth(1).unwrap_or_default();
    format!("Source passages:\n{passages}\n\nClaim: {claim}")
}

/// The part-wise reply a script wrote, as the judge answers `user`: a part's
/// `passage-N` in `from` is every sentence of that passage, and words that are
/// not in the claim are the whole claim.
pub(super) fn resolved_parts(reply: &str, user: &str) -> String {
    let Ok(mut answer) = serde_json::from_str::<Value>(reply) else {
        return reply.to_owned();
    };
    let claim = user
        .split("\n\nClaim: ")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .unwrap_or_default();
    let chunks = shown_chunks(user);
    let Some(parts) = answer["parts"].as_array_mut() else {
        return reply.to_owned();
    };
    for part in parts {
        let words = part["words"].as_str().unwrap_or_default();
        if lattice_core::shared::text::same_text::find(claim, words).is_none() {
            part["words"] = json!(claim);
        }
        let from: Vec<String> = part["from"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .flat_map(|written| {
                match written
                    .strip_prefix("passage-")
                    .and_then(|place| place.parse::<usize>().ok())
                {
                    Some(place) => chunks
                        .get(place)
                        .map(|chunk| marks_of(chunk).0)
                        .unwrap_or_default(),
                    None => vec![written.to_owned()],
                }
            })
            .collect();
        part["from"] = json!(from);
    }
    answer.to_string()
}

pub(super) fn parts_reply(parts: Value, _verdict: &str) -> String {
    parts_json(parts, "The passage says Quillet is a lexer.")
}

pub(super) fn half_stated() -> String {
    parts_reply(
        json!([
            {"part": "Quillet is a lexer", "kind": "stated", "passage": "passage-0"},
            {"part": "turns characters into tokens", "kind": "common", "passage": null},
        ]),
        "supported_with_common",
    )
}

#[tokio::test]
async fn a_page_retrieved_from_the_course_is_read_by_the_plain_strict_judge() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // Nothing is cited: the claim meets the course's first page for it.
    let model = Reading::new(vec![half_stated()]);
    let reads = paged(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &ClaimChecks::default(),
    )
    .await?;
    let (part_wise, strict) = model.requests();
    assert_eq!(part_wise, 0);
    assert!(strict > 0, "the page was judged");
    assert_eq!(reads[0].finding().verdict, ClaimVerdict::Unsupported);
    assert!(reads[0].finding().two_pass.parts.is_empty());
    Ok(())
}
