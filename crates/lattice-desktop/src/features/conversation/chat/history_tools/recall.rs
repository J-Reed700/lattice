//! Automatic recall before generation, and the queries it searches with.
use super::{
    clip_to_bytes, HistoryToolScope, ADJACENT_TURNS, DEFAULT_RESPONSE_BYTES, MAX_CANDIDATES,
    MAX_CONTEXT_TOKENS, MAX_EXACT_TERMS,
};
use lattice_core::application::ports::conversation_memory::{
    ConversationMemoryReadPort, RecallCandidate, SourceReadLimits,
};
use lattice_core::application::services::context_assembler::{RecallDiagnostics, SelectedPassage};
use lattice_core::domain::conversation::memory::SourceMessage;
use lattice_core::shared::error::Result;

// ---------------------------------------------------------------------------
// Automatic per-turn retrieval
// ---------------------------------------------------------------------------

/// What automatic retrieval is given about the turn.
#[derive(Debug, Default)]
pub struct RecallRequest<'a> {
    /// The user's message for this turn, exactly as submitted.
    pub user_input: &'a str,
    /// Recent turn text already in the prompt, oldest first. Used both to
    /// resolve pronouns in the query and to avoid re-sending what is already
    /// there.
    pub recent_turns: &'a [String],
    /// Active memory text already in the prompt, for the same exclusion.
    pub active_memory: &'a [String],
    /// Byte ceiling on the packed passages.
    pub max_bytes: usize,
}

/// One candidate and the turns around it, which is the unit that explains a
/// short reply: "yes, option B" is only an answer next to its question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassageGroup {
    /// Chronological within the group (§9.1 step 7). Always exact source text.
    pub passages: Vec<SelectedPassage>,
    /// Ids this group explains but does not repeat, because the prompt already
    /// carries them verbatim — from a recent turn, from active memory, or from
    /// an earlier group of this same recall (§9.1 step 8).
    pub already_in_context: Vec<String>,
    /// Sequence of the candidate this group was built around.
    pub anchor_sequence: i64,
    /// Order this group was selected in, best first. The model reads groups
    /// chronologically, but `ContextAssembler` evicts from the tail, so the rank is
    /// kept to drop the weakest group rather than the newest.
    pub selection_rank: usize,
    /// True when the anchor came from exact-identifier matching rather than
    /// ranking, which is the case where lexical rank is least trustworthy.
    pub from_exact_identifier: bool,
}

/// Whether this retrieval ran, and what that means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallAvailability {
    /// Ran and selected passages.
    Selected,
    /// Ran and matched nothing. Not evidence that it was never said.
    NotFoundByRetrieval,
    /// Could not run. `diagnostics.index_error` carries the code.
    Unavailable,
}

impl RecallAvailability {
    /// The flag §9.3 requires beside automatically retrieved context, in the
    /// words the model sees. It never tells the model to call a tool.
    pub fn note(self) -> &'static str {
        match self {
            Self::Selected => {
                "Automatic retrieval found older passages in this conversation. Any passages included below are exact original text, with ids and roles."
            }
            Self::NotFoundByRetrieval => {
                "A search of this conversation's older messages found nothing for this turn. \
                 That is not evidence the user never said it; ask rather than assert."
            }
            Self::Unavailable => {
                "This conversation's older messages could not be searched for this turn, so \
                 nothing here rules out something having been said earlier."
            }
        }
    }
}

/// Packed passage groups and what it took to get them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecalledContext {
    pub groups: Vec<PassageGroup>,
    pub diagnostics: RecallDiagnostics,
}

impl RecalledContext {
    /// Flattened for `ContextRequest::recalled`: groups in selection order,
    /// best first, and chronological inside each group. Worst-last is what makes
    /// the assembler's tail eviction drop the weakest group rather than the
    /// newest one.
    pub fn passages(&self) -> Vec<SelectedPassage> {
        let mut ordered: Vec<&PassageGroup> = self.groups.iter().collect();
        ordered.sort_by_key(|group| group.selection_rank);
        ordered
            .into_iter()
            .flat_map(|group| group.passages.iter().cloned())
            .collect()
    }

    pub fn availability(&self) -> RecallAvailability {
        if self.diagnostics.index_error.is_some() && self.groups.is_empty() {
            RecallAvailability::Unavailable
        } else if self.groups.is_empty() {
            RecallAvailability::NotFoundByRetrieval
        } else {
            RecallAvailability::Selected
        }
    }
}

