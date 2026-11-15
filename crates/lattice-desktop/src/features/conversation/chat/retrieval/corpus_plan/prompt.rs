//! The planner's user turn, sized in tokens. The window is the limit: the
//! prompt target is the room it leaves after the plan's reservation, held
//! lower only when a server refused a prompt as larger than its real window
//! (see `health`). Rows are chosen on the port's estimate and the finished
//! prompt is then counted with the server's own tokenizer, since an estimate
//! is never trusted with the whole window.

use std::collections::{HashMap, HashSet};

use tracing::warn;

use lattice_core::application::contracts::search::CorpusDocument;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::Answered;
use lattice_core::application::services::model_contract::shown::{Scope, Told};
use lattice_core::shared::error::{AppError, Result};

use super::answer::{catalog_json, Correction, PlanContract, PlanInput, Row};
use super::{PLANNER_SYSTEM, SUMMARY_CATALOG_CHARS};

/// What the planner may write back, enforced on the request and reserved out
/// of the window. A full plan (three short queries, four handles, the JSON
/// around them) is under 200 tokens even pretty-printed.
pub(super) const PLANNER_OUTPUT_TOKENS: u32 = 384;

/// The chat template's role markers and turn delimiters around the two
/// messages, which the tokenizer is not asked to count.
const TEMPLATE_FRAMING_TOKENS: usize = 32;

/// Characters of each document's opening a compact row shows. Enough to tell
/// an introduction from a change log or a chapter. Rows are compact while the
/// prompt budget binds, so it holds more documents; when every row fits, what
/// the budget has left shows rows whole.
const OPENING_CHARS: usize = 300;

/// Extracted headings a compact row shows; a whole row shows every heading
/// the catalog read.
const SECTIONS_SHOWN: usize = 8;

/// The share of the prompt target the conversation excerpt may take.
/// Resolving a follow-up needs the last exchange, not the whole excerpt the
/// HyDE context builder assembles; the share grows with the target.
const HISTORY_SHARE_DIVISOR: usize = 5;

/// Re-selections after an exact count found the estimate read low.
const RECOUNTS: usize = 2;

/// An estimate can read low on text it was not calibrated on, so only part of
/// the window is spent on one. An exact count needs no margin.
fn spendable(room: usize) -> usize {
    room * 4 / 5
}

/// The prompt, system prompt included, the planner's window holds once the
/// plan's reservation and the chat template's framing are paid for.
pub(super) fn window_room(llm: &dyn LLMPort) -> usize {
    llm.max_context_tokens()
        .saturating_sub(PLANNER_OUTPUT_TOKENS as usize + TEMPLATE_FRAMING_TOKENS)
}

/// What a prompt may measure and still be sent, by how it was measured.
fn sendable(llm: &dyn LLMPort, exact: bool) -> usize {
    let room = window_room(llm);
    if exact {
        room
    } else {
        spendable(room)
    }
}

fn clipped(text: &str, chars: usize) -> String {
    text.chars()
        .take(chars)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// `document` as one catalog row. Its book is named by a short handle ("g1")
/// rather than a UUID, which is long on tokenizers that split digits one by one.
fn row(
    document: &CorpusDocument,
    group_handle: Option<String>,
    summaries: &HashMap<String, String>,
    whole: bool,
) -> Row {
    let (opening_chars, sections_shown, summary_chars) = if whole {
        (usize::MAX, usize::MAX, usize::MAX)
    } else {
        (OPENING_CHARS, SECTIONS_SHOWN, SUMMARY_CATALOG_CHARS)
    };
    Row {
        id: document.id.clone(),
        name: Told(document.name.clone()),
        opening: Told(clipped(&document.opening, opening_chars)),
        chapter_number: document.chapter_number.map(Told),
        source_context: document.source_context.as_ref().map(|context| {
            let mut context = context.clone();
            if let Some(handle) = group_handle {
                context.group.id = handle;
            }
            Told(context)
        }),
        sections: Told(
            document
                .sections
                .get(..sections_shown.min(document.sections.len()))
                .unwrap_or_default()
                .to_vec(),
        ),
        summary: summaries
            .get(&document.id)
            .map(|summary| Told(clipped(summary, summary_chars))),
    }
}

/// Everything in the planner's user turn except the catalog rows.
pub(super) struct PromptParts<'a> {
    pub question: &'a str,
    pub history: &'a str,
    pub catalog_total: usize,
    pub correction: Option<&'a Correction>,
    pub summaries: &'a HashMap<String, String>,
}

/// A planner prompt ready to send.
pub(super) struct PlannerPrompt {
    /// The rows the planner is shown, in catalog order.
    pub documents: Vec<Row>,
    pub shown: usize,
    /// System prompt plus user turn, as counted.
    pub tokens: usize,
    /// Whether `tokens` came from the server's tokenizer.
    pub exact: bool,
    /// The size this prompt was built to.
    pub limit: usize,
}

