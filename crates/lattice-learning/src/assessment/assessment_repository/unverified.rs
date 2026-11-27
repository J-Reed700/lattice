//! Guards quiz-bank authoring and quiz draws against lesson claims that are
//! unverified (`no_source`, `unsupported`, `contradicted`, or still unsettled).
use crate::content_verification::{
    is_claim_active_unverified, report_unverified_block_quotes, report_unverified_question_indices,
    LessonVerificationReport,
};
use crate::lessons::claim_reviews::repository::load_in;
use crate::persistence::decode;
use lattice_core::domain::learning::claim_review::{ClaimLedgerEntry, ClaimState, UnitKind};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;
use lattice_core::shared::text::same_text::overlaps;
use lattice_core::shared::text::segmentation::{code_fences, sentences};
use sqlx::{Row, SqliteConnection};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
struct UnverifiedClaim<'a> {
    span: Option<(usize, usize)>,
    quote: &'a str,
}

fn strip_unverified_sentences(body: &str, claims: &[UnverifiedClaim<'_>]) -> String {
    let fences = code_fences(body);
    if (claims.is_empty() && fences.is_empty()) || body.trim().is_empty() {
        return body.to_owned();
    }
    let sentence_spans = sentences(body);
    if sentence_spans.is_empty() {
        if !fences.is_empty() {
            return String::new();
        }
        let covered = claims
            .iter()
            .any(|c| c.span.is_some() || quote_matches_sentence(body, c.quote));
        return if covered {
            String::new()
        } else {
            body.to_owned()
        };
    }

    let mut out = String::new();
    let mut prev_end: Option<usize> = None;
    let mut skipped_since_kept = false;
    for (start, end) in sentence_spans {
        let Some(text) = body.get(start..end) else {
            continue;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let in_fence = fences
            .iter()
            .any(|fence| overlaps((start, end), (fence.whole().start, fence.whole().end)));
        let blocked = in_fence
            || claims.iter().any(|claim| {
                let span = claim
                    .span
                    .and_then(|s| valid_claim_span(body, s, claim.quote));
                span.is_some_and(|s| overlaps((start, end), s))
                    || quote_matches_sentence(trimmed, claim.quote)
            });
        if blocked {
            skipped_since_kept = true;
            continue;
        }
        if let Some(last_end) = prev_end {
            let fence_between = fences
                .iter()
                .any(|fence| overlaps((last_end, start), (fence.whole().start, fence.whole().end)));
            if skipped_since_kept || fence_between {
                out.push_str("\n\n\n");
            } else if let Some(between) = body.get(last_end..start) {
                out.push_str(between);
            } else {
                out.push(' ');
            }
        }
        out.push_str(text);
        prev_end = Some(end);
        skipped_since_kept = false;
    }
    out
}

/// Filters `learning_blocks` bodies for `lesson_id`, removing every sentence
/// whose verification claim is `no_source`/unverified or unsettled.
pub(super) async fn verified_block_bodies_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    blocks: &[(usize, String)],
) -> Result<Vec<String>> {
    Box::pin(verified_block_bodies_inner(connection, lesson_id, blocks)).await
}

#[derive(Default, Clone, Copy)]
struct BlockClaimStats {
    unverified: usize,
    verified: usize,
    consistent: usize,
}

impl BlockClaimStats {
    fn is_ungrounded_dominated(self) -> bool {
        self.unverified > self.verified
            && (self.verified + self.consistent >= 1 || self.unverified >= 2)
    }
}

fn filter_blocks_with_report(raw: &str, blocks: &[(usize, String)]) -> Option<Vec<String>> {
    let report = decode::<LessonVerificationReport>(raw).ok()?;
    let by_block = report_unverified_block_quotes(&report, blocks.len());
    let mut stats: HashMap<usize, BlockClaimStats> = HashMap::new();
    for finding in &report.findings {
        if finding.unit < blocks.len() {
            let entry = stats.entry(finding.unit).or_default();
            if !finding.verdict.is_supported() {
                entry.unverified += 1;
            } else {
                entry.verified += 1;
            }
        }
    }
    let filtered: Vec<String> = blocks
        .iter()
        .map(|(ordinal, body)| match by_block.get(ordinal) {
            Some(_)
                if stats
                    .get(ordinal)
                    .copied()
                    .unwrap_or_default()
                    .is_ungrounded_dominated() =>
            {
                String::new()
            }
            Some(quotes) => {
                let claims: Vec<UnverifiedClaim<'_>> = quotes
                    .iter()
                    .map(|quote| UnverifiedClaim { span: None, quote })
                    .collect();
                strip_unverified_sentences(body, &claims)
            }
            None => strip_unverified_sentences(body, &[]),
        })
        .collect();
    if filtered.iter().any(|body| !body.trim().is_empty()) {
        return Some(filtered);
    }
    Some(
        blocks
            .iter()
            .map(|(ordinal, body)| match by_block.get(ordinal) {
                Some(quotes) => {
                    let claims: Vec<UnverifiedClaim<'_>> = quotes
                        .iter()
                        .map(|quote| UnverifiedClaim { span: None, quote })
                        .collect();
                    strip_unverified_sentences(body, &claims)
                }
                None => strip_unverified_sentences(body, &[]),
            })
            .collect(),
    )
}

fn collect_block_claim_stats(
    claims: &[ClaimLedgerEntry],
    block_count: usize,
    by_id: &HashMap<&str, &ClaimState>,
) -> HashMap<usize, BlockClaimStats> {
    let mut stats: HashMap<usize, BlockClaimStats> = HashMap::new();
    for entry in claims {
        let loc = entry.location();
        if loc.unit_kind == UnitKind::Block && loc.field == "body" && loc.unit_index < block_count {
            let counts = stats.entry(loc.unit_index).or_default();
            if is_claim_active_unverified(entry.state(), by_id) {
                counts.unverified += 1;
            } else {
                match entry.state() {
                    ClaimState::ResolvedByEvidence { .. }
                    | ClaimState::GeneralKnowledge { .. }
                    | ClaimState::Confirmed { .. } => counts.verified += 1,
                    ClaimState::Consistent { .. } => counts.consistent += 1,
                    _ => {}
                }
            }
        }
    }
    stats
}

async fn verified_block_bodies_inner(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    blocks: &[(usize, String)],
) -> Result<Vec<String>> {
    let ledger = Box::pin(load_in(connection, lesson_id)).await?;
    if !ledger.claims.is_empty() {
        let by_id: HashMap<&str, &ClaimState> = ledger
            .claims
            .iter()
            .map(|entry| (entry.claim_id().0.as_str(), entry.state()))
            .collect();
        let mut by_block: HashMap<usize, Vec<UnverifiedClaim<'_>>> = HashMap::new();
        for entry in &ledger.claims {
            let loc = entry.location();
            if loc.unit_kind == UnitKind::Block
                && loc.field == "body"
                && is_claim_active_unverified(entry.state(), &by_id)
            {
                by_block
                    .entry(loc.unit_index)
                    .or_default()
                    .push(UnverifiedClaim {
                        span: loc.span,
                        quote: &loc.quote,
                    });
            }
        }
        let stats = collect_block_claim_stats(&ledger.claims, blocks.len(), &by_id);
        let filtered: Vec<String> = blocks
            .iter()
            .map(|(ordinal, body)| match by_block.get(ordinal) {
                Some(_)
                    if stats
                        .get(ordinal)
                        .copied()
                        .unwrap_or_default()
                        .is_ungrounded_dominated() =>
                {
                    String::new()
                }
                Some(claims) => strip_unverified_sentences(body, claims),
                None => strip_unverified_sentences(body, &[]),
            })
            .collect();
        if filtered.iter().any(|body| !body.trim().is_empty()) {
            return Ok(filtered);
        }
        return Ok(blocks
            .iter()
            .map(|(ordinal, body)| match by_block.get(ordinal) {
                Some(claims) => strip_unverified_sentences(body, claims),
                None => strip_unverified_sentences(body, &[]),
            })
            .collect());
    }

    let report_json: Option<String> = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE lesson_id=? ORDER BY checked_at DESC LIMIT 1",
    )
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await?;
    if let Some(raw) = report_json {
        if let Some(filtered) = filter_blocks_with_report(&raw, blocks) {
            return Ok(filtered);
        }
    }

    Ok(blocks
        .iter()
        .map(|(_, body)| strip_unverified_sentences(body, &[]))
        .collect())
}