/// Retrieval for one turn, for a path that has no tools to offer the model.
///
/// Implements §9.1 steps 1–8. The conversation comes from `scope`, never from
/// anything the model produced, so this shares the tools' scope guarantee.
///
/// # Errors
///
/// Returns `Err` only if the port fails outside its own degradation path. A
/// missing or broken index is reported through
/// [`RecallDiagnostics::index_error`] and leaves the turn otherwise intact.
pub async fn recall_for_turn(
    port: &dyn ConversationMemoryReadPort,
    scope: &HistoryToolScope,
    request: &RecallRequest<'_>,
) -> Result<RecalledContext> {
    let mut diagnostics = RecallDiagnostics::default();
    let query = build_recall_query(request.user_input, request.recent_turns);
    let exact_terms = exact_identifiers(request.user_input, request.recent_turns);
    if query.trim().is_empty() && exact_terms.is_empty() {
        return Ok(RecalledContext {
            groups: Vec::new(),
            diagnostics,
        });
    }
    if scope.budget.expired() {
        // Out of time is not the same as out of matches.
        diagnostics.index_error = Some("turn_budget_exhausted".to_string());
        return Ok(RecalledContext {
            groups: Vec::new(),
            diagnostics,
        });
    }

    // The message being answered is saved before recall runs; it is already the
    // prompt's question, so it is not recalled as well.
    let mut already: Vec<&str> = vec![request.user_input];
    already.extend(request.recent_turns.iter().map(String::as_str));
    already.extend(request.active_memory.iter().map(String::as_str));

    let max_bytes = if request.max_bytes == 0 {
        DEFAULT_RESPONSE_BYTES
    } else {
        request.max_bytes
    };
    let mut remaining = max_bytes;
    let mut groups: Vec<PassageGroup> = Vec::new();
    let mut claimed: Vec<String> = Vec::new();
    let mut considered: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Groups fill the byte budget, which the turn sizes from its recall pool,
    // rather than stopping at a fixed count. Candidates are searched a page at
    // a time, each page twice the last, while the budget has room and the
    // search may hold more than it returned.
    let mut limit = MAX_CANDIDATES;
    loop {
        let found = port
            .search_source_messages(scope.conversation_id(), &query, &exact_terms, limit)
            .await?;
        diagnostics.lexical_ran |= found.lexical_ran;
        diagnostics.semantic_ran |= found.semantic_ran;
        diagnostics.index_error = found.index_error.clone();
        let exhausted = found.candidates.len() < limit;
        let fresh: Vec<&RecallCandidate> = ranked(&found.candidates)
            .into_iter()
            .filter(|candidate| considered.insert(candidate.message_id.clone()))
            .collect();
        let had_fresh = !fresh.is_empty();
        for candidate in fresh {
            if remaining == 0 {
                break;
            }
            if let Some(group) = pack_group(
                port,
                scope,
                candidate,
                &already,
                &mut claimed,
                &mut remaining,
                max_bytes,
                groups.len(),
            )
            .await?
            {
                groups.push(group);
            }
        }
        if remaining == 0 || exhausted || !had_fresh {
            break;
        }
        limit = limit.saturating_mul(2);
    }
    diagnostics.candidates_considered = considered.len();

    // Selection is by rank so the budget drops the weakest group, but the model
    // reads the thread in the order it happened, so emission is chronological.
    groups.sort_by_key(|group| group.anchor_sequence);
    diagnostics.passages_selected = groups.iter().map(|group| group.passages.len()).sum();
    diagnostics.selected_message_ids = groups
        .iter()
        .flat_map(|group| group.passages.iter().map(|p| p.message_id.clone()))
        .collect();

    Ok(RecalledContext {
        groups,
        diagnostics,
    })
}

