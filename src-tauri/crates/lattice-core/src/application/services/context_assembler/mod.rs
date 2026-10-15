//! The single owner of what reaches the model.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §10.
//!
//! Before this existed, four places independently decided what to drop when a
//! prompt got large: `ContextWindowBuilder` scanned newest-first and stopped at
//! a budget, conversational QA truncated differently, HyDE concatenated the
//! whole completed history, and the tool loop grew the prompt with every tool
//! result. None of them knew about a recorded user constraint, so each of them
//! could drop one.
//!
//! Now they all call [`ContextAssembler::assemble`], and the order in which
//! things are given up is written down once, in [`ContextAssembler::assemble`]'s
//! eviction loop. The short version: optional material goes first, in rank
//! order. Evidence is optional even when the caller rendered it into the
//! current message ([`InlineEvidence`]): only the policy, the tool schemas and
//! the bare message are charged as fixed, so a window too small for all of a
//! turn's evidence carries the best of it rather than refusing the turn. The
//! current input and active constraints are retained; any omission of
//! unprocessed original messages marks the plan as requiring compaction before
//! generation.
//!
//! The rendered order is chosen for the server's prompt cache as much as for
//! the model: the system policy, then the summary when older turns are out of
//! the window, then the verbatim recent turns, then one user message carrying
//! everything that depends on this turn — recall, recorded requirements,
//! supporting material, the date and the request, in that order. Two turns of
//! the same conversation therefore share every message up to the newest
//! answer, and the server re-reads only what was added.

pub mod budget;
pub mod plan;
pub mod render;

use crate::application::ports::llm_port::CompletionInput;
use crate::domain::conversation::memory::{MemoryItem, MemorySnapshot, SourceMessage};
use crate::shared::error::{AppError, Result};

pub use budget::{
    ActiveMemoryBudgetExceeded, BudgetAllocation, BudgetError, BudgetRequest, EvidenceBudget,
    EvidenceShare, HistoryCharge, ModelCapacity, PromptBudgetExceeded, TokenAccounting,
    MIN_USABLE_CAPACITY,
};
pub use plan::{ContextAccounting, ContextPlan, RecallDiagnostics, SelectedPassage};
pub use render::MEMORY_USE_POLICY;

/// Counts tokens for the active continuation model.
pub type TokenCounter = std::sync::Arc<dyn Fn(&str) -> usize + Send + Sync>;

/// A ranked piece of optional evidence from documents or the web.
///
/// Ranked because eviction is by rank: when a request will not fit, the
/// lowest-ranked evidence goes first, and something has to define "lowest".
#[derive(Debug, Clone, PartialEq)]
pub struct RankedEvidence {
    pub label: String,
    pub content: String,
    /// Higher is better. Ties break on insertion order.
    pub rank: f32,
}

/// Evidence the caller renders into the current message itself.
///
/// The chat turn numbers its passages and wraps them in its own template, so
/// they reach the assembler inside the message rather than as
/// [`RankedEvidence`]. They are optional all the same. The request's
/// `current_input` is the message without them, and that is what the budget
/// charges as fixed; the evidence is charged after the history, and when the
/// request will not fit, the lowest-ranked items are given up first.
#[derive(Clone, Copy)]
pub struct InlineEvidence<'a> {
    /// How many items the message can carry, best first.
    pub items: usize,
    /// The message carrying the best `keep` items, for `keep` in
    /// `1..=items`. It is not called with zero: that message is
    /// `current_input`.
    pub render: &'a (dyn Fn(usize) -> String + Send + Sync),
}

