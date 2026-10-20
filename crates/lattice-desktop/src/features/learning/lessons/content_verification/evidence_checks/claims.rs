//! One claim's comparisons in a pass: pass A against what its section
//! cites, prepared at once, and pass B's first page, restored or read and
//! sized as soon as the job can size it (course evidence index §5.3).
use super::*;

/// One of a claim's comparisons in this pass.
pub(super) enum Comparison {
    /// Its finding, and whether a saved check gave it.
    Done(Finding, bool),
    Pending(PendingClaim),
    /// Sent to the model; its finding is on the way.
    Sent,
}

impl Comparison {
    pub(super) fn done(&self) -> Option<(&Finding, bool)> {
        match self {
            Self::Done(finding, reused) => Some((finding, *reused)),
            _ => None,
        }
    }

    /// The page it compares, and whether the model is asked about it.
    fn page(&self) -> (&[EvidencePassage], bool) {
        match self {
            Self::Pending(item) => (&item.evidence, true),
            Self::Done(finding, _) => (&finding.evidence, false),
            Self::Sent => (&[], false),
        }
    }

    /// The comparison to send, now marked sent; `None` when it is not
    /// waiting for the model.
    pub(super) fn send(&mut self) -> Option<PendingClaim> {
        match std::mem::replace(self, Self::Sent) {
            Self::Pending(item) => Some(item),
            other => {
                *self = other;
                None
            }
        }
    }
}

/// A claim whose two passes are compared in this pass.
pub(super) struct Compared {
    pub(super) ordinal: usize,
    pub(super) unit: usize,
    pub(super) claim: Claim,
    pub(super) checked: String,
    /// The key of its whole decision (`outcome::decision_key`), once pass
    /// B's first page is known.
    pub(super) key: String,
    a_key: Option<String>,
    pub(super) attribution: Option<Comparison>,
    /// Pass B's first page; `None` while it waits to be sized.
    pub(super) corroboration: Option<Comparison>,
    /// Pass A's reference passages, which pass B never shows.
    shown_by_a: Vec<EvidencePassage>,
    /// The reach pass B's first page was read from.
    pub(super) reach: Option<Reach>,
    /// Source versions research saved for the claim, which its walk reads.
    pub(super) researched: Vec<String>,
    /// The source versions the claim's section cites.
    pub(super) cited: Vec<String>,
    /// The receipt key of pass B's first page.
    pub(super) first_receipt: String,
}

impl Compared {
    /// The claim's finding once every comparison is done, and whether all
    /// of them were restored.
    pub(super) fn decided(&self) -> Option<(Finding, bool)> {
        let (b, b_reused) = self.corroboration.as_ref()?.done()?;
        let a = match &self.attribution {
            Some(side) => Some(side.done()?),
            None => None,
        };
        let reused = b_reused && a.is_none_or(|(_, reused)| reused);
        Some((
            combine(
                a.map(|(finding, _)| finding.clone()),
                b.clone(),
                (self.reach, 1),
                &self.cited,
            ),
            reused,
        ))
    }

    /// The comparisons of this claim waiting for the model, now sent.
    pub(super) fn send(&mut self) -> Vec<PendingClaim> {
        self.attribution
            .iter_mut()
            .chain(self.corroboration.iter_mut())
            .filter_map(Comparison::send)
            .collect()
    }

    /// Its pass A comparison, if waiting for the model, now sent.
    pub(super) fn send_attribution(&mut self) -> Option<PendingClaim> {
        self.attribution.as_mut().and_then(Comparison::send)
    }

    /// Records the model's finding for its `pass`.
    pub(super) fn answered(&mut self, pass: Pass, finding: Finding) {
        let side = match pass {
            Pass::Attribution => self.attribution.as_mut(),
            Pass::Corroboration => self.corroboration.as_mut(),
        };
        if let Some(side) = side {
            *side = Comparison::Done(finding, false);
        }
    }
}

pub(super) enum Prepared {
    /// A finished decision: `true` when it is a retained final decision that
    /// already walked its pages, `false` for one decided on the spot.
    Complete(Box<CheckedClaim>, bool),
    Compared(Box<Compared>),
}

/// What preparing a claim reads beside the claim itself.
pub(super) struct Context<'c, 'r> {
    pub(super) llm: &'c dyn LLMPort,
    pub(super) references: &'c ReferenceCollection<'r>,
    pub(super) sources: &'c [SourceBinding],
    pub(super) executions: &'c [execution::Observation],
    pub(super) retained: &'c ClaimChecks,
    pub(super) citations: &'c Citations,
    pub(super) paging: Paging,
}