/// One candidate and its neighbours, packed into what is left of the budget.
/// `None` when the candidate is already claimed or everything in its window
/// is already in front of the model.
///
/// A passage goes in whole or not at all, so a later, shorter group can still
/// use the room. The one exception is a passage too long for the whole recall
/// budget that would otherwise leave recall empty: it is cut to the budget,
/// logged, and marked in its text so the model knows it reads a part.
#[allow(clippy::too_many_arguments)]
async fn pack_group(
    port: &dyn ConversationMemoryReadPort,
    scope: &HistoryToolScope,
    candidate: &RecallCandidate,
    already: &[&str],
    claimed: &mut Vec<String>,
    remaining: &mut usize,
    max_bytes: usize,
    selection_rank: usize,
) -> Result<Option<PassageGroup>> {
    if claimed.contains(&candidate.message_id) {
        // Already inside a neighbouring group; a second copy of it would
        // spend budget to say the same thing twice.
        return Ok(None);
    }
    let window = port
        .read_adjacent_turns(
            scope.conversation_id(),
            candidate.sequence,
            ADJACENT_TURNS,
            ADJACENT_TURNS,
            SourceReadLimits::new(1 + 2 * ADJACENT_TURNS, SourceReadLimits::DEFAULT.max_bytes),
        )
        .await?;
    let window: Vec<&SourceMessage> = window
        .iter()
        .filter(|message| message.is_eligible_source())
        .collect();
    if window.is_empty() {
        return Ok(None);
    }

    let mut group = PassageGroup {
        passages: Vec::new(),
        already_in_context: Vec::new(),
        anchor_sequence: candidate.sequence,
        selection_rank,
        from_exact_identifier: candidate.exact_identifier,
    };
    for message in &window {
        let in_prompt = already
            .iter()
            .any(|present| present.contains(message.content.as_str()));
        // Adjacent windows overlap, so a neighbour can belong to a group
        // that was already packed. Paying for it twice buys nothing and
        // takes the budget from a group that has something new to say.
        if in_prompt || claimed.contains(&message.id) {
            // §9.1 step 8: named so the group still explains the link,
            // without paying for text the prompt already carries.
            group.already_in_context.push(message.id.clone());
            claimed.push(message.id.clone());
            continue;
        }
        let text = if message.content.len() <= *remaining {
            message.content.clone()
        } else if *remaining == max_bytes && selection_rank == 0 && group.passages.is_empty() {
            let (part, _) =
                clip_to_bytes(&message.content, max_bytes.saturating_sub(CUT_MARK_BYTES));
            if part.is_empty() {
                break;
            }
            tracing::warn!(
                message_id = %message.id,
                bytes = message.content.len(),
                budget = max_bytes,
                "A recalled message is longer than the whole recall budget; carrying its opening"
            );
            format!("{part}{}", cut_mark(message.content.len() - part.len()))
        } else {
            // Does not fit what is left: leave the room to a shorter group.
            break;
        };
        *remaining = remaining.saturating_sub(text.len());
        claimed.push(message.id.clone());
        group.passages.push(SelectedPassage {
            message_id: message.id.clone(),
            sequence: message.sequence,
            role: message.role,
            text,
        });
        if *remaining == 0 {
            break;
        }
    }
    Ok((!group.passages.is_empty()).then_some(group))
}

/// Room kept for the mark on a cut passage.
const CUT_MARK_BYTES: usize = 96;

/// The mark on a passage cut to the recall budget.
fn cut_mark(left_out_bytes: usize) -> String {
    format!(" […cut: {left_out_bytes} more bytes of this message are not shown]")
}

/// Candidates in selection order: exact-identifier hits first, then BM25 rank
/// (more negative is better), with sequence breaking ties so the order is
/// stable across runs.
fn ranked(candidates: &[RecallCandidate]) -> Vec<&RecallCandidate> {
    let mut ordered: Vec<&RecallCandidate> = candidates.iter().collect();
    ordered.sort_by(|a, b| {
        b.exact_identifier
            .cmp(&a.exact_identifier)
            .then(
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.sequence.cmp(&b.sequence))
    });
    ordered
}

// ---------------------------------------------------------------------------
// Query formation
// ---------------------------------------------------------------------------

/// The user's input plus a bounded tail of recent-turn context (§9.1 step 1).
///
/// The tail exists for pronouns: "does that still hold?" has nothing in it to
/// match on. It is capped in tokens rather than messages because one pasted
/// message would otherwise become the entire query.
pub(super) fn build_recall_query(user_input: &str, recent_turns: &[String]) -> String {
    let mut query = String::from(user_input.trim());
    let mut budget = MAX_CONTEXT_TOKENS;
    // Newest first: the nearest turn is the one a pronoun refers to.
    for turn in recent_turns.iter().rev() {
        if budget == 0 {
            break;
        }
        let taken: Vec<&str> = turn.split_whitespace().take(budget).collect();
        if taken.is_empty() {
            continue;
        }
        budget -= taken.len();
        query.push(' ');
        query.push_str(&taken.join(" "));
    }
    query
}

