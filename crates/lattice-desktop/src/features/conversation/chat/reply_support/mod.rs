//! Per-chip citation support metadata and unquoted quotation wrapping for
//! chat, Explorer, and journal-insight replies (§4, §7.2).

mod sentences;
mod unquoted;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::ops::Range;

use lattice_core::application::contracts::replies::{
    AnchorDto, ByteSpanDto, RelationDto, ReplyPieceDto, SupportDto,
};
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::services::claim_verification::ClaimVerdict;
use lattice_core::application::services::model_contract::shown::Kind;
use lattice_core::application::services::tool_handles::Source;
use lattice_core::shared::markdown_math::inline_math_spans;
use lattice_core::shared::text::same_text;
use lattice_core::shared::text::segmentation::{code_fences, CodeFence};
use serde_json::Value;

use self::sentences::{assign_sentences, split_sentence_spans, SentenceSpan};
use self::unquoted::find_unquoted_in_sentence;
use super::reply_text::{wire, Placed, ReplyText};
use super::verification::{lexical, quotes};

#[derive(Debug, Clone)]
struct StoredVerdict {
    sentence: String,
    folded: String,
    citation_ids: Vec<u32>,
    verdict: ClaimVerdict,
    evidence_quote: Option<String>,
    method: String,
}

#[derive(Debug, Clone)]
struct ReferenceSupport {
    support: Option<SupportDto>,
    quoted_span: Option<Range<usize>>,
}

struct SourceText<'a> {
    /// Full page text stored on a captured snapshot (starts at byte 0).
    full: Option<&'a str>,
    /// Passage text (content or excerpt) without document byte offset.
    passage: &'a str,
}

impl<'a> SourceText<'a> {
    fn of(source: &'a SourceDto) -> Self {
        let full = source
            .web_snapshot
            .as_ref()
            .map(|snapshot| snapshot.text.as_str())
            .filter(|text| !text.trim().is_empty());
        let passage = lexical::passage_text(source);
        Self { full, passage }
    }

    fn haystack(&self) -> &'a str {
        self.full.unwrap_or(self.passage)
    }

    /// Locates `quote` in `web_snapshot.text` (returning a document byte span)
    /// or `passage` (`span: None` because chunk byte offsets are unknown).
    fn anchor_for_find(
        &self,
        quote: &str,
        matched: fn(&str, &str) -> Option<(usize, usize)>,
    ) -> Option<AnchorDto> {
        if let Some(full) = self.full {
            if let Some((start, end)) = matched(full, quote) {
                if let Some(exact) = full.get(start..end) {
                    return Some(AnchorDto {
                        exact: exact.to_owned(),
                        span: Some(ByteSpanDto { start, end }),
                    });
                }
            }
        }
        if let Some((start, end)) = matched(self.passage, quote) {
            if let Some(exact) = self.passage.get(start..end) {
                return Some(AnchorDto {
                    exact: exact.to_owned(),
                    span: None,
                });
            }
        }
        None
    }

    /// Locates `quote` inside `web_snapshot.text` if available, else returns an
    /// unspanned anchor carrying `quote`.
    fn anchor_from_evidence(&self, quote: &str) -> AnchorDto {
        self.anchor_for_find(quote, same_text::find)
            .unwrap_or_else(|| AnchorDto {
                exact: quote.to_owned(),
                span: None,
            })
    }
}

/// Converts `reply` into frontend pieces with `ReplyReferenceDto.support`
/// populated from `verification` and `sources`, and exact unquoted quotations
/// wrapped as `ReplyPieceDto::Quoted` (§4, §7.2).
pub(crate) fn pieces(
    reply: &ReplyText,
    sources: &[SourceDto],
    verification: Option<&Value>,
) -> Vec<ReplyPieceDto> {
    if reply.references.is_empty() {
        return reply.pieces();
    }

    let verdicts = parse_verdicts(verification);
    let fences = code_fences(&reply.text);
    let formulas = inline_math_spans(&reply.text);
    let sentences = split_sentence_spans(reply, &fences, &formulas);
    let ref_sentences = assign_sentences(reply, &sentences);

    let mut per_ref: Vec<ReferenceSupport> = Vec::with_capacity(reply.references.len());
    let mut quoted_spans: Vec<Range<usize>> = Vec::new();

    for (ref_index, placed) in reply.references.iter().enumerate() {
        let sentence = ref_sentences
            .get(ref_index)
            .and_then(|&idx| idx.and_then(|i| sentences.get(i)));
        let outcome = evaluate_reference(
            reply, placed, sentence, &fences, &formulas, sources, &verdicts,
        );
        if let Some(span) = outcome.quoted_span.clone() {
            if can_keep_quoted_span(reply, &quoted_spans, &span) {
                quoted_spans.push(span);
            }
        }
        per_ref.push(outcome);
    }

    assemble_pieces(reply, &per_ref, &quoted_spans)
}