impl Context<'_, '_> {
    /// The claim's comparisons: pass A's prepared, pass B's first page as
    /// [`Self::corroborate`] can have it at `sizing`. A claim settled by its
    /// quotation, by the user's evidence or by an earlier pass's whole
    /// decision is complete.
    pub(super) async fn prepare(
        &self,
        ordinal: usize,
        (unit, claim): (usize, Claim),
        sizing: &Sizing,
    ) -> Result<Prepared> {
        let (llm, references, citations) = (self.llm, self.references, self.citations);
        // A quotation found in the source it names is settled by the match.
        if let Some(finding) = quote_claims::settle(references, citations, unit, &claim)? {
            let key = ClaimChecks::key(unit, &claim, &finding.evidence);
            return Ok(Prepared::Complete(
                Box::new((ordinal, key, finding, false)),
                true,
            ));
        }
        // Resolved by evidence the user brought: that check is the receipt.
        if let Some(finding) = self.retained.settled_finding(unit, &claim) {
            let key = ClaimChecks::key(unit, &claim, &finding.evidence);
            return Ok(Prepared::Complete(
                Box::new((ordinal, key, finding, true)),
                true,
            ));
        }
        let checked = as_checked(&claim, citations, references);
        // Pass A: the section's passed example runs and the passages it
        // cites, while a check of them alone fits the window.
        let mut attribution = example_runs(self.executions, unit);
        let shown_by_a = take_cited(
            llm,
            &checked.statement,
            &mut attribution,
            citations.of(unit),
        );
        canonicalize_evidence(&mut attribution);
        let a_key =
            (!attribution.is_empty()).then(|| ClaimChecks::key(unit, &checked, &attribution));
        let mut cited: Vec<String> = shown_by_a
            .iter()
            .map(|passage| passage.source_id.clone())
            .collect();
        cited.dedup();
        let mut compared = Compared {
            ordinal,
            unit,
            claim,
            checked: checked.statement,
            key: String::new(),
            a_key: a_key.clone(),
            attribution: None,
            corroboration: None,
            shown_by_a,
            reach: None,
            researched: Vec::new(),
            cited,
            first_receipt: String::new(),
        };
        if let Some(a_key) = a_key {
            let pending = pending(llm, &compared, a_key, attribution, Pass::Attribution);
            let side = compare(llm, self.retained, pending).await?;
            let (page, judged) = side.page();
            log_page(llm, (ordinal, 1), Pass::Attribution, page, None, judged).await;
            compared.attribution = Some(side);
        }
        if let Some(finding) = self.corroborate(&mut compared, sizing).await? {
            return Ok(Prepared::Complete(
                Box::new((ordinal, compared.key, finding, true)),
                true,
            ));
        }
        Ok(Prepared::Compared(Box::new(compared)))
    }

    /// Pass B's first page for `claim`, when it can be had at `sizing`:
    /// restored from its saved selection, or read and sized now. A page that
    /// must be sized waits while the job's answers are unmeasured, leaving
    /// the claim's corroboration empty. Returns an earlier pass's whole
    /// decision on the same two comparisons, which completes the claim.
    pub(super) async fn corroborate(
        &self,
        claim: &mut Compared,
        sizing: &Sizing,
    ) -> Result<Option<Finding>> {
        let llm = self.llm;
        let Some(selected) = evidence_selection::select(
            llm,
            (&claim.claim, &claim.checked, claim.unit),
            self.references,
            self.sources,
            self.retained,
            (self.citations, &claim.shown_by_a, self.paging, sizing),
        )
        .await?
        else {
            return Ok(None);
        };
        let checked = Claim {
            quote: claim.claim.quote.clone(),
            statement: claim.checked.clone(),
        };
        let b_key = ClaimChecks::key(claim.unit, &checked, &selected.evidence);
        claim.key = decision_key(claim.a_key.as_deref(), &b_key);
        claim.first_receipt = claim_receipt_key(llm, &b_key);
        claim.reach = selected.reach;
        claim.researched = selected.researched;
        if let Some(finding) = self.retained.get(&claim.key, claim.unit, &claim.claim, &[]) {
            return Ok(Some(finding));
        }
        let pending = pending(llm, claim, b_key, selected.evidence, Pass::Corroboration);
        let side = compare(llm, self.retained, pending).await?;
        let (page, judged) = side.page();
        log_page(
            llm,
            (claim.ordinal, 1),
            Pass::Corroboration,
            page,
            selected.note.as_ref(),
            judged,
        )
        .await;
        claim.corroboration = Some(side);
        Ok(None)
    }
}

/// `claim`'s comparison of `evidence` for `pass`, keyed by `key`.
fn pending(
    llm: &dyn LLMPort,
    claim: &Compared,
    key: String,
    evidence: Vec<EvidencePassage>,
    pass: Pass,
) -> PendingClaim {
    PendingClaim {
        ordinal: claim.ordinal,
        receipt_key: claim_receipt_key(llm, &key),
        key,
        unit: claim.unit,
        claim: claim.claim.clone(),
        checked: claim.checked.clone(),
        evidence,
        pass,
    }
}

/// One comparison: restored from its receipt (or a receipt saved under an
/// earlier key for the same claim and evidence), decided on the spot when it
/// has nothing to compare, or left for the model.
async fn compare(
    llm: &dyn LLMPort,
    retained: &ClaimChecks,
    pending: PendingClaim,
) -> Result<Comparison> {
    let (unit, claim, evidence) = (pending.unit, &pending.claim, &pending.evidence);
    if let Some(finding) =
        crate::features::learning::lesson_drafts::checkpoint(&pending.receipt_key)
            .await?
            .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
            .and_then(|receipt| receipt.finding(unit, claim, evidence))
    {
        return Ok(Comparison::Done(finding, true));
    }
    if let Some(receipt) = claim_reuse::legacy_receipt(llm, retained, unit, claim, evidence).await?
    {
        let saved = serde_json::to_value(&receipt)?;
        if let Some(finding) = receipt.finding(unit, claim, evidence) {
            crate::features::learning::lesson_drafts::record_checkpoint(
                &pending.receipt_key,
                saved,
            )
            .await?;
            return Ok(Comparison::Done(finding, true));
        }
    }
    if evidence.is_empty() {
        let judgment = ClaimJudgment::Judged(JudgeOutcome {
            verdict: ClaimVerdict::Unsupported,
            reason: Some("No relevant saved passages were found for this claim.".into()),
            quote: None,
            confidence: None,
        });
        let (_, _, finding, _) = finish(pending, judgment, None).await?;
        return Ok(Comparison::Done(finding, false));
    }
    Ok(Comparison::Pending(pending))
}