/// Exact identifiers worth probing as substrings rather than trusting to rank:
/// paths, version numbers, quoted phrases, numbers, and tokens carrying enough
/// punctuation to be a name rather than a word.
///
/// These are held separately from the lexical query because truncating that
/// query must never be able to discard one (§9.1 step 1), and because
/// `instr` is case-sensitive: `AuthToken` and `authtoken` are different names.
pub(super) fn exact_identifiers(user_input: &str, recent_turns: &[String]) -> Vec<String> {
    fn push(candidate: &str, found: &mut Vec<String>) {
        let candidate = candidate.trim();
        if candidate.is_empty()
            || candidate.chars().count() < 2
            || found.len() >= MAX_EXACT_TERMS
            || found.iter().any(|existing| existing == candidate)
        {
            return;
        }
        found.push(candidate.to_string());
    }

    let mut found: Vec<String> = Vec::new();

    for source in std::iter::once(user_input).chain(recent_turns.iter().rev().map(String::as_str)) {
        for quoted in quoted_spans(source) {
            push(quoted, &mut found);
        }
        for token in source.split_whitespace() {
            // Sentence punctuation is not part of a name, but a trailing `/` or
            // `)` inside one is, so only the outermost separators come off.
            let token =
                token.trim_matches(|c: char| matches!(c, ',' | ';' | '"' | '\'' | '(' | ')'));
            let token = token.trim_end_matches(['.', '?', '!', ':']);
            if is_exact_identifier(token) {
                push(token, &mut found);
            }
        }
        if found.len() >= MAX_EXACT_TERMS {
            break;
        }
    }
    found
}

/// Text inside matching double or single quotes. A quoted phrase is the
/// strongest possible signal that the user means these exact words.
fn quoted_spans(text: &str) -> Vec<&str> {
    let mut spans = Vec::new();
    for quote in ['"', '\''] {
        let mut rest = text;
        while let Some(open) = rest.find(quote) {
            let after = &rest[open + quote.len_utf8()..];
            let Some(close) = after.find(quote) else {
                break;
            };
            spans.push(&after[..close]);
            rest = &after[close + quote.len_utf8()..];
        }
    }
    spans
}

/// Whether a token is a name rather than a word.
fn is_exact_identifier(token: &str) -> bool {
    if token.is_empty() || token.chars().count() > 128 {
        return false;
    }
    let alphanumeric = token.chars().filter(|c| c.is_alphanumeric()).count();
    if alphanumeric == 0 {
        return false;
    }
    let punctuation = token
        .chars()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count();
    // Paths and namespaced names.
    if token.contains('/') || token.contains('\\') || token.contains("::") {
        return true;
    }
    // A digit next to a separator: versions, error codes, ids, prices.
    if token.chars().any(|c| c.is_ascii_digit()) && (punctuation > 0 || alphanumeric >= 2) {
        return true;
    }
    // Scripts the FTS index cannot segment. `conversation_search_fts` is
    // tokenized `unicode61`, which splits on separators; Japanese, Chinese,
    // Korean and Thai are written without them, so a whole clause arrives as one
    // token and a lexical query matches only when it is byte-identical. Probing
    // these as substrings is the only way a query in them reaches its passage.
    if token.chars().filter(|c| is_unsegmented_script(*c)).count() >= 2 {
        return true;
    }
    // Two or more separators inside one token is not prose.
    punctuation >= 2
}

/// Whether `c` belongs to a script written without word separators.
fn is_unsegmented_script(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}'      // Hiragana and Katakana
        | '\u{3400}'..='\u{4DBF}'    // CJK Extension A
        | '\u{4E00}'..='\u{9FFF}'    // CJK Unified Ideographs
        | '\u{AC00}'..='\u{D7AF}'    // Hangul syllables
        | '\u{0E00}'..='\u{0E7F}'    // Thai
        | '\u{F900}'..='\u{FAFF}'    // CJK compatibility ideographs
    )
}