fn evaluate_reference(
    reply: &ReplyText,
    placed: &Placed,
    sentence: Option<&SentenceSpan>,
    fences: &[CodeFence],
    formulas: &[Range<usize>],
    sources: &[SourceDto],
    verdicts: &[StoredVerdict],
) -> ReferenceSupport {
    if placed.reference.prefix != Source::PREFIX {
        return ReferenceSupport {
            support: None,
            quoted_span: None,
        };
    }

    let cites_relation = SupportDto {
        relation: RelationDto::Cites,
        anchors: Vec::new(),
    };

    let Some(sentence) = sentence else {
        return ReferenceSupport {
            support: Some(cites_relation),
            quoted_span: None,
        };
    };
    if sentence.in_fence || sentence.is_heading {
        return ReferenceSupport {
            support: Some(cites_relation),
            quoted_span: None,
        };
    }

    let sentence_plain = reply.without_references_in(sentence.range.clone());
    let Some(verdict) = find_matching_verdict(&sentence_plain, placed.reference.number, verdicts)
    else {
        return ReferenceSupport {
            support: Some(cites_relation),
            quoted_span: None,
        };
    };

    if verdict.verdict != ClaimVerdict::Supported {
        return ReferenceSupport {
            support: Some(cites_relation),
            quoted_span: None,
        };
    }

    let Some(source) =
        lexical::source_index(placed.reference.number, sources).and_then(|idx| sources.get(idx))
    else {
        return ReferenceSupport {
            support: Some(SupportDto {
                relation: RelationDto::Backs,
                anchors: backs_anchors_without_source(verdict),
            }),
            quoted_span: None,
        };
    };

    let text = SourceText::of(source);
    let haystack = text.haystack();

    // 1. Explicit quoted words inside `"..."` or `“...”` for this sentence/chip.
    if !haystack.trim().is_empty() {
        let prose_for_explicit = explicit_quote_context(reply, sentence, placed);
        let spans = quotes::matched_quotations(&prose_for_explicit, haystack);
        if !spans.is_empty() {
            let anchors = spans
                .into_iter()
                .filter_map(|(start, end)| {
                    let exact = haystack.get(start..end)?;
                    text.anchor_for_find(exact, same_text::quotation)
                        .or_else(|| {
                            Some(AnchorDto {
                                exact: exact.to_owned(),
                                span: None,
                            })
                        })
                })
                .collect();
            return ReferenceSupport {
                support: Some(SupportDto {
                    relation: RelationDto::Quotes,
                    anchors,
                }),
                quoted_span: None,
            };
        }

        // 2. Unquoted exact quotation in prose -> wrap as `Quoted` and mark `Quotes`.
        if let Some((prose_span, exact_quote)) =
            find_unquoted_in_sentence(reply, sentence, placed, fences, formulas, haystack)
        {
            let anchor = text
                .anchor_for_find(&exact_quote, same_text::quotation)
                .unwrap_or(AnchorDto {
                    exact: exact_quote,
                    span: None,
                });
            return ReferenceSupport {
                support: Some(SupportDto {
                    relation: RelationDto::Quotes,
                    anchors: vec![anchor],
                }),
                quoted_span: Some(prose_span),
            };
        }

        // 3. Explicit `verbatim` / `quote` method or near-verbatim lexical settlement.
        let method_is_verbatim = verdict.method == "verbatim"
            || verdict.method == "quote"
            || (verdict.method == "lexical" && lexical::is_verbatim_copy(&sentence_plain, source));
        if method_is_verbatim {
            let candidate = verdict
                .evidence_quote
                .as_deref()
                .map(str::trim)
                .filter(|quote| !quote.is_empty())
                .map(str::to_owned)
                .or_else(|| lexical::best_sentence(&sentence_plain, haystack));
            let anchors = candidate
                .map(|quote| vec![text.anchor_from_evidence(&quote)])
                .unwrap_or_default();
            return ReferenceSupport {
                support: Some(SupportDto {
                    relation: RelationDto::Quotes,
                    anchors,
                }),
                quoted_span: None,
            };
        }
    } else if verdict.method == "verbatim" || verdict.method == "quote" {
        return ReferenceSupport {
            support: Some(SupportDto {
                relation: RelationDto::Quotes,
                anchors: backs_anchors_without_source(verdict),
            }),
            quoted_span: None,
        };
    }

    // 4. Supported by verifier -> `Backs` with evidence quote anchor when known.
    ReferenceSupport {
        support: Some(SupportDto {
            relation: RelationDto::Backs,
            anchors: backs_anchors_for_source(verdict, &text),
        }),
        quoted_span: None,
    }
}

