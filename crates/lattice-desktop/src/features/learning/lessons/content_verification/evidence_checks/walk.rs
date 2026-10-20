//! Pass B, corroboration (course evidence index §5.2–§5.4): the claim read
//! against the course by relevance, one page at a time.
//!
//! A reach is one scope's list for the claim (`ReferenceCollection::
//! claim_list`): the claim and its counter query ranked together, without
//! what the claim was shown, ending at the last passage on the claim's
//! subject. Pages are its consecutive slices, each as large as the page
//! budget (`page_budget`) and as a check alone can hold in the window, a
//! passage never split. A reach whose list has run out gives way to the
//! next in [`REACHES`]; when none is left, the claim goes to the targeted
//! web research of the pass that follows, with nothing more read from the
//! course. A claim research has just found sources for reads only those.
//!
//! Every claim's first page is checked with its pass A comparison (D1); a
//! claim still unsupported reads on, page by page, only while [`B_DEPTH`]
//! says so. The judge's verdict on each page is the only stop: no count, no
//! score and no round that settles nothing ends a walk.
use super::*;
use crate::features::learning::reference_collection::{ClaimList, Scope};
use std::collections::BTreeMap;

/// Where pass B reads, one scope after another (§5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::features::learning) enum Reach {
    /// The source versions the lesson cites, by relevance.
    LessonSources,
    /// The whole course, by relevance.
    Course,
    /// The whole course, by relevance, the lesson's own sources first among
    /// passages of equal fused score.
    CourseLessonFirstOnTies,
    /// The versions research saved for this claim, and nothing else.
    Researched,
}

/// The order pass B reads in: the one line that holds decision D2, pure
/// relevance, until the page-recall eval confirms it.
pub(in crate::features::learning) const REACHES: &[Reach] = &[Reach::Course];

/// The three orders of §5.2 by the page-recall eval's names: pure relevance,
/// the lesson's sources first, and the lesson's sources first on ties. The
/// eval reads them all; the app reads [`REACHES`].
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::features::learning) const ORDERS: [(&str, &[Reach]); 3] = [
    ("relevance", &[Reach::Course]),
    ("lesson-sources", &[Reach::LessonSources, Reach::Course]),
    ("tie-break", &[Reach::CourseLessonFirstOnTies]),
];

/// How far pass B reads a claim its first page leaves unsupported (D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::features::learning) enum Depth {
    /// The first page only; past it the claim goes to web research. Built
    /// when the owner sets [`B_DEPTH`] to it.
    #[cfg_attr(not(test), allow(dead_code))]
    FirstPage,
    /// Page after page until the lists of every reach run out (§5.4).
    ListEnd,
}

/// Decision D5's switch: the list-end rule, until the eval decides.
pub(in crate::features::learning) const B_DEPTH: Depth = Depth::ListEnd;

/// How pass B pages: its reaches in order and how deep it reads.
#[derive(Clone, Copy, Debug)]
pub(in crate::features::learning) struct Paging {
    pub(in crate::features::learning) reaches: &'static [Reach],
    pub(in crate::features::learning) depth: Depth,
}

impl Paging {
    /// The switches as set.
    pub(in crate::features::learning) const SET: Self = Self {
        reaches: REACHES,
        depth: B_DEPTH,
    };
}

/// The tokens a pass B page holds (§5.3): the reading that costs as long as
/// one more answer, at the backend's measured pace, for `answers` per claim.
/// Before any answer is measured the answer is the checker's output cap,
/// the largest one can be; the first measured answer replaces it. `None`
/// without a measured pace: the window alone bounds the page.
pub(super) fn budget(llm: &dyn LLMPort, answers: AnswerSize) -> Option<usize> {
    let answer = answers
        .per_claim()
        .unwrap_or(f64::from(CHECKER_OUTPUT_TOKENS));
    page_budget(llm.pace(), Some(answer))
}

/// What a page's log line and the report say about its budget.
pub(super) fn budget_text(budget: Option<usize>) -> String {
    budget.map_or_else(
        || "page budget unavailable: no measured pace".to_string(),
        |tokens| format!("{tokens} tokens"),
    )
}

