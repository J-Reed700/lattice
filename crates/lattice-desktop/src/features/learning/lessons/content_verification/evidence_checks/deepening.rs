//! Deepening: a claim its first page leaves unsupported walks outward
//! through the course's sources (claim evidence pages §4.1, §7). Each round
//! builds every walker's next page, ring 0 left over first, and checks the
//! pages packed by section like first checks. Rings 1 and 2 run to the end
//! for every walker; a walker on the deeper pages gets another round only
//! while the last round settled a claim, since a long document's fifth
//! passage is where the cost is. Each round logs the page budget the
//! backend's measured pace and the job's measured answers give
//! (`page_budget`); pages are not yet sized by it.
use super::*;
use crate::features::learning::reference_collection::Gaps;
use std::collections::BTreeMap;

/// A claim still looking for evidence: what it has been shown so far, and
/// how far its walk has gone.
struct Walk {
    unit: usize,
    claim: Claim,
    shown: Vec<EvidencePassage>,
    /// The stage of the last page it was checked against.
    stage: Stage,
}

/// What one walker's page held, for its round's log line.
struct PageNote {
    ordinal: usize,
    stage: Stage,
    passages: usize,
    tokens: usize,
    counted_exactly: bool,
    gaps: Gaps,
    counter: bool,
}

impl std::fmt::Display for PageNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let gap = |clear: Option<bool>| match clear {
            Some(true) => "clear",
            Some(false) => "none",
            None => "-",
        };
        write!(
            f,
            "claim {} {:?}: {} passages, {} tokens{}, library gap {}, keyword gap {}, counter page {}",
            self.ordinal,
            self.stage,
            self.passages,
            self.tokens,
            if self.counted_exactly { "" } else { " (estimated)" },
            gap(self.gaps.library),
            gap(self.gaps.keyword),
            if self.counter { "ran" } else { "none" },
        )
    }
}