fn backs_anchors_without_source(verdict: &StoredVerdict) -> Vec<AnchorDto> {
    verdict
        .evidence_quote
        .as_deref()
        .map(str::trim)
        .filter(|quote| !quote.is_empty())
        .map(|quote| AnchorDto {
            exact: quote.to_owned(),
            span: None,
        })
        .into_iter()
        .collect()
}

fn backs_anchors_for_source(verdict: &StoredVerdict, text: &SourceText<'_>) -> Vec<AnchorDto> {
    let Some(quote) = verdict
        .evidence_quote
        .as_deref()
        .map(str::trim)
        .filter(|quote| !quote.is_empty())
    else {
        return Vec::new();
    };
    if let Some(anchor) = text.anchor_for_find(quote, same_text::find) {
        return vec![anchor];
    }
    if verdict.citation_ids.len() <= 1 || text.haystack().trim().is_empty() {
        return vec![AnchorDto {
            exact: quote.to_owned(),
            span: None,
        }];
    }
    Vec::new()
}

fn explicit_quote_context(reply: &ReplyText, sentence: &SentenceSpan, placed: &Placed) -> String {
    let line_start = reply
        .text
        .get(..sentence.range.start)
        .and_then(|head| head.rfind('\n'))
        .map_or(0, |at| at + 1);
    let prev_ref_end = reply
        .references
        .iter()
        .filter(|other| other.range.end <= placed.range.start && other.range.end >= line_start)
        .map(|other| other.range.end)
        .max()
        .unwrap_or(line_start);
    let end = placed
        .range
        .start
        .max(sentence.range.end.min(placed.range.end));
    let base_slice = reply.without_references_in(sentence.range.start..end);
    let has_odd_quote = base_slice.matches(['"', '\u{201c}', '\u{201d}']).count() % 2 == 1;
    let from = if has_odd_quote {
        prev_ref_end.min(sentence.range.start)
    } else {
        sentence.range.start
    };
    let to = end.min(reply.text.len());
    reply.without_references_in(from..to)
}

fn can_keep_quoted_span(
    reply: &ReplyText,
    existing: &[Range<usize>],
    candidate: &Range<usize>,
) -> bool {
    if candidate.start >= candidate.end {
        return false;
    }
    let overlaps_existing = existing
        .iter()
        .any(|kept| candidate.start < kept.end && kept.start < candidate.end);
    let crosses_reference = reply
        .references
        .iter()
        .any(|placed| placed.range.start < candidate.end && candidate.start < placed.range.end);
    !overlaps_existing && !crosses_reference
}