/// Whether `page` stays within `budget` tokens of a passage bank and a check
/// of `claim` against it alone fits the window.
pub(in crate::features::learning::lessons::content_verification) fn within_page(
    llm: &dyn LLMPort,
    claim: &str,
    page: &[EvidencePassage],
    budget: Option<usize>,
) -> bool {
    let tokens: usize = page
        .iter()
        .map(|passage| bank_tokens(llm, &passage.text))
        .sum();
    budget.is_none_or(|budget| tokens <= budget) && fits_window(llm, claim, page)
}

/// The next page of `list` for `claim`: from its first passage `shown` does
/// not overlap, the consecutive passages not shown while the page stays
/// [`within_page`]. A passage is never split; the first is taken whatever
/// the budget when a check of it alone fits the window, so every page reads
/// something, and one no check of this model can hold is passed over. Also
/// returns how many were passed over.
pub(in crate::features::learning::lessons::content_verification) fn page_of(
    llm: &dyn LLMPort,
    claim: &str,
    list: &[EvidencePassage],
    shown: &[EvidencePassage],
    budget: Option<usize>,
) -> (Vec<EvidencePassage>, usize) {
    let mut page: Vec<EvidencePassage> = Vec::new();
    let mut passed_over = 0;
    for passage in list
        .iter()
        .filter(|passage| !shown.iter().any(|seen| overlaps(seen, passage)))
    {
        let mut grown = page.clone();
        grown.push(passage.clone());
        if page.is_empty() {
            if !fits_window(llm, claim, &grown) {
                passed_over += 1;
                continue;
            }
        } else if !within_page(llm, claim, &grown, budget) {
            break;
        }
        page = grown;
    }
    (page, passed_over)
}

/// The scope `reach` searches.
fn scope_of(reach: Reach, citations: &Citations, researched: &[String]) -> Scope {
    match reach {
        Reach::LessonSources => Scope::Keys(citations.sources().clone()),
        Reach::Course | Reach::CourseLessonFirstOnTies => Scope::All,
        Reach::Researched => Scope::Keys(researched.iter().cloned().collect()),
    }
}

/// `reach`'s list for `statement`, without what the claim was shown.
async fn list_of(
    references: &ReferenceCollection<'_>,
    statement: &str,
    reach: Reach,
    shown: &[EvidencePassage],
    (citations, researched): (&Citations, &[String]),
) -> Result<ClaimList> {
    let mut list = references
        .claim_list(statement, &scope_of(reach, citations, researched), shown)
        .await?;
    if reach == Reach::CourseLessonFirstOnTies {
        // Only passages of equal fused score trade places: relevance still
        // orders the list.
        for run in list
            .passages
            .chunk_by_mut(|a, b| a.score.total_cmp(&b.score).is_eq())
        {
            run.sort_by_key(|passage| !citations.cites(&passage.source_id));
        }
    }
    Ok(list)
}

/// What one pass B page held, for its round's log line.
pub(in crate::features::learning::lessons::content_verification) struct PageNote {
    pub(in crate::features::learning::lessons::content_verification) reach: Reach,
    pub(in crate::features::learning::lessons::content_verification) page: usize,
    pub(in crate::features::learning::lessons::content_verification) budget: Option<usize>,
    /// The page's last passage is its list's last.
    pub(in crate::features::learning::lessons::content_verification) list_end: bool,
    pub(in crate::features::learning::lessons::content_verification) counter: bool,
    pub(in crate::features::learning::lessons::content_verification) passed_over: usize,
}

/// A claim's walk through pass B: what it was shown, the reaches left and
/// the current reach's list.
pub(super) struct Walk {
    pub(super) ordinal: usize,
    pub(super) unit: usize,
    pub(super) claim: Claim,
    /// The claim as the checker reads it (`as_checked`).
    pub(super) checked: String,
    /// Everything the claim was shown: pass A's passages and every page read.
    pub(super) shown: Vec<EvidencePassage>,
    /// The reaches left, the current one first.
    reaches: Vec<Reach>,
    list: Option<ClaimList>,
    researched: Vec<String>,
    budget: Option<usize>,
    /// The pass B pages read so far.
    pub(super) pages: usize,
}