/// Everything the assembler needs for one turn.
pub struct ContextRequest<'a> {
    pub conversation_id: &'a str,
    /// The real application/space/conversation policy. Budgeted as policy.
    pub system_policy: &'a str,
    /// The user's message for this turn, exactly as submitted. With
    /// `inline_evidence`, the message as it reads with none of that evidence.
    pub current_input: &'a str,
    /// Evidence rendered into the current message, when the caller has any.
    pub inline_evidence: Option<InlineEvidence<'a>>,
    /// Cost of the tool schemas actually being sent.
    pub tool_schema_tokens: usize,
    /// One consistent memory read. `None` when memory is unavailable or
    /// rebuilding: the turn still runs, it simply has no ledger.
    pub snapshot: Option<&'a MemorySnapshot>,
    /// Candidate recent messages, chronological, oldest first. The current
    /// input must not appear here — it is supplied separately and rendered once.
    pub recent: &'a [SourceMessage],
    /// True when `recent` begins at the conversation's first message. A window
    /// that keeps its oldest entry then carries the whole transcript, and the
    /// summary would only repeat it. `false` when unknown.
    pub recent_reaches_start: bool,
    /// Sequence through which extraction has processed. A recent message at or
    /// below this has a durable memory record; one above it does not, so
    /// dropping it would lose the only copy.
    pub processed_through_sequence: i64,
    /// Passages recalled from the original transcript, best first.
    pub recalled: Vec<SelectedPassage>,
    pub retrieval: RecallDiagnostics,
    /// Human-readable outcome of the automatic history search. Kept as a
    /// separate user-role note so a miss or unavailable index is not mistaken
    /// for evidence, and included in the same bounded recall allocation.
    pub recall_status: Option<&'a str>,
    /// Document and web evidence, any order; ranked for eviction.
    pub document_evidence: Vec<RankedEvidence>,
    /// The user's local date for the date line. Supplied by the caller so a
    /// plan stays a function of its request.
    pub today: chrono::NaiveDate,
    pub capacity: ModelCapacity,
}

/// Assembles bounded, typed model input against one budget.
pub struct ContextAssembler {
    count_tokens: TokenCounter,
}

impl ContextAssembler {
    pub fn new(count_tokens: TokenCounter) -> Self {
        Self { count_tokens }
    }

    fn count(&self, text: &str) -> usize {
        (self.count_tokens)(text)
    }

    /// The fixed parts of a request, priced as [`Self::assemble`] prices
    /// them: the policy with the memory rules appended, the tool schemas, and
    /// the current message under its date line, with nothing a plan may cut.
    ///
    /// A caller that sizes evidence before the request exists plans with
    /// this, so the room it hands out is the room assembly will find.
    pub fn fixed_parts(
        &self,
        system_policy: &str,
        current_input: &str,
        tool_schema_tokens: usize,
        today: chrono::NaiveDate,
    ) -> BudgetRequest {
        let (policy, last) = fixed_messages(system_policy, current_input, today);
        BudgetRequest::new(
            self.count_input(&policy),
            tool_schema_tokens,
            self.count_input(&last.tail_message()),
        )
    }

    fn count_input(&self, input: &CompletionInput) -> usize {
        match input {
            CompletionInput::Message { role, content } => {
                // Role names and the provider's per-message framing are real
                // tokens. Counting only content is how a prompt that "fits"
                // gets rejected.
                self.count(role) + self.count(content) + MESSAGE_FRAMING_TOKENS
            }
            CompletionInput::ToolCall {
                name, arguments, ..
            } => self.count(name) + self.count(&arguments.to_string()) + MESSAGE_FRAMING_TOKENS,
            CompletionInput::ToolResult { output, .. } => {
                self.count(output) + MESSAGE_FRAMING_TOKENS
            }
            CompletionInput::Native { value } => {
                self.count(&value.to_string()) + MESSAGE_FRAMING_TOKENS
            }
        }
    }

    /// Select and render one bounded request.
    ///
    /// # Errors
    ///
    /// - [`AppError::InvalidConfig`] when the model is too small to use.
    /// - [`AppError::InvalidInput`] when the policy, tools and current message
    ///   alone exceed the input budget — including the case where a single
    ///   pasted message is larger than the window. It is never clipped.
    ///   Evidence rendered into the message is not part of that check: it is
    ///   cut before anything is refused.
    /// - [`AppError::InvalidState`] carrying [`ActiveMemoryBudgetExceeded`] when
    ///   the genuinely active requirements do not fit even after borrowing.
    pub fn assemble(&self, request: &ContextRequest<'_>) -> Result<ContextPlan> {
        self.assemble_with_memory(request, None)
    }

