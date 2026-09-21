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
//! order. The current input and active constraints are retained; any omission
//! of unprocessed original messages marks the plan as requiring compaction
//! before generation.

pub mod budget;
pub mod plan;
pub mod render;

use crate::application::ports::llm_port::CompletionInput;
use crate::domain::conversation_memory::{MemoryItem, MemorySnapshot, SourceMessage};
use crate::shared::error::{AppError, Result};

pub use budget::{
    ActiveMemoryBudgetExceeded, BudgetAllocation, ModelCapacity, TokenAccounting,
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

/// Everything the assembler needs for one turn.
pub struct ContextRequest<'a> {
    pub conversation_id: &'a str,
    /// The real application/space/conversation policy. Budgeted as policy.
    pub system_policy: &'a str,
    /// The user's message for this turn, exactly as submitted.
    pub current_input: &'a str,
    /// Cost of the tool schemas actually being sent.
    pub tool_schema_tokens: usize,
    /// One consistent memory read. `None` when memory is unavailable or
    /// rebuilding: the turn still runs, it simply has no ledger.
    pub snapshot: Option<&'a MemorySnapshot>,
    /// Candidate recent messages, chronological, oldest first. The current
    /// input must not appear here — it is supplied separately and rendered once.
    pub recent: &'a [SourceMessage],
    /// Sequence through which extraction has processed. A recent message at or
    /// below this has a durable memory record; one above it does not, so
    /// dropping it would lose the only copy.
    pub processed_through_sequence: i64,
    /// Passages recalled from the original transcript, best first.
    pub recalled: Vec<SelectedPassage>,
    pub retrieval: RecallDiagnostics,
    /// Document and web evidence, any order; ranked for eviction.
    pub document_evidence: Vec<RankedEvidence>,
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
    /// - [`AppError::InvalidState`] carrying [`ActiveMemoryBudgetExceeded`] when
    ///   the genuinely active requirements do not fit even after borrowing.
    pub fn assemble(&self, request: &ContextRequest<'_>) -> Result<ContextPlan> {
        // --- 1. fixed costs -------------------------------------------------
        let policy = if request.system_policy.trim().is_empty() {
            render::MEMORY_USE_POLICY.to_string()
        } else {
            format!(
                "{}\n\n{}",
                request.system_policy.trim(),
                render::MEMORY_USE_POLICY
            )
        };
        let policy_message = CompletionInput::Message {
            role: "system".into(),
            content: policy,
        };
        let current_message = render::render_current_input(request.current_input);

        let policy_tokens = self.count_input(&policy_message);
        let current_tokens = self.count_input(&current_message);
        let fixed = policy_tokens
            .saturating_add(current_tokens)
            .saturating_add(request.tool_schema_tokens);

        let allocation = BudgetAllocation::plan(&request.capacity, fixed)?;
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
        let (mandatory_blocks, mandatory_items, unresolvable) = match request.snapshot {
            Some(snapshot) if snapshot.is_usable() => self.render_items(snapshot, request.recent),
            _ => (Vec::new(), Vec::new(), 0),
        };
        accounting.active_mandatory_count = mandatory_items.len();
        accounting.active_conflict_count = unresolvable
            + mandatory_items
                .iter()
                .filter(|item| {
                    item.review == crate::domain::conversation_memory::MemoryReview::Ambiguous
                })
                .count();

        let memory_message = render::render_memory_block(&mandatory_blocks);
        let memory_tokens = memory_message
            .as_ref()
            .map(|message| self.count_input(message))
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
                    count: mandatory_items.len(),
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

        // --- 5. working summary --------------------------------------------
        let summary_message = request
            .snapshot
            .filter(|snapshot| snapshot.is_usable())
            .and_then(|snapshot| snapshot.summary.as_deref())
            .and_then(render::render_summary);
        let mut summary_message = match summary_message {
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

        // --- 6. recent whole turns ----------------------------------------
        // Newest first so the most recent turn is the one that survives, then
        // restored to chronological order.
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

        // --- 7. recalled passages -----------------------------------------
        let mut recalled = Vec::new();
        let mut recall_tokens = 0usize;
        let recall_pool = allocation.recall.min(remaining);
        for passage in &request.recalled {
            let cost = self.count(&passage.text) + MESSAGE_FRAMING_TOKENS;
            if recall_tokens + cost > recall_pool {
                break;
            }
            recall_tokens += cost;
            recalled.push(passage.clone());
        }
        let mut retrieval = request.retrieval.clone();
        retrieval.passages_selected = recalled.len();
        retrieval.selected_message_ids = recalled
            .iter()
            .map(|passage| passage.message_id.clone())
            .collect();
        let mut recalled_message = render::render_recalled(&recalled);
        if let Some(message) = &recalled_message {
            let cost = self.count_input(message);
            if cost <= remaining {
                accounting.recall = cost;
                remaining -= cost;
            } else {
                accounting.evicted.push("recall".into());
                recalled_message = None;
                retrieval.passages_selected = 0;
                retrieval.selected_message_ids.clear();
            }
        }

        // --- 8. document and web evidence ----------------------------------
        let mut evidence: Vec<&RankedEvidence> = request.document_evidence.iter().collect();
        evidence.sort_by(|a, b| {
            b.rank
                .partial_cmp(&a.rank)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut evidence_blocks = Vec::new();
        let mut evidence_tokens = 0usize;
        let evidence_pool = allocation.rag_and_tools.min(remaining);
        for item in evidence {
            let block = format!("- {}\n{}", item.label, item.content);
            let cost = self.count(&block) + MESSAGE_FRAMING_TOKENS;
            if evidence_tokens + cost > evidence_pool {
                accounting.evicted.push(format!("evidence:{}", item.label));
                continue;
            }
            evidence_tokens += cost;
            evidence_blocks.push(block);
        }
        let evidence_message = (!evidence_blocks.is_empty()).then(|| CompletionInput::Message {
            role: "user".into(),
            content: format!(
                "[supporting material retrieved for this question]\n{}",
                evidence_blocks.join("\n")
            ),
        });
        if let Some(message) = &evidence_message {
            accounting.document_evidence = self.count_input(message);
        }

        // --- 9. render in precedence order (§10.4) -------------------------
        let mut messages = vec![policy_message];
        if let Some(message) = summary_message.take() {
            messages.push(message);
        }
        if let Some(message) = recalled_message {
            messages.push(message);
        }
        if let Some(message) = memory_message {
            messages.push(message);
        }
        if let Some(message) = evidence_message {
            messages.push(message);
        }
        let recent_start = messages.len();
        let recent_sequences: Vec<i64> = recent_messages
            .iter()
            .map(|(sequence, _)| *sequence)
            .collect();
        messages.extend(recent_messages.into_iter().map(|(_, message)| message));
        // Current input last, exactly once.
        messages.push(current_message);

        accounting.total_input = messages
            .iter()
            .map(|message| self.count_input(message))
            .sum::<usize>()
            .saturating_add(request.tool_schema_tokens);

        // Counting the serialized result can exceed the budget even after the
        // per-pool arithmetic said it would not, because framing estimates are
        // estimates. Optional material is evicted in rank order until it fits.
        let evicted_sequences = self.evict_until_fits(
            &mut messages,
            &mut accounting,
            recent_start,
            &recent_sequences,
        );
        let dropped_unprocessed = dropped_unprocessed
            || evicted_sequences
                .iter()
                .any(|sequence| *sequence > request.processed_through_sequence);

        Ok(ContextPlan {
            messages,
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
        })
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
    /// The order is fixed and deliberate: document evidence, then recalled
    /// passages, then the summary, then the oldest recent messages. Returned
    /// source sequences let the caller flag any unprocessed originals removed
    /// here. The system policy, active memory and current input are preserved.
    fn evict_until_fits(
        &self,
        messages: &mut Vec<CompletionInput>,
        accounting: &mut ContextAccounting,
        mut recent_start: usize,
        recent_sequences: &[i64],
    ) -> Vec<i64> {
        let mut evicted_sequences = Vec::new();
        let marker_of = |message: &CompletionInput| -> Option<&'static str> {
            let CompletionInput::Message { role, content } = message else {
                return None;
            };
            match role.as_str() {
                "user" if content.starts_with("[supporting material") => Some("evidence"),
                "user" if content.starts_with("[older passages") => Some("recall"),
                "assistant" if content.starts_with("[generated summary") => Some("summary"),
                _ => None,
            }
        };

        for marker in ["evidence", "recall", "summary"] {
            if accounting.fits() {
                return evicted_sequences;
            }
            if let Some(index) = messages
                .iter()
                .take(recent_start)
                .position(|message| marker_of(message) == Some(marker))
            {
                let removed = messages.remove(index);
                recent_start -= 1;
                let cost = self.count_input(&removed);
                accounting.total_input = accounting.total_input.saturating_sub(cost);
                match marker {
                    "evidence" => accounting.document_evidence = 0,
                    "recall" => accounting.recall = 0,
                    _ => accounting.summary = 0,
                }
                accounting.evicted.push(format!("overflow:{marker}"));
            }
        }

        // Track source identity through final eviction, too. This pass can
        // remove a message that survived the initial selection; the caller must
        // compact before sending if that original is above the watermark.
        // Keep the newest message and current input intact. Use positions rather
        // than content prefixes, which also occur in ordinary user messages.
        for sequence in recent_sequences
            .iter()
            .take(recent_sequences.len().saturating_sub(1))
        {
            if accounting.fits() {
                break;
            }
            let removed = messages.remove(recent_start);
            let cost = self.count_input(&removed);
            accounting.total_input = accounting.total_input.saturating_sub(cost);
            accounting.recent_history = accounting.recent_history.saturating_sub(cost);
            accounting.evicted.push("overflow:recent".into());
            evicted_sequences.push(*sequence);
        }
        evicted_sequences
    }
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