/// Candidate IDs in `program_id` that depend on unverified/`no_source` claims:
/// - lesson seed questions whose unit (`blocks.len() + question_ordinal`) holds
///   any active unverified/unsettled claim;
/// - bank items whose `source_quote` matches any active unverified block quote
///   in their lesson.
pub(super) async fn unverified_candidates_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<HashSet<String>> {
    Box::pin(unverified_candidates_inner(connection, program_id)).await
}

fn collect_report_unverified(
    raw: &str,
    lesson_id: &str,
    block_count: usize,
    unverified_questions: &mut HashSet<(String, usize)>,
    unverified_quotes: &mut HashMap<String, Vec<String>>,
) {
    let Ok(report) = decode::<LessonVerificationReport>(raw) else {
        return;
    };
    for index in report_unverified_question_indices(&report, block_count) {
        unverified_questions.insert((lesson_id.to_owned(), index));
    }
    for quotes in report_unverified_block_quotes(&report, block_count).into_values() {
        for quote in quotes {
            if !quote.trim().is_empty() {
                unverified_quotes
                    .entry(lesson_id.to_owned())
                    .or_default()
                    .push(quote.to_owned());
            }
        }
    }
}

async fn unverified_candidates_inner(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<HashSet<String>> {
    let block_counts: HashMap<String, usize> = sqlx::query_as::<_, (String, i64)>(
        "SELECT b.lesson_id, COUNT(*) FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=? GROUP BY b.lesson_id",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .filter_map(|(lesson_id, count)| usize::try_from(count).ok().map(|n| (lesson_id, n)))
    .collect();

    let lesson_ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM learning_lessons WHERE program_id=?")
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await?;

    let mut unverified_questions: HashSet<(String, usize)> = HashSet::new();
    let mut unverified_quotes: HashMap<String, Vec<String>> = HashMap::new();

    for lesson_id in lesson_ids {
        let block_count = block_counts.get(&lesson_id).copied().unwrap_or(0);
        let ledger = Box::pin(load_in(connection, &lesson_id)).await?;
        let poisoned_blocks = if !ledger.claims.is_empty() {
            let by_id: HashMap<&str, &ClaimState> = ledger
                .claims
                .iter()
                .map(|entry| (entry.claim_id().0.as_str(), entry.state()))
                .collect();
            collect_ledger_unverified(
                &lesson_id,
                block_count,
                &ledger.claims,
                &by_id,
                &mut unverified_questions,
                &mut unverified_quotes,
            );
            ledger_poisoned_blocks(block_count, &ledger.claims, &by_id)
        } else {
            let report_json: Option<String> = sqlx::query_scalar(
                "SELECT report_json FROM learning_lesson_verifications WHERE lesson_id=? ORDER BY checked_at DESC LIMIT 1",
            )
            .bind(&lesson_id)
            .fetch_optional(&mut *connection)
            .await?;
            if let Some(raw) = report_json {
                collect_report_unverified(
                    &raw,
                    &lesson_id,
                    block_count,
                    &mut unverified_questions,
                    &mut unverified_quotes,
                );
            }
            HashSet::new()
        };
        if !poisoned_blocks.is_empty() {
            let block_rows: Vec<(i64, String)> =
                sqlx::query_as("SELECT ordinal, body FROM learning_blocks WHERE lesson_id=?")
                    .bind(&lesson_id)
                    .fetch_all(&mut *connection)
                    .await?;
            for (ordinal, body) in block_rows {
                if usize::try_from(ordinal)
                    .ok()
                    .is_some_and(|idx| poisoned_blocks.contains(&idx))
                {
                    for (start, end) in sentences(&body) {
                        if let Some(piece) = body.get(start..end).map(str::trim) {
                            if piece.chars().count() >= 25 {
                                unverified_quotes
                                    .entry(lesson_id.clone())
                                    .or_default()
                                    .push(piece.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut held_back = HashSet::new();
    if !unverified_questions.is_empty() {
        let own: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, lesson_id FROM learning_assessment_candidates WHERE program_id=? AND origin IN ('lesson','migrated') AND lesson_id IS NOT NULL ORDER BY rowid",
        )
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await?;
        let mut seen: HashMap<String, usize> = HashMap::new();
        for (id, lesson_id) in own {
            let index = seen.entry(lesson_id.clone()).or_insert(0);
            if unverified_questions.contains(&(lesson_id, *index)) {
                held_back.insert(id);
            }
            *index += 1;
        }
    }

    if !unverified_quotes.is_empty() {
        let bank_rows = sqlx::query(
            "SELECT id, lesson_id, source_quote FROM learning_assessment_candidates WHERE program_id=? AND origin='bank' AND source_quote <> ''",
        )
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await?;
        for row in bank_rows {
            let lesson_id: Option<String> = row.get("lesson_id");
            let source_quote: String = row.get("source_quote");
            let quote_matches = |quote: &String| quote_matches_sentence(&source_quote, quote);
            let tainted = match lesson_id.as_deref() {
                Some(lid) => unverified_quotes
                    .get(lid)
                    .is_some_and(|quotes| quotes.iter().any(quote_matches)),
                None => unverified_quotes.values().flatten().any(quote_matches),
            };
            if tainted {
                held_back.insert(row.get("id"));
            }
        }
    }

    Ok(held_back)
}

fn valid_claim_span(body: &str, span: (usize, usize), quote: &str) -> Option<(usize, usize)> {
    let (cs, ce) = span;
    if cs >= ce || ce > body.len() || !body.is_char_boundary(cs) || !body.is_char_boundary(ce) {
        return None;
    }
    let slice = body.get(cs..ce)?;
    quote_matches_sentence(slice, quote).then_some(span)
}

fn quote_matches_sentence(text: &str, quote: &str) -> bool {
    let text = text.trim();
    let quote = quote.trim();
    if text.is_empty() || quote.is_empty() {
        return false;
    }
    text == quote
        || (quote.len() >= 20 && text.contains(quote))
        || (text.len() >= 20 && quote.contains(text))
        || contains_word_bounded(text, quote)
}

fn contains_word_bounded(haystack: &str, needle: &str) -> bool {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let starts_word = needle.chars().next().is_some_and(is_word);
    let ends_word = needle.chars().next_back().is_some_and(is_word);
    haystack.match_indices(needle).any(|(pos, _)| {
        let before = haystack[..pos].chars().next_back();
        let after = haystack[pos + needle.len()..].chars().next();
        let left_ok = !starts_word || before.is_none_or(|c| !is_word(c));
        let right_ok = !ends_word || after.is_none_or(|c| !is_word(c));
        left_ok && right_ok
    })
}

fn ledger_poisoned_blocks(
    block_count: usize,
    claims: &[ClaimLedgerEntry],
    by_id: &HashMap<&str, &ClaimState>,
) -> HashSet<usize> {
    collect_block_claim_stats(claims, block_count, by_id)
        .into_iter()
        .filter(|(_, s)| s.is_ungrounded_dominated())
        .map(|(unit, _)| unit)
        .collect()
}

fn collect_ledger_unverified(
    lesson_id: &str,
    block_count: usize,
    claims: &[ClaimLedgerEntry],
    by_id: &HashMap<&str, &ClaimState>,
    unverified_questions: &mut HashSet<(String, usize)>,
    unverified_quotes: &mut HashMap<String, Vec<String>>,
) {
    for entry in claims {
        if !is_claim_active_unverified(entry.state(), by_id) {
            continue;
        }
        let loc = entry.location();
        match loc.unit_kind {
            UnitKind::Block if !loc.quote.trim().is_empty() => {
                unverified_quotes
                    .entry(lesson_id.to_owned())
                    .or_default()
                    .push(loc.quote.clone());
            }
            UnitKind::Question | UnitKind::AnswerKey if loc.unit_index >= block_count => {
                unverified_questions.insert((lesson_id.to_owned(), loc.unit_index - block_count));
            }
            _ => {}
        }
    }
}

/// Question indices (`0..questions.len()`) in a prepared lesson whose unit
/// carries any unverified claim in `report`.
pub(crate) fn unverified_question_indices(
    block_count: usize,
    report: &LessonVerificationReport,
) -> HashSet<usize> {
    report_unverified_question_indices(report, block_count)
}

/// Syncs `review_status` (`disputed` <-> `agreed`) on a lesson's own seeded
/// questions in `learning_assessment_candidates` against the lesson's current
/// claim ledger. Never changes structurally invalid `dropped` items.
pub(crate) async fn sync_disputed_questions_in(tx: &mut WriteTx, lesson_id: &str) -> Result<()> {
    Box::pin(sync_disputed_questions_inner(tx, lesson_id)).await
}

async fn sync_disputed_questions_inner(tx: &mut WriteTx, lesson_id: &str) -> Result<()> {
    let ledger = Box::pin(load_in(tx, lesson_id)).await?;
    if ledger.claims.is_empty() {
        return Ok(());
    }
    let block_count = usize::try_from(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=?")
            .bind(lesson_id)
            .fetch_one(&mut **tx)
            .await?,
    )
    .unwrap_or(0);
    let by_id: HashMap<&str, &ClaimState> = ledger
        .claims
        .iter()
        .map(|entry| (entry.claim_id().0.as_str(), entry.state()))
        .collect();
    let mut unverified_questions = HashSet::new();
    let mut ignored = HashMap::new();
    collect_ledger_unverified(
        lesson_id,
        block_count,
        &ledger.claims,
        &by_id,
        &mut unverified_questions,
        &mut ignored,
    );
    let own: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, review_status FROM learning_assessment_candidates WHERE lesson_id=? AND origin IN ('lesson','migrated') ORDER BY rowid",
    )
    .bind(lesson_id)
    .fetch_all(&mut **tx)
    .await?;
    for (index, (id, status)) in own.into_iter().enumerate() {
        let is_unverified = unverified_questions.contains(&(lesson_id.to_owned(), index));
        let parsed = super::bank::CandidateReviewStatus::parse(&status)?;
        let next = match (parsed, is_unverified) {
            (super::bank::CandidateReviewStatus::Disputed, false) => {
                Some(super::bank::CandidateReviewStatus::Agreed)
            }
            (super::bank::CandidateReviewStatus::Agreed, true) => {
                Some(super::bank::CandidateReviewStatus::Disputed)
            }
            _ => None,
        };
        if let Some(target_status) = next {
            super::bank::set_candidate_review_status_in(tx, &id, target_status).await?;
        }
    }
    Ok(())
}