    pub fn assemble_with_memory(
        &self,
        request: &ContextRequest<'_>,
        prepared: Option<&crate::application::ports::conversation_memory::PreparedMemory>,
    ) -> Result<ContextPlan> {
        // --- 1. fixed costs -------------------------------------------------
        // The date line and the request are the part of the final message no
        // eviction removes, so both are charged as fixed. Evidence rendered
        // into the request is not: it is added at step 8.
        let (policy_message, mut last) =
            fixed_messages(request.system_policy, request.current_input, request.today);

        let policy_tokens = self.count_input(&policy_message);
        let current_tokens = self.count_input(&last.tail_message());

        let allocation = BudgetAllocation::plan(
            &request.capacity,
            &BudgetRequest::new(policy_tokens, request.tool_schema_tokens, current_tokens),
        )?;
        let mut accounting =
            ContextAccounting::from_allocation(&allocation, &request.capacity.model_identity);
        accounting.fixed_policy = policy_tokens;
        accounting.current_input = current_tokens;
        accounting.tool_schemas = request.tool_schema_tokens;

        // --- 2. mandatory memory -------------------------------------------
        // Loaded whole and validated against live source. An item whose evidence
        // no longer resolves is not carried as fact: it is counted as a conflict
        // and left out, because the alternative is quoting text the user does
        // not have any more.
        let (mut mandatory_blocks, mandatory_items, unresolvable) = match request.snapshot {
            Some(snapshot) if snapshot.is_usable() => self.render_items(snapshot, request.recent),
            _ => (Vec::new(), Vec::new(), 0),
        };
        let mut used_memory_ids: Vec<String> = mandatory_items
            .iter()
            .map(|item| item.id.as_str().to_owned())
            .collect();
        if let Some(prepared) = prepared {
            mandatory_blocks = prepared.mandatory.clone();
            used_memory_ids = prepared
                .mandatory
                .iter()
                .filter_map(|text| prepared_memory_id(text))
                .collect();
        }
        accounting.active_mandatory_count = mandatory_blocks.len();
        accounting.active_conflict_count = if prepared.is_some() { 0 } else { unresolvable }
            + mandatory_items
                .iter()
                .filter(|item| {
                    item.review == crate::domain::conversation::memory::MemoryReview::Ambiguous
                })
                .count();

        let memory_section = render::render_memory_block(&mandatory_blocks);
        let memory_tokens = memory_section
            .as_deref()
            .map(|section| self.section_cost(section))
            .unwrap_or(0);

        // --- 3. reserve the newest complete turn ---------------------------
        let newest_turn_tokens = request
            .recent
            .last()
            .map(|message| self.count_input(&render::render_recent(message)))
            .unwrap_or(0);

        // --- 4. validate mandatory capacity, borrowing if needed ----------
        let ceiling = allocation.mandatory_ceiling_with_turn_reserved(newest_turn_tokens);
        if memory_tokens > allocation.mandatory_memory {
            if memory_tokens > ceiling {
                // A real finite-context limit. Not resolved by dropping the
                // oldest restriction or truncating a negation.
                return Err(ActiveMemoryBudgetExceeded {
                    count: mandatory_blocks.len(),
                    required: memory_tokens,
                    available: ceiling,
                }
                .into());
            }
            accounting.mandatory_borrowed = true;
        }
        accounting.active_memory = memory_tokens;

        // Room left for everything selectable after mandatory memory is paid.
        let mut remaining = allocation.available.saturating_sub(memory_tokens);

        // --- 5. recent whole turns ----------------------------------------
        // Chosen before the summary: the user's own turns outrank a model's
        // paraphrase of them, and whether the summary is needed at all depends
        // on how far back the window reaches. Newest first so the most recent
        // turn is the one that survives, then restored to chronological order.
        let mut recent_messages: Vec<(i64, CompletionInput)> = Vec::new();
        let mut recent_tokens = 0usize;
        let mut oldest_kept_sequence = i64::MAX;
        let recent_pool = allocation
            .recent_history
            .max(newest_turn_tokens)
            .min(remaining);
        for message in request.recent.iter().rev() {
            let rendered = render::render_recent(message);
            let cost = self.count_input(&rendered);
            if recent_tokens + cost > recent_pool && !recent_messages.is_empty() {
                break;
            }
            if recent_tokens + cost > remaining && !recent_messages.is_empty() {
                break;
            }
            recent_tokens += cost;
            oldest_kept_sequence = oldest_kept_sequence.min(message.sequence);
            recent_messages.push((message.sequence, rendered));
        }
        recent_messages.reverse();
        remaining = remaining.saturating_sub(recent_tokens);
        accounting.recent_history = recent_tokens;

        // Any eligible message above the watermark that did not fit exists
        // nowhere else: there is no memory record of it and no summary covering
        // it. Dropping it is the silent-truncation failure this design exists to
        // remove, so the turn asks for compaction instead.
        let dropped_unprocessed = request
            .recent
            .iter()
            .filter(|message| message.sequence > request.processed_through_sequence)
            .any(|message| {
                oldest_kept_sequence == i64::MAX || message.sequence < oldest_kept_sequence
            });

        // --- 6. working summary --------------------------------------------
        // Background consolidation refolds the summary after every answer, so
        // carrying it while the window still holds the whole transcript would
        // change the second message of every request — and with it the cached
        // prefix of everything after — to repeat text the model already has
        // verbatim. It is carried only once older turns have left the window.
        let window_reaches_start = request.recent_reaches_start
            && request
                .recent
                .first()
                .is_none_or(|first| oldest_kept_sequence == first.sequence);
        let summary_message = request
            .snapshot
            .filter(|snapshot| snapshot.is_usable() && !window_reaches_start)
            .and_then(|snapshot| snapshot.summary.as_deref())
            .and_then(render::render_summary);
        let summary_message = match summary_message {
            Some(message) => {
                let cost = self.count_input(&message);
                // Dropped whole rather than sliced: cutting a summary mid-clause
                // can invert a condition it was describing.
                if cost <= allocation.summary.min(remaining) {
                    accounting.summary = cost;
                    remaining -= cost;
                    Some(message)
                } else {
                    accounting.evicted.push("summary".into());
                    None
                }
            }
            None => None,
        };

        // --- 7. recalled passages -----------------------------------------
        // Recall was sized to its share when it was searched; here it may use
        // whatever the pools above left, ahead of any document. A passage too
        // long for what is left is passed over, not the end of recall: the
        // shorter ones after it still go in, and the ones left out are named
        // by message id so they can be read.
        let status_cost = render::render_recalled(&[], request.recall_status, &[])
            .map(|section| self.section_cost(&section))
            .unwrap_or(0);
        let passage_pool = remaining.saturating_sub(status_cost);
        // Indexes into `request.recalled`, so both lists stay in rank order.
        let mut kept: Vec<usize> = Vec::new();
        let mut left_out: Vec<usize> = Vec::new();
        let mut recall_tokens = 0usize;
        for (index, passage) in request.recalled.iter().enumerate() {
            let cost = self.count(&passage.text) + MESSAGE_FRAMING_TOKENS;
            if recall_tokens + cost > passage_pool {
                left_out.push(index);
                continue;
            }
            recall_tokens += cost;
            kept.push(index);
        }
        let render_recall = |kept: &[usize], left_out: &[usize]| {
            let passages: Vec<SelectedPassage> = kept
                .iter()
                .filter_map(|&index| request.recalled.get(index).cloned())
                .collect();
            let ids: Vec<&str> = left_out
                .iter()
                .filter_map(|&index| request.recalled.get(index))
                .map(|passage| passage.message_id.as_str())
                .collect();
            render::render_recalled(&passages, request.recall_status, &ids)
        };
        // The note naming what was left out costs room too: the lowest-ranked
        // passages give way to it until the section fits.
        let mut recalled_section = render_recall(&kept, &left_out);
        while let Some(section) = &recalled_section {
            let cost = self.section_cost(section);
            if cost <= remaining {
                accounting.recall = cost;
                remaining -= cost;
                break;
            }
            let Some(last) = kept.pop() else {
                accounting.evicted.push("recall".into());
                recalled_section = None;
                break;
            };
            left_out.push(last);
            left_out.sort_unstable();
            recalled_section = render_recall(&kept, &left_out);
        }
        if !left_out.is_empty() {
            accounting
                .evicted
                .push(format!("recall:{}", left_out.len()));
        }
        let recalled: Vec<SelectedPassage> = kept
            .iter()
            .filter_map(|&index| request.recalled.get(index).cloned())
            .collect();
        let mut retrieval = request.retrieval.clone();
        retrieval.passages_selected = recalled.len();
        retrieval.selected_message_ids = recalled
            .iter()
            .map(|passage| passage.message_id.clone())
            .collect();

        // --- 8. document and web evidence ----------------------------------
        // Evidence the caller rendered into the current message is carried
        // whole while the request fits, and has the first claim on what the
        // history left: it is what this turn answers from, and the saved
        // memory below is optional context. Being charged here rather than as
        // part of the message is what lets it give way, lowest-ranked first,
        // instead of refusing the turn.
        let inline = request.inline_evidence.filter(|inline| inline.items > 0);
        let mut inline_tokens = 0usize;
        if let Some(inline) = inline {
            last.set_input((inline.render)(inline.items));
            inline_tokens = self
                .count_input(&last.tail_message())
                .saturating_sub(current_tokens);
        }
        let optional: Vec<RankedEvidence> = prepared
            .into_iter()
            .flat_map(|p| &p.optional)
            .enumerate()
            .map(|(index, content)| RankedEvidence {
                label: SAVED_MEMORY_LABEL.into(),
                content: content.clone(),
                rank: 1.0 / (index + 1) as f32,
            })
            .collect();
        let mut optional_memory_ids = Vec::new();
        let mut evidence: Vec<&RankedEvidence> =
            request.document_evidence.iter().chain(&optional).collect();
        evidence.sort_by(|a, b| {
            b.rank
                .partial_cmp(&a.rank)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut evidence_blocks = Vec::new();
        let mut evidence_tokens = 0usize;
        // Everything the history left, not only the evidence share: room a
        // memory pool reserved and did not use (no summary yet, few active
        // facts, a short recall) goes to saved memories and ranked evidence.
        // Capped at the evidence share, a 32K model gave optional saved
        // memories about 3K tokens once the memory pools lost their fixed
        // ceilings, however little of those pools a turn used. The pools are
        // still charged first, and every recalled passage that fit was taken
        // above, so a document only ever has room no recalled passage could
        // use.
        let evidence_pool = remaining.saturating_sub(inline_tokens);
        for item in evidence {
            let block = format!("- {}\n{}", item.label, item.content);
            let cost = self.count(&block) + MESSAGE_FRAMING_TOKENS;
            if evidence_tokens + cost > evidence_pool {
                accounting.evicted.push(format!("evidence:{}", item.label));
                continue;
            }
            evidence_tokens += cost;
            if item.label == SAVED_MEMORY_LABEL {
                accounting.optional_selected_count += 1;
                optional_memory_ids.extend(prepared_memory_id(&item.content));
            }
            evidence_blocks.push(block);
        }
        let evidence_section = render::render_evidence(&evidence_blocks);
        if let Some(section) = &evidence_section {
            accounting.document_evidence = self.section_cost(section);
        }
        accounting.document_evidence += inline_tokens;
        accounting.inline_evidence_kept = inline.map_or(0, |inline| inline.items);

        // --- 9. render, query-dependent material last ----------------------
        last.recalled = recalled_section;
        last.memory = memory_section;
        last.evidence = evidence_section;
        let mut layout = Layout {
            policy: policy_message,
            summary: summary_message,
            recent: recent_messages,
            last,
            inline: inline.map(|inline| InlineCut {
                evidence: inline,
                kept: inline.items,
                bare: request.current_input,
            }),
        };
        accounting.total_input = self.measure(&layout, request.tool_schema_tokens);

        // Counting the serialized result can exceed the budget even after the
        // per-pool arithmetic said it would not, because framing estimates are
        // estimates. Optional material is evicted in rank order until it fits.
        let evicted_sequences =
            self.evict_until_fits(&mut layout, &mut accounting, request.tool_schema_tokens);
        // Overflow eviction drops the evidence section whole, and the optional
        // memory went with it.
        if layout.last.evidence.is_some() {
            used_memory_ids.extend(optional_memory_ids);
        }
        let dropped_unprocessed = dropped_unprocessed
            || evicted_sequences
                .iter()
                .any(|sequence| *sequence > request.processed_through_sequence);

        Ok(ContextPlan {
            messages: layout.into_messages(),
            memory_revision: request
                .snapshot
                .map(|snapshot| snapshot.state.memory_revision)
                .unwrap_or(0),
            transcript_revision: request
                .snapshot
                .map(|snapshot| snapshot.transcript_revision)
                .unwrap_or(0),
            accounting,
            retrieval,
            compaction_required: dropped_unprocessed,
            max_output_tokens: allocation.output_reserved,
            used_memory_ids,
        })
    }

    /// A section of the final message, priced as the message it would be on
    /// its own: the framing estimate also covers the separator and its header.
    fn section_cost(&self, section: &str) -> usize {
        self.count(section) + MESSAGE_FRAMING_TOKENS
    }

    /// The whole request as it would be sent, tool schemas included.
    fn measure(&self, layout: &Layout, tool_schema_tokens: usize) -> usize {
        let history: usize = layout
            .summary
            .iter()
            .chain(layout.recent.iter().map(|(_, message)| message))
            .map(|message| self.count_input(message))
            .sum();
        self.count_input(&layout.policy)
            .saturating_add(history)
            .saturating_add(self.count_input(&layout.last.render()))
            .saturating_add(tool_schema_tokens)
    }

    /// Render every mandatory item, resolving evidence against live source.
    ///
    /// Returns `(blocks, items_rendered, unresolvable_count)`.
    fn render_items(
        &self,
        snapshot: &MemorySnapshot,
        recent: &[SourceMessage],
    ) -> (Vec<String>, Vec<MemoryItem>, usize) {
        // Recent messages are already in hand; anything else has to come from
        // the snapshot's own evidence, which carries the digest it was recorded
        // against. A span with no live content available resolves to nothing,
        // which is the correct answer — not a cached quotation.
        let by_id: std::collections::HashMap<&str, &str> = recent
            .iter()
            .map(|message| (message.id.as_str(), message.content.as_str()))
            .collect();

        let mut blocks = Vec::new();
        let mut rendered = Vec::new();
        let mut unresolvable = 0usize;
        for item in snapshot.mandatory_items() {
            match render::render_memory_item(item, |message_id| by_id.get(message_id).copied()) {
                Some(block) => {
                    blocks.push(block);
                    rendered.push(item.clone());
                }
                None => unresolvable += 1,
            }
        }
        (blocks, rendered, unresolvable)
    }

    /// Drop optional material, lowest value first, until the request fits.
    ///
    /// The order is fixed and deliberate: document evidence, then the
    /// evidence rendered into the current message, lowest-ranked item first,
    /// then recalled passages, then the summary, then the oldest recent
    /// messages. Returned source sequences let the caller flag any unprocessed
    /// originals removed here. The system policy, active memory, the newest
    /// turn and the current input are preserved.
    fn evict_until_fits(
        &self,
        layout: &mut Layout,
        accounting: &mut ContextAccounting,
        tool_schema_tokens: usize,
    ) -> Vec<i64> {
        if !accounting.fits() {
            if let Some(section) = layout.last.evidence.take() {
                accounting.document_evidence = accounting
                    .document_evidence
                    .saturating_sub(self.section_cost(&section));
                accounting.evicted.push("overflow:evidence".into());
                accounting.total_input = self.measure(layout, tool_schema_tokens);
            }
        }
        // One item at a time, so a window that holds most of the turn's
        // evidence still carries most of it. Only the final message changes,
        // so only it is counted again.
        if let Some(cut) = layout.inline.as_mut() {
            let carried = cut.kept;
            let mut last_tokens = self.count_input(&layout.last.render());
            while !accounting.fits() && cut.kept > 0 {
                cut.kept -= 1;
                layout.last.set_input(if cut.kept == 0 {
                    cut.bare.to_owned()
                } else {
                    (cut.evidence.render)(cut.kept)
                });
                let tokens = self.count_input(&layout.last.render());
                accounting.total_input = accounting
                    .total_input
                    .saturating_sub(last_tokens)
                    .saturating_add(tokens);
                accounting.document_evidence = accounting
                    .document_evidence
                    .saturating_sub(last_tokens.saturating_sub(tokens));
                last_tokens = tokens;
            }
            if cut.kept < carried {
                accounting.evicted.push(format!(
                    "overflow:turn-evidence:{} of {}",
                    carried - cut.kept,
                    cut.evidence.items
                ));
            }
            accounting.inline_evidence_kept = cut.kept;
        }
        if !accounting.fits() && layout.last.recalled.take().is_some() {
            accounting.recall = 0;
            accounting.evicted.push("overflow:recall".into());
            accounting.total_input = self.measure(layout, tool_schema_tokens);
        }
        if !accounting.fits() && layout.summary.take().is_some() {
            accounting.summary = 0;
            accounting.evicted.push("overflow:summary".into());
            accounting.total_input = self.measure(layout, tool_schema_tokens);
        }

        // Track source identity through final eviction, too. This pass can
        // remove a message that survived the initial selection; the caller must
        // compact before sending if that original is above the watermark.
        let mut evicted_sequences = Vec::new();
        while !accounting.fits() && layout.recent.len() > 1 {
            let (sequence, removed) = layout.recent.remove(0);
            let cost = self.count_input(&removed);
            accounting.total_input = accounting.total_input.saturating_sub(cost);
            accounting.recent_history = accounting.recent_history.saturating_sub(cost);
            accounting.evicted.push("overflow:recent".into());
            evicted_sequences.push(sequence);
        }
        evicted_sequences
    }
}

/// A plan's messages before they are flattened, kept in parts so overflow
/// eviction removes material by what it is rather than by matching text a
/// user could also have typed.
struct Layout<'a> {
    policy: CompletionInput,
    summary: Option<CompletionInput>,
    /// Verbatim turns, oldest first, beside their source sequence.
    recent: Vec<(i64, CompletionInput)>,
    last: render::FinalUserMessage,
    /// Evidence rendered into the current message, when there is any.
    inline: Option<InlineCut<'a>>,
}

/// The current message's own evidence, and how much of it the request still
/// carries.
struct InlineCut<'a> {
    evidence: InlineEvidence<'a>,
    kept: usize,
    /// The message with none of it.
    bare: &'a str,
}