impl Walk {
    /// A walk from `reach`, the first of `reaches` it has not left, having
    /// read `pages` pages and been shown `shown`. A claim research found
    /// sources for (`researched`) reads only those.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        (ordinal, unit): (usize, usize),
        claim: Claim,
        checked: String,
        shown: Vec<EvidencePassage>,
        (reaches, reach): (&[Reach], Option<Reach>),
        researched: Vec<String>,
        budget: Option<usize>,
        pages: usize,
    ) -> Self {
        let reaches = if researched.is_empty() {
            let from = reach
                .and_then(|reach| reaches.iter().position(|r| *r == reach))
                .unwrap_or_default();
            reaches.get(from..).unwrap_or_default().to_vec()
        } else {
            vec![Reach::Researched]
        };
        Self {
            ordinal,
            unit,
            claim,
            checked,
            shown: shown
                .into_iter()
                .filter(|passage| passage.retrieval_kind != "execution")
                .collect(),
            reaches,
            list: None,
            researched,
            budget,
            pages,
        }
    }

    /// The walk's next page, with what its log line says; `None` once every
    /// reach has run out.
    pub(super) async fn next_page(
        &mut self,
        llm: &dyn LLMPort,
        references: &ReferenceCollection<'_>,
        citations: &Citations,
    ) -> Result<Option<(Vec<EvidencePassage>, PageNote)>> {
        while let Some(&reach) = self.reaches.first() {
            let list = match self.list.take() {
                Some(list) => list,
                None => {
                    list_of(
                        references,
                        &self.claim.statement,
                        reach,
                        &self.shown,
                        (citations, &self.researched),
                    )
                    .await?
                }
            };
            let (page, passed_over) =
                page_of(llm, &self.checked, &list.passages, &self.shown, self.budget);
            if page.is_empty() {
                tracing::info!(
                    claim = self.ordinal,
                    ?reach,
                    pages = self.pages,
                    passed_over,
                    deeper = list.deeper,
                    "Corroboration: this reach's list has run out"
                );
                self.reaches.remove(0);
                continue;
            }
            self.pages += 1;
            let list_end = !list.passages.iter().any(|passage| {
                !self
                    .shown
                    .iter()
                    .chain(&page)
                    .any(|seen| overlaps(seen, passage))
            });
            let note = PageNote {
                reach,
                page: self.pages,
                budget: self.budget,
                list_end,
                counter: list.counter,
                passed_over,
            };
            self.list = Some(list);
            return Ok(Some((page, note)));
        }
        Ok(None)
    }
}