fn assemble_pieces(
    reply: &ReplyText,
    per_ref: &[ReferenceSupport],
    quoted_spans: &[Range<usize>],
) -> Vec<ReplyPieceDto> {
    let mut text_cuts: BTreeSet<usize> = BTreeSet::new();
    for span in quoted_spans {
        text_cuts.insert(span.start);
        text_cuts.insert(span.end);
    }

    let mut pieces = Vec::new();
    let mut at = 0usize;
    for (index, placed) in reply.references.iter().enumerate() {
        push_segmented_text(
            &mut pieces,
            &reply.text,
            at..placed.range.start,
            quoted_spans,
            &text_cuts,
        );
        let mut reference = wire(&placed.reference);
        reference.support = per_ref.get(index).and_then(|entry| entry.support.clone());
        pieces.push(ReplyPieceDto::Reference { reference });
        at = placed.range.end;
    }
    push_segmented_text(
        &mut pieces,
        &reply.text,
        at..reply.text.len(),
        quoted_spans,
        &text_cuts,
    );
    pieces
}

fn push_segmented_text(
    pieces: &mut Vec<ReplyPieceDto>,
    full_text: &str,
    range: Range<usize>,
    quoted_spans: &[Range<usize>],
    text_cuts: &BTreeSet<usize>,
) {
    if range.start >= range.end {
        return;
    }
    let mut from = range.start;
    for &cut in text_cuts.range((range.start + 1)..range.end) {
        push_chunk(pieces, full_text, from..cut, quoted_spans);
        from = cut;
    }
    push_chunk(pieces, full_text, from..range.end, quoted_spans);
}

fn push_chunk(
    pieces: &mut Vec<ReplyPieceDto>,
    full_text: &str,
    range: Range<usize>,
    quoted_spans: &[Range<usize>],
) {
    let Some(chunk) = full_text.get(range.clone()).filter(|s| !s.is_empty()) else {
        return;
    };
    let is_quoted = quoted_spans
        .iter()
        .any(|span| span.start <= range.start && range.end <= span.end);
    if is_quoted {
        pieces.push(ReplyPieceDto::Quoted {
            text: chunk.to_owned(),
        });
    } else {
        pieces.push(ReplyPieceDto::Text {
            text: chunk.to_owned(),
        });
    }
}

fn parse_verdicts(verification: Option<&Value>) -> Vec<StoredVerdict> {
    let Some(items) = verification
        .and_then(|v| v.get("claimVerdicts"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let sentence = item.get("sentence")?.as_str()?.trim().to_owned();
            let verdict = match item.get("verdict")?.as_str()? {
                "supported" => ClaimVerdict::Supported,
                "contradicted" => ClaimVerdict::Contradicted,
                "unsupported" => ClaimVerdict::Unsupported,
                "unverified" => ClaimVerdict::Unverified,
                _ => return None,
            };
            let citation_ids = item
                .get("citationIds")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| u32::try_from(v.as_u64()?).ok())
                        .collect()
                })
                .unwrap_or_default();
            let evidence_quote = item
                .get("evidenceQuote")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let method = item
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            Some(StoredVerdict {
                folded: same_text::fold(&sentence),
                sentence,
                citation_ids,
                verdict,
                evidence_quote,
                method,
            })
        })
        .collect()
}

fn find_matching_verdict<'a>(
    sentence_plain: &str,
    citation_number: u32,
    verdicts: &'a [StoredVerdict],
) -> Option<&'a StoredVerdict> {
    let folded_sentence = same_text::fold(sentence_plain);
    let folded_trimmed = same_text::fold(sentence_plain.trim_end_matches('.'));
    let mut best: Option<(u8, usize, &'a StoredVerdict)> = None;

    for verdict in verdicts {
        if !verdict.citation_ids.is_empty() && !verdict.citation_ids.contains(&citation_number) {
            continue;
        }
        let verdict_trimmed = same_text::fold(verdict.sentence.trim_end_matches('.'));
        let tier = if !folded_sentence.is_empty() && verdict.folded == folded_sentence {
            3u8
        } else if !folded_trimmed.is_empty() && verdict_trimmed == folded_trimmed {
            2u8
        } else if same_text::overlap(&verdict.sentence, sentence_plain) {
            1u8
        } else {
            continue;
        };
        let prefix_score = same_text::common_prefix(&verdict.folded, &folded_sentence);
        let replaces = best.is_none_or(|(best_tier, best_score, _)| {
            tier > best_tier || (tier == best_tier && prefix_score > best_score)
        });
        if replaces {
            best = Some((tier, prefix_score, verdict));
        }
    }
    best.map(|(_, _, verdict)| verdict)
}