/// The system policy and the final message as they stand before anything
/// optional is chosen: the policy with the memory rules appended, and the
/// current input under its date line.
fn fixed_messages(
    system_policy: &str,
    current_input: &str,
    today: chrono::NaiveDate,
) -> (CompletionInput, render::FinalUserMessage) {
    let policy = if system_policy.trim().is_empty() {
        render::MEMORY_USE_POLICY.to_string()
    } else {
        format!("{}\n\n{}", system_policy.trim(), render::MEMORY_USE_POLICY)
    };
    (
        CompletionInput::Message {
            role: "system".into(),
            content: policy,
        },
        render::FinalUserMessage::new(current_input, today),
    )
}

impl Layout<'_> {
    fn into_messages(self) -> Vec<CompletionInput> {
        let mut messages = Vec::with_capacity(self.recent.len() + 3);
        messages.push(self.policy);
        messages.extend(self.summary);
        messages.extend(self.recent.into_iter().map(|(_, message)| message));
        messages.push(self.last.render());
        messages
    }
}

/// Label of a prepared optional memory item among the evidence.
const SAVED_MEMORY_LABEL: &str = "Saved memory with original evidence";

/// The id of one prepared memory item.
///
/// Prepared items arrive as the JSON object the prompt carries, `memory_id`
/// included; reading the id from that object is reading the item, not parsing
/// rendered prose.
fn prepared_memory_id(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .get("memory_id")?
        .as_str()
        .map(str::to_owned)
}

/// Per-message provider framing, in tokens.
///
/// Every provider spends something on role delimiters and message boundaries.
/// The exact number is provider-specific and not exposed; four is deliberately
/// a small over-estimate, and the safety margin covers the rest. Counting zero
/// here is what makes a prompt that "fits" get rejected by the API.
pub const MESSAGE_FRAMING_TOKENS: usize = 4;

/// A token counter that estimates ~1.3 tokens per whitespace-separated word.
///
/// For callers with no provider tokenizer. Plans built with it must report
/// [`TokenAccounting::Estimated`], because they are.
pub fn estimating_counter() -> TokenCounter {
    std::sync::Arc::new(|text: &str| {
        let words = text.split_whitespace().count();
        (words as f64 * 1.3) as usize
    })
}

/// Re-exported for callers that need to name the error type directly.
pub type AssembleError = AppError;

#[cfg(test)]
mod tests;