/// The walker's next page: the passages its section cites that it has not
/// been shown, while a check of them alone fits the window, then its next
/// ring page and, from ring 2 on, its counter-evidence page. `None` once
/// nothing is left: ring 0 shown and the deeper pages exhausted.
async fn next_page(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    ordinal: usize,
    walk: &mut Walk,
    citations: &Citations,
) -> Result<Option<(Vec<EvidencePassage>, PageNote)>> {
    let mut page: Vec<EvidencePassage> = Vec::new();
    for passage in citations.of(walk.unit) {
        if walk
            .shown
            .iter()
            .chain(&page)
            .any(|seen| overlaps(seen, passage))
        {
            continue;
        }
        if !fits_window(
            llm,
            &walk.claim.statement,
            page.iter().chain(std::iter::once(passage)),
        ) {
            break;
        }
        page.push(passage.clone());
    }
    let shown: Vec<_> = walk.shown.iter().chain(&page).cloned().collect();
    let ring = ring_page(
        references,
        &walk.claim.statement,
        walk.stage,
        &shown,
        citations,
    )
    .await?;
    walk.stage = ring.stage;
    let mut counter = false;
    if !ring.passages.is_empty() && ring.stage >= Stage::Ring2 {
        let seen: Vec<_> = shown.iter().chain(&ring.passages).cloned().collect();
        if let Some(found) = counter_page(references, &walk.claim.statement, &seen).await? {
            counter = true;
            page.extend(found);
        }
    }
    page.extend(ring.passages);
    if page.is_empty() {
        return Ok(None);
    }
    canonicalize_evidence(&mut page);
    let text = page
        .iter()
        .map(|passage| passage.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    // The log's measure only: a backend that cannot count leaves the estimate.
    let (tokens, counted_exactly) = match llm.count_tokens_exact(&text).await {
        Ok(tokens) => (tokens, llm.counts_tokens_exactly()),
        Err(error) => {
            tracing::debug!(%error, "The backend could not count a deeper page; logging the estimate");
            (llm.count_tokens(&text), false)
        }
    };
    let note = PageNote {
        ordinal,
        stage: ring.stage,
        passages: page.len(),
        tokens,
        counted_exactly,
        gaps: ring.gaps,
        counter,
    };
    Ok(Some((page, note)))
}

/// Check every first-page result that is unsupported against its next pages
/// until a page decides it or nothing on its subject is left. A deciding
/// page's finding replaces the first page's; the first page's key still
/// names the decision, so a later pass on the same evidence reuses it.
/// `answers` is what the job's checks so far cost per claim; each round's
/// checks add to it, and each round logs the page budget it gives.
pub(super) async fn deepen(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    results: &mut [CheckedClaim],
    decided: &HashSet<usize>,
    citations: &Citations,
    labels: &Labels,
    mut answers: AnswerSize,
) -> Result<()> {
    let mut walks: BTreeMap<usize, Walk> = results
        .iter()
        .filter(|(ordinal, _, finding, _)| {
            !decided.contains(ordinal) && finding.verdict == ClaimVerdict::Unsupported
        })
        .map(|(ordinal, _, finding, _)| {
            (
                *ordinal,
                Walk {
                    unit: finding.unit,
                    claim: Claim {
                        quote: finding.quote.clone(),
                        statement: finding.statement.clone(),
                    },
                    shown: finding
                        .evidence
                        .iter()
                        .filter(|passage| passage.retrieval_kind != "execution")
                        .cloned()
                        .collect(),
                    // A ring the first page finished is skipped when the
                    // walk asks it again.
                    stage: Stage::Ring1,
                },
            )
        })
        .collect();
    let mut round = 0;
    while !walks.is_empty() {
        round += 1;
        // Measured, not yet applied: the next page structure sizes pages by it.
        let budget = page_budget(llm.pace(), answers.per_claim());
        let mut pending = Vec::new();
        let mut outcomes = Vec::new();
        let mut notes = Vec::new();
        for (ordinal, mut walk) in std::mem::take(&mut walks) {
            let Some((page, note)) =
                next_page(llm, references, ordinal, &mut walk, citations).await?
            else {
                continue;
            };
            notes.push(note);
            let checked = as_checked(&walk.claim, citations, references);
            let key = ClaimChecks::key(walk.unit, &checked, &page);
            let receipt_key = claim_receipt_key(llm, &key);
            if let Some(finding) =
                crate::features::learning::lesson_drafts::checkpoint(&receipt_key)
                    .await?
                    .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
                    .and_then(|receipt| receipt.finding(walk.unit, &walk.claim, &page))
            {
                outcomes.push((ordinal, finding));
            } else {
                pending.push(PendingClaim {
                    ordinal,
                    key,
                    receipt_key,
                    unit: walk.unit,
                    claim: walk.claim.clone(),
                    checked: checked.statement,
                    evidence: page,
                });
            }
            // The page joins what the claim was shown once its check says
            // what the judge saw: a passage cut for size was not shown.
            walks.insert(ordinal, walk);
        }
        if !pending.is_empty() || !outcomes.is_empty() {
            crate::features::learning::lesson_progress::deeper_page(
                round + 1,
                pending.len() + outcomes.len(),
            );
            // A saved check of this page is finished as soon as it is restored.
            for (_, finding) in &outcomes {
                crate::features::learning::lesson_progress::rechecked(
                    finding.verdict == ClaimVerdict::Supported,
                );
            }
        }
        if !pending.is_empty() {
            crate::features::learning::lesson_progress::stage(format!(
                "Checking {} unsupported claims against their next page of evidence (page {})",
                pending.len(),
                round + 1
            ));
            let mut batches = futures::stream::iter(pack(llm, pending, labels))
                .map(|request| group(llm, request, Check::Deeper, labels))
                .buffer_unordered(1);
            while let Some(batch) = batches.next().await {
                let (batch, answered) = batch?;
                answers.add(answered);
                outcomes.extend(
                    batch
                        .into_iter()
                        .map(|(ordinal, _, finding, _)| (ordinal, finding)),
                );
            }
        }
        let mut settled = 0;
        for (ordinal, finding) in outcomes {
            if finding.verdict == ClaimVerdict::Unsupported {
                if let Some(walk) = walks.get_mut(&ordinal) {
                    walk.shown.extend(finding.evidence);
                }
                continue;
            }
            settled += usize::from(matches!(
                finding.verdict,
                ClaimVerdict::Supported | ClaimVerdict::Contradicted
            ));
            walks.remove(&ordinal);
            if let Some(result) = results.iter_mut().find(|result| result.0 == ordinal) {
                result.2 = finding;
            }
        }
        // The group stop: a claim on the deeper pages gets another round only
        // while the last one settled a claim. Rings 1 and 2 run to the end.
        if settled == 0 {
            walks.retain(|_, walk| walk.stage < Stage::Deeper);
        }
        tracing::info!(
            round,
            settled,
            looking = walks.len(),
            page_budget = ?budget,
            pages = %notes
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
            "Deepened the evidence of unsupported claims"
        );
    }
    crate::features::learning::lesson_progress::deeper_done();
    Ok(())
}

/// One page of a claim's evidence walk, as [`walk_pages`] reports it.
#[cfg(test)]
pub(in crate::features::learning) struct WalkedPage {
    /// The ring its retrieved passages came from.
    pub(in crate::features::learning) stage: String,
    pub(in crate::features::learning) passages: Vec<EvidencePassage>,
    pub(in crate::features::learning) gaps: Gaps,
    pub(in crate::features::learning) counter: bool,
}

/// The pages `statement`'s walk shows, in order, if no page settles it and
/// the judge reads each page whole: page 1, then the deeper pages until
/// `found` holds for a page or nothing on the subject is left. `cited` is
/// what each section cites, by unit. For the page-recall eval, which asks
/// on which page a passage would first be read.
#[cfg(test)]
pub(in crate::features::learning) async fn walk_pages(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    (unit, statement): (usize, &str),
    cited: HashMap<usize, Vec<EvidencePassage>>,
    found: impl Fn(&[EvidencePassage]) -> bool,
) -> Result<Vec<WalkedPage>> {
    let citations = Citations::new(cited, HashMap::new());
    let (page, ring) = page_one(llm, statement, unit, references, &[], &citations).await?;
    let mut walk = Walk {
        unit,
        claim: Claim {
            quote: statement.into(),
            statement: statement.into(),
        },
        shown: page.clone(),
        stage: Stage::Ring1,
    };
    let mut pages = vec![WalkedPage {
        stage: format!("{:?}", ring.stage),
        passages: page,
        gaps: ring.gaps,
        counter: false,
    }];
    while !pages.last().is_some_and(|page| found(&page.passages)) {
        let Some((page, note)) = next_page(llm, references, 0, &mut walk, &citations).await? else {
            break;
        };
        walk.shown.extend(page.iter().cloned());
        pages.push(WalkedPage {
            stage: format!("{:?}", note.stage),
            passages: page,
            gaps: note.gaps,
            counter: note.counter,
        });
    }
    Ok(pages)
}