/// One round's log line for one claim (§5.6): the pass, where B read and
/// which page, the passages, their exact tokens, the budget, the fused
/// scores of the page's first and last passage, whether the list's end was
/// reached and whether a counter query's branches were fused.
pub(super) async fn log_page(
    llm: &dyn LLMPort,
    (ordinal, round): (usize, usize),
    pass: Pass,
    page: &[EvidencePassage],
    note: Option<&PageNote>,
    judged: bool,
) {
    let text = page
        .iter()
        .map(|passage| passage.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    // The log's measure only, asked of the backend for a page sent to the
    // judge: a restored page, or a backend that cannot count, leaves the
    // estimate.
    let counted = if judged {
        llm.count_tokens_exact(&text).await.map_err(|error| {
            tracing::debug!(%error, "The backend could not count a claim's page; logging the estimate");
        })
    } else {
        Err(())
    };
    let (tokens, exact) = match counted {
        Ok(tokens) => (tokens, llm.counts_tokens_exactly()),
        Err(()) => (llm.count_tokens(&text), false),
    };
    let scores: Vec<f64> = page
        .iter()
        .filter(|passage| passage.retrieval_kind != CITED)
        .map(|passage| passage.score)
        .collect();
    tracing::info!(
        claim = ordinal,
        round,
        ?pass,
        reach = ?note.map(|note| note.reach),
        page = note.map(|note| note.page),
        passages = page.len(),
        tokens,
        exact,
        budget = %note.map_or_else(|| "none needed".to_string(), |note| budget_text(note.budget)),
        first_fused = ?scores.first(),
        last_fused = ?scores.last(),
        list_end = note.map(|note| note.list_end),
        counter = note.map(|note| note.counter),
        passed_over = note.map(|note| note.passed_over),
        "Claim page"
    );
}

/// Pass B's first page for `statement`, as a claim with no saved selection
/// starts it: the first reach of `reaches` whose list holds anything. The
/// reach it came from is `None` when every list is empty: the claim has
/// nothing on its subject in the course.
#[allow(clippy::too_many_arguments)]
pub(in crate::features::learning::lessons::content_verification) async fn page_one(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    statement: &str,
    checked: &str,
    citations: &Citations,
    shown: &[EvidencePassage],
    reaches: &[Reach],
    budget: Option<usize>,
) -> Result<(Vec<EvidencePassage>, Option<PageNote>)> {
    let claim = Claim {
        quote: statement.into(),
        statement: statement.into(),
    };
    let mut walk = Walk::new(
        (0, 0),
        claim,
        checked.to_owned(),
        shown.to_vec(),
        (reaches, None),
        Vec::new(),
        budget,
        0,
    );
    Ok(match walk.next_page(llm, references, citations).await? {
        Some((page, note)) => (page, Some(note)),
        None => (Vec::new(), None),
    })
}

/// [`page_one`] for a claim of a section that cites nothing, its pages sized
/// as at the start of a job: what a fresh check selects.
#[cfg(test)]
pub(in crate::features::learning::lessons::content_verification) async fn first_page(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    statement: &str,
    citations: &Citations,
    shown: &[EvidencePassage],
) -> Result<Vec<EvidencePassage>> {
    let mut page = page_one(
        llm,
        references,
        statement,
        statement,
        citations,
        shown,
        REACHES,
        budget(llm, AnswerSize::default()),
    )
    .await?
    .0;
    canonicalize_evidence(&mut page);
    Ok(page)
}

/// The checkpoint holding the budget a claim's walk pages by, keyed by the
/// receipt of its first page: the same first page pages the same way on a
/// later pass or after a restart, so every deeper page finds its receipt.
fn walk_key(first_page_receipt: &str) -> String {
    format!("claim-walk-v1:{}", digest(first_page_receipt))
}

/// The budget the walk after `first_page_receipt` pages by: the one it was
/// first given, or `fresh`, saved for next time.
pub(super) async fn walk_budget(
    first_page_receipt: &str,
    fresh: Option<usize>,
) -> Result<Option<usize>> {
    let key = walk_key(first_page_receipt);
    if let Some(saved) = crate::features::learning::lesson_drafts::checkpoint(&key).await? {
        if let Ok(budget) = serde_json::from_value::<Option<usize>>(saved) {
            return Ok(budget);
        }
    }
    crate::features::learning::lesson_drafts::record_checkpoint(&key, json!(fresh)).await?;
    Ok(fresh)
}

/// A walker's decided outcomes, per ordinal: the page's finding, with where
/// it was read.
type Decided = BTreeMap<usize, (Finding, Reach, usize)>;

/// Reads every walker on, a page a round, until its page decides it or its
/// reaches run out. A page's check has its own receipt; a page whose
/// receipt is saved is not checked again. Returns each decided walker's
/// deciding page's finding; a walker whose reaches ran out is not in it.
pub(super) async fn deepen(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    walks: Vec<Walk>,
    citations: &Citations,
    labels: &Labels,
    answers: &mut AnswerSize,
) -> Result<Decided> {
    let mut walks: BTreeMap<usize, Walk> =
        walks.into_iter().map(|walk| (walk.ordinal, walk)).collect();
    let mut decided = Decided::new();
    let mut round = 1;
    while !walks.is_empty() {
        round += 1;
        let mut pending = Vec::new();
        let mut outcomes: Vec<(usize, Finding)> = Vec::new();
        let mut at: HashMap<usize, (Reach, usize)> = HashMap::new();
        for (ordinal, mut walk) in std::mem::take(&mut walks) {
            let Some((mut page, note)) = walk.next_page(llm, references, citations).await? else {
                tracing::info!(
                    claim = ordinal,
                    pages = walk.pages,
                    "Corroboration found nothing more in the course; the claim goes to web research"
                );
                continue;
            };
            canonicalize_evidence(&mut page);
            at.insert(ordinal, (note.reach, note.page));
            let checked = Claim {
                quote: walk.claim.quote.clone(),
                statement: walk.checked.clone(),
            };
            let key = ClaimChecks::key(walk.unit, &checked, &page);
            let receipt_key = claim_receipt_key(llm, &key);
            let restored = crate::features::learning::lesson_drafts::checkpoint(&receipt_key)
                .await?
                .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
                .and_then(|receipt| receipt.finding(walk.unit, &walk.claim, &page));
            log_page(
                llm,
                (ordinal, round),
                Pass::Corroboration,
                &page,
                Some(&note),
                restored.is_none(),
            )
            .await;
            if let Some(finding) = restored {
                outcomes.push((ordinal, finding));
            } else {
                pending.push(PendingClaim {
                    ordinal,
                    key,
                    receipt_key,
                    unit: walk.unit,
                    claim: walk.claim.clone(),
                    checked: walk.checked.clone(),
                    evidence: page,
                    pass: Pass::Corroboration,
                });
            }
            // The page joins what the claim was shown once its check says
            // what the judge saw: a passage cut for size was not shown.
            walks.insert(ordinal, walk);
        }
        if !pending.is_empty() || !outcomes.is_empty() {
            crate::features::learning::lesson_progress::deeper_page(
                round,
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
                "Corroborating {} claims across the course, page {round}",
                pending.len()
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
                        .map(|(_, (ordinal, _, finding, _))| (ordinal, finding)),
                );
            }
        }
        for (ordinal, finding) in outcomes {
            if finding.verdict == ClaimVerdict::Unsupported {
                if let Some(walk) = walks.get_mut(&ordinal) {
                    let before = walk.shown.len();
                    walk.shown.extend(finding.evidence);
                    // A page the judge was shown none of cannot move the
                    // walk on: it would be built again the same.
                    if walk.shown.len() == before {
                        walks.remove(&ordinal);
                    }
                }
                continue;
            }
            walks.remove(&ordinal);
            if let Some((reach, page)) = at.get(&ordinal) {
                decided.insert(ordinal, (finding, *reach, *page));
            }
        }
    }
    crate::features::learning::lesson_progress::deeper_done();
    Ok(decided)
}

