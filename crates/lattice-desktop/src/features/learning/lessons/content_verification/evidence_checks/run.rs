//! One pass's claims as their checks finish: what is decided, what lesson
//! progress was told, the request that measures the job's answers, and the
//! walks of the claims still unsupported.
use super::*;

/// The claims of one pass.
#[derive(Default)]
pub(super) struct Run {
    pub(super) results: Vec<CheckedClaim>,
    pub(super) compared: BTreeMap<usize, Compared>,
    /// Claims every comparison of which was restored.
    pub(super) reused: usize,
    /// Final decisions made before this pass: never weighed again.
    pub(super) decided: HashSet<usize>,
    /// Claims lesson progress was told about.
    reported: HashSet<usize>,
    /// Claims decided after a model request.
    pub(super) done: usize,
    /// Claims the model was asked about.
    pub(super) asked: HashSet<usize>,
}

impl Run {
    /// A claim decided without comparing its passes now.
    pub(super) fn complete(&mut self, result: CheckedClaim, final_decision: bool) {
        self.reused += usize::from(result.3);
        if final_decision {
            self.decided.insert(result.0);
        }
        if self.reported.insert(result.0) {
            crate::features::learning::lesson_progress::checked(
                result.3,
                result.2.verdict != ClaimVerdict::Supported,
            );
        }
        self.results.push(result);
    }

    /// Tells lesson progress, once, about each compared claim whose
    /// comparisons are now all done.
    pub(super) fn report_decided(&mut self) {
        for claim in self.compared.values() {
            if self.reported.contains(&claim.ordinal) {
                continue;
            }
            let Some((finding, all_reused)) = claim.decided() else {
                continue;
            };
            self.reported.insert(claim.ordinal);
            self.reused += usize::from(all_reused);
            crate::features::learning::lesson_progress::checked(
                all_reused,
                finding.verdict != ClaimVerdict::Supported,
            );
        }
    }

    /// Records one finished comparison on its claim, and counts the claim
    /// when that was its last.
    pub(super) fn answer(&mut self, pass: Pass, (ordinal, _, finding, _): &CheckedClaim) {
        let Some(claim) = self.compared.get_mut(ordinal) else {
            return;
        };
        claim.answered(pass, finding.clone());
        if self.reported.contains(ordinal) {
            return;
        }
        let Some((decided, all_reused)) = claim.decided() else {
            return;
        };
        self.reported.insert(*ordinal);
        self.reused += usize::from(all_reused);
        self.done += 1;
        crate::features::learning::lesson_progress::checked(
            all_reused,
            decided.verdict != ClaimVerdict::Supported,
        );
    }

    /// The job's first request, which measures what an answer costs before
    /// any pass B page is sized (§5.3): the first request of the pass A
    /// comparisons waiting, packed by section as usual, the rest returned to
    /// their claims to go with pass B. With no pass A waiting, the first
    /// claim waiting for its first page reads it bounded by the window
    /// alone, and is asked about alone. Empty when nothing is left to ask.
    pub(super) async fn measuring_request(
        &mut self,
        context: &Context<'_, '_>,
        labels: &Labels,
    ) -> Result<Vec<PendingClaim>> {
        let attributions: Vec<PendingClaim> = self
            .compared
            .values_mut()
            .filter_map(Compared::send_attribution)
            .collect();
        if !attributions.is_empty() {
            let mut requests = pack(context.llm, attributions, labels).into_iter();
            let first = requests.next().unwrap_or_default();
            for item in requests.flatten() {
                if let Some(claim) = self.compared.get_mut(&item.ordinal) {
                    claim.attribution = Some(Comparison::Pending(item));
                }
            }
            self.asked.extend(first.iter().map(|item| item.ordinal));
            return Ok(first);
        }
        let window = Sizing::Sized(PageBudget::of(context.llm, AnswerSize::default()));
        let waiting: Vec<usize> = self
            .compared
            .values()
            .filter(|claim| claim.corroboration.is_none())
            .map(|claim| claim.ordinal)
            .collect();
        for ordinal in waiting {
            let Some(claim) = self.compared.get_mut(&ordinal) else {
                continue;
            };
            if let Some(finding) = context.corroborate(claim, &window).await? {
                let key = claim.key.clone();
                self.compared.remove(&ordinal);
                self.complete((ordinal, key, finding, true), true);
                continue;
            }
            if let Some(item) = claim.corroboration.as_mut().and_then(Comparison::send) {
                self.asked.insert(ordinal);
                return Ok(vec![item]);
            }
        }
        Ok(Vec::new())
    }

    /// Every claim's finding: each compared claim's decision, with the walks
    /// of those still unsupported read on through pass B (`deepen`) as
    /// `paging` says, each walk paged by the budget it was first given.
    pub(super) async fn finish(
        &mut self,
        context: &Context<'_, '_>,
        labels: &Labels,
        answers: &mut AnswerSize,
    ) -> Result<Vec<CheckedClaim>> {
        let llm = context.llm;
        let mut results = std::mem::take(&mut self.results);
        let mut walks = Vec::new();
        let mut attributions: HashMap<usize, (Option<Finding>, Vec<String>)> = HashMap::new();
        for claim in std::mem::take(&mut self.compared).into_values() {
            let Some((finding, all_reused)) = claim.decided() else {
                return Err(invalid(
                    "A claim check finished without its finding; no lesson was published.",
                ));
            };
            if context.paging.depth == Depth::ListEnd
                && finding.verdict == ClaimVerdict::Unsupported
            {
                let shown: Vec<EvidencePassage> = claim
                    .attribution
                    .iter()
                    .chain(claim.corroboration.iter())
                    .filter_map(Comparison::done)
                    .flat_map(|(finding, _)| finding.evidence.clone())
                    .collect();
                walks.push(Walk::new(
                    (claim.ordinal, claim.unit),
                    claim.claim.clone(),
                    claim.checked.clone(),
                    shown,
                    (context.paging.reaches, claim.reach),
                    claim.researched.clone(),
                    walk_budget(&claim.first_receipt, PageBudget::of(llm, *answers)).await?,
                    1,
                ));
                attributions.insert(
                    claim.ordinal,
                    (
                        claim
                            .attribution
                            .as_ref()
                            .and_then(Comparison::done)
                            .map(|(finding, _)| finding.clone()),
                        claim.cited.clone(),
                    ),
                );
            }
            results.push((claim.ordinal, claim.key, finding, all_reused));
        }
        if !walks.is_empty() {
            let found = deepen(
                llm,
                context.references,
                walks,
                context.citations,
                labels,
                answers,
            )
            .await?;
            for (ordinal, (finding, reach, page)) in found {
                let Some((attribution, cited)) = attributions.remove(&ordinal) else {
                    continue;
                };
                if let Some(result) = results.iter_mut().find(|result| result.0 == ordinal) {
                    result.2 = combine(attribution, finding, (Some(reach), page), &cited);
                }
            }
        }
        Ok(results)
    }
}