/// Tokens of conversation a planner prompt of `target` tokens carries. The
/// excerpt is built to this size from whole turns, newest first, with a line
/// counting the earlier ones left out (see `query_planning_context`).
pub(in super::super) fn history_tokens(target: usize) -> usize {
    target / HISTORY_SHARE_DIVISOR
}

/// The newest whole lines of the conversation excerpt that fit its share of
/// a `target`-token prompt, with a line counting the ones left out. The
/// excerpt is normally built to that share already; this keeps the prompt
/// inside its target when it was not, without cutting a line.
pub(super) fn recent_history(llm: &dyn LLMPort, history: Option<&str>, target: usize) -> String {
    use lattice_core::application::services::history_share::newest_that_fit;
    let lines: Vec<&str> = history.unwrap_or("").lines().collect();
    let (kept, omitted) = newest_that_fit(&lines, history_tokens(target), |line| {
        llm.count_tokens(line).saturating_add(1)
    });
    (omitted > 0)
        .then(|| format!("({omitted} earlier lines of the conversation not shown)"))
        .into_iter()
        .chain(kept.iter().map(|line| (*line).to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render the user turn with `shown` (in priority order) as its catalog. Rows
/// appear in catalog order, which is reading order. Returns the turn as the
/// planner is asked it and the rows it shows.
fn render(
    parts: &PromptParts<'_>,
    shown: &[&CorpusDocument],
    catalog: &[CorpusDocument],
    whole: &HashSet<&str>,
) -> Result<(String, Vec<Row>)> {
    let position: HashMap<&str, usize> = catalog
        .iter()
        .enumerate()
        .map(|(index, document)| (document.id.as_str(), index))
        .collect();
    let mut in_order = shown.to_vec();
    in_order.sort_by_key(|document| position.get(document.id.as_str()).copied());
    let mut group_handles: HashMap<&str, String> = HashMap::new();
    let mut rows = Vec::with_capacity(in_order.len());
    for document in &in_order {
        let group_handle = document.source_context.as_ref().map(|context| {
            let next = group_handles.len() + 1;
            group_handles
                .entry(context.group.id.as_str())
                .or_insert_with(|| format!("g{next}"))
                .clone()
        });
        rows.push(row(
            document,
            group_handle,
            parts.summaries,
            whole.contains(document.id.as_str()),
        ));
    }
    let input = PlanInput {
        question: Told(parts.question.to_owned()),
        history: Told(parts.history.to_owned()),
        catalog_total: Told(parts.catalog_total),
        correction: parts.correction.cloned().map(Told),
        documents: rows,
    };
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let says = PlanContract::says(&input, &mut showing)?;
    Ok((says.task, input.documents))
}

/// Tokens in the system prompt and `user`, from the server's tokenizer where
/// it answers, otherwise the port's estimate. Returns whether it was exact.
async fn count(llm: &dyn LLMPort, user: &str) -> (usize, bool) {
    if llm.counts_tokens_exactly() {
        match (
            llm.count_tokens_exact(PLANNER_SYSTEM).await,
            llm.count_tokens_exact(user).await,
        ) {
            (Ok(system), Ok(user)) => return (system + user, true),
            (Err(error), _) | (_, Err(error)) => {
                warn!(%error, "Planner prompt could not be counted exactly; using the estimate")
            }
        }
    }
    (
        llm.count_tokens(PLANNER_SYSTEM) + llm.count_tokens(user),
        false,
    )
}

/// What `document`'s row costs on the port's estimate, the comma after it
/// included. Costed with the widest handles a shortlist uses, so the real
/// render is never longer than what was budgeted for it.
fn row_cost(
    llm: &dyn LLMPort,
    document: &CorpusDocument,
    summaries: &HashMap<String, String>,
    whole: bool,
) -> usize {
    let group = document.source_context.as_ref().map(|_| "g99".to_string());
    let json =
        catalog_json(&row(document, group, summaries, whole), "d99").unwrap_or_default();
    llm.count_tokens(&json) + 1
}

/// What the prompt costs before any catalog row: the system prompt and the
/// user turn around an empty catalog.
fn base_tokens(
    llm: &dyn LLMPort,
    parts: &PromptParts<'_>,
    catalog: &[CorpusDocument],
) -> Result<usize> {
    let (empty, _) = render(parts, &[], catalog, &HashSet::new())?;
    Ok(llm.count_tokens(PLANNER_SYSTEM) + llm.count_tokens(&empty))
}

/// How many catalog rows a prompt built to `target` holds beside `parts`:
/// the room [`build`] fills with rows, over the catalog's mean compact-row
/// cost. The shortlist search ranks this many documents, since the prompt
/// shows no more of them.
pub(super) fn rows_held(
    llm: &dyn LLMPort,
    parts: &PromptParts<'_>,
    catalog: &[CorpusDocument],
    target: usize,
) -> Result<usize> {
    let limit = target.min(sendable(llm, llm.counts_tokens_exactly()));
    let room = limit.saturating_sub(base_tokens(llm, parts, catalog)?);
    let catalog_cost: usize = catalog
        .iter()
        .map(|document| row_cost(llm, document, parts.summaries, false))
        .sum();
    Ok((room * catalog.len())
        .checked_div(catalog_cost)
        .map_or(0, |rows| rows.min(catalog.len())))
}

/// Rows from the front of `ranked` while their scaled cost fits `room`, and
/// their unscaled estimate. The budget alone decides how many.
fn take_rows<'a>(
    ranked: &[&'a CorpusDocument],
    costs: &[usize],
    scale: f64,
    room: f64,
) -> (Vec<&'a CorpusDocument>, usize) {
    let mut spent = 0;
    let mut shown = Vec::new();
    for (document, cost) in ranked.iter().zip(costs) {
        if (spent + cost) as f64 * scale > room {
            continue;
        }
        spent += cost;
        shown.push(*document);
    }
    (shown, spent)
}

/// Build the planner's user turn from `ranked` (best first) within `target`
/// tokens and the model's window.
///
/// Fails, without anything being sent, when not even the system prompt and
/// the request fit the window.
pub(super) async fn build(
    llm: &dyn LLMPort,
    parts: &PromptParts<'_>,
    ranked: &[&CorpusDocument],
    catalog: &[CorpusDocument],
    target: usize,
) -> Result<PlannerPrompt> {
    let window = |exact: bool| sendable(llm, exact);
    let limit = target.min(window(llm.counts_tokens_exactly()));

    let compact = HashSet::new();
    let base = base_tokens(llm, parts, catalog)?;
    let costs: Vec<usize> = ranked
        .iter()
        .map(|document| row_cost(llm, document, parts.summaries, false))
        .collect();

    let mut scale = 1.0_f64;
    let mut attempt = 0;
    let (mut documents, shown, mut tokens, mut exact, spent, shown_rows) = loop {
        let room = limit as f64 - base as f64 * scale;
        let (shown, spent) = take_rows(ranked, &costs, scale, room);
        let (user, documents) = render(parts, &shown, catalog, &compact)?;
        let (tokens, exact) = count(llm, &user).await;
        if tokens <= limit || shown.is_empty() || attempt == RECOUNTS {
            break (documents, shown.len(), tokens, exact, spent, shown);
        }
        // The estimate read low on this text. Choose again at the measured
        // rate, and never at a lower rate than last time.
        scale = (tokens as f64 / (base + spent).max(1) as f64).max(scale * 1.1);
        attempt += 1;
    };
    // Every document fit as a compact row and the budget has room left: show
    // rows whole, best first, while it lasts, so nothing is clipped from a
    // row the budget could have carried.
    if shown == ranked.len() && tokens <= limit {
        let mut extra = 0usize;
        let mut whole: HashSet<&str> = HashSet::new();
        for (document, compact_cost) in shown_rows.iter().zip(&costs) {
            let growth =
                row_cost(llm, document, parts.summaries, true).saturating_sub(*compact_cost);
            if growth == 0 {
                continue;
            }
            if (base + spent + extra + growth) as f64 * scale > limit as f64 {
                continue;
            }
            extra += growth;
            whole.insert(document.id.as_str());
        }
        if !whole.is_empty() {
            let (whole_user, whole_documents) = render(parts, &shown_rows, catalog, &whole)?;
            let (whole_tokens, whole_exact) = count(llm, &whole_user).await;
            if whole_tokens <= limit {
                documents = whole_documents;
                tokens = whole_tokens;
                exact = whole_exact;
            }
        }
    }
    if tokens > window(exact) {
        if shown > 0 {
            // Recounting could not bring the rows under the window; a plan from
            // the request alone is still better than none.
            let (user, documents) = render(parts, &[], catalog, &compact)?;
            let (tokens, exact) = count(llm, &user).await;
            if tokens <= window(exact) {
                return Ok(PlannerPrompt {
                    documents,
                    shown: 0,
                    tokens,
                    exact,
                    limit,
                });
            }
        }
        return Err(AppError::InvalidState(format!(
            "The retrieval planner's prompt needs {tokens} tokens and the utility model's window leaves {}",
            window(exact)
        )));
    }
    Ok(PlannerPrompt {
        documents,
        shown,
        tokens,
        exact,
        limit,
    })
}