/// One page of a claim's pass B walk, as [`walk_pages`] reports it.
#[cfg(test)]
pub(in crate::features::learning) struct WalkedPage {
    pub(in crate::features::learning) reach: Reach,
    pub(in crate::features::learning) passages: Vec<EvidencePassage>,
    pub(in crate::features::learning) list_end: bool,
    pub(in crate::features::learning) counter: bool,
}

/// Every pass B page `statement`'s walk reads, in order, if no page settles
/// it and the judge reads each page whole: from the first page to the end of
/// the last reach's list. `cited` is what the claim's section cites, read by
/// pass A; `lesson` what the lesson cites anywhere. For the page-recall
/// eval, which asks on which page a passage would first be read and what
/// reading a list to its end costs.
#[cfg(test)]
pub(in crate::features::learning) async fn walk_pages(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    (unit, statement): (usize, &str),
    (cited, lesson): (Vec<EvidencePassage>, Vec<EvidencePassage>),
    reaches: &[Reach],
    budget: Option<usize>,
) -> Result<Vec<WalkedPage>> {
    let citations = Citations::new(
        HashMap::from([(unit, cited.clone()), (usize::MAX, lesson)]),
        HashMap::new(),
    );
    let claim = Claim {
        quote: statement.into(),
        statement: statement.into(),
    };
    let mut walk = Walk::new(
        (0, unit),
        claim,
        statement.into(),
        cited,
        (reaches, None),
        Vec::new(),
        budget,
        0,
    );
    let mut pages = Vec::new();
    while let Some((page, note)) = walk.next_page(llm, references, &citations).await? {
        walk.shown.extend(page.iter().cloned());
        pages.push(WalkedPage {
            reach: note.reach,
            passages: page,
            list_end: note.list_end,
            counter: note.counter,
        });
    }
    Ok(pages)
}
