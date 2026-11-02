//! Pass B, corroboration (course evidence index §5.2–§5.4): the claim read
//! against the course by relevance, its first page only. A page is a slice of
//! the claim's ranked list, sized by the page budget and the window; a claim
//! it leaves unsupported goes to web research.
use super::*;
use crate::features::learning::reference_collection::Scope;

/// Where pass B read a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::features::learning) enum Reach {
    /// The whole course, by relevance.
    Course,
    /// The versions research saved for this claim, which lead its page.
    Researched,
}

/// The tokens a pass B page holds (§5.3), or why it has no budget and the
/// window alone bounds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::features::learning::lessons::content_verification) struct PageBudget {
    pub(in crate::features::learning::lessons::content_verification) tokens: Option<usize>,
    pub(in crate::features::learning::lessons::content_verification) unavailable: Option<String>,
}

impl PageBudget {
    /// The reading that costs as long as one more answer, at the backend's
    /// measured pace, for `answers` per claim. Without a measured pace, or
    /// before the job has measured an answer, there is none: the window
    /// alone bounds the page, and the log and the report say why.
    pub(in crate::features::learning::lessons::content_verification) fn of(
        llm: &dyn LLMPort,
        answers: AnswerSize,
    ) -> Self {
        let pace = llm.pace();
        let answer = answers.per_claim();
        let unavailable = |why: &str| Self {
            tokens: None,
            unavailable: Some(why.into()),
        };
        match (pace, answer) {
            (None, _) => unavailable("no measured pace"),
            (_, None) => unavailable("no answer measured yet"),
            (pace, answer) => match page_budget(pace, answer) {
                Some(tokens) => Self {
                    tokens: Some(tokens),
                    unavailable: None,
                },
                None => unavailable("no measured pace"),
            },
        }
    }

    /// What a page's log line and the report say about it.
    pub(in crate::features::learning::lessons::content_verification) fn text(&self) -> String {
        match (self.tokens, &self.unavailable) {
            (Some(tokens), _) => format!("{tokens} tokens"),
            (None, Some(why)) => format!("page budget unavailable: {why}"),
            (None, None) => "page budget unavailable".into(),
        }
    }
}

/// How a first page that must be read now can be sized.
#[derive(Clone, Debug)]
pub(in crate::features::learning::lessons::content_verification) enum Sizing {
    /// Before the job has measured an answer: a page that must be sized
    /// waits for the answers of the job's first request.
    Unmeasured,
    /// Sized by this budget.
    Sized(PageBudget),
}

/// The tokens `page` holds as a passage bank: what its budget counts.
fn page_tokens(llm: &dyn LLMPort, page: &[EvidencePassage]) -> usize {
    page.iter()
        .map(|passage| bank_tokens(llm, &passage.text))
        .sum()
}

/// Whether `page` stays within `budget` tokens of a passage bank and a check
/// of `claim` against it alone fits the window.
pub(in crate::features::learning::lessons::content_verification) fn within_page(
    llm: &dyn LLMPort,
    claim: &str,
    page: &[EvidencePassage],
    budget: Option<usize>,
) -> bool {
    let tokens = page_tokens(llm, page);
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

/// What a pass B page held, for its log line.
pub(in crate::features::learning::lessons::content_verification) struct PageNote {
    pub(in crate::features::learning::lessons::content_verification) reach: Reach,
    pub(in crate::features::learning::lessons::content_verification) page: usize,
    pub(in crate::features::learning::lessons::content_verification) budget: PageBudget,
    /// The page's last passage is its list's last.
    pub(in crate::features::learning::lessons::content_verification) list_end: bool,
    pub(in crate::features::learning::lessons::content_verification) counter: bool,
    pub(in crate::features::learning::lessons::content_verification) passed_over: usize,
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
        budget = %note.map_or_else(|| "none needed".to_string(), |note| note.budget.text()),
        first_fused = ?scores.first(),
        last_fused = ?scores.last(),
        list_end = note.map(|note| note.list_end),
        counter = note.map(|note| note.counter),
        passed_over = note.map(|note| note.passed_over),
        "Claim page"
    );
}

/// Pass B's first page for `statement`, as a claim with no saved selection
/// starts it: the course's list for it, without what `shown` holds, sliced by
/// `budget` and the window. The note is `None` when the course has nothing on
/// the claim's subject.
pub(in crate::features::learning::lessons::content_verification) async fn page_one(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    statement: &str,
    checked: &str,
    shown: &[EvidencePassage],
    budget: &PageBudget,
) -> Result<(Vec<EvidencePassage>, Option<PageNote>)> {
    let list = references.claim_list(statement, &Scope::All, shown).await?;
    let (page, passed_over) = page_of(llm, checked, &list.passages, shown, budget.tokens);
    if page.is_empty() {
        tracing::info!(
            passed_over,
            deeper = list.deeper,
            "Corroboration: the course's list has run out"
        );
        return Ok((Vec::new(), None));
    }
    let list_end = !list.passages.iter().any(|passage| {
        !shown
            .iter()
            .chain(&page)
            .any(|seen| overlaps(seen, passage))
    });
    let note = PageNote {
        reach: Reach::Course,
        page: 1,
        budget: budget.clone(),
        list_end,
        counter: list.counter,
        passed_over,
    };
    Ok((page, Some(note)))
}

/// [`page_one`] for a claim of a section that cites nothing, its pages sized
/// as at the start of a job: what a fresh check selects.
#[cfg(test)]
pub(in crate::features::learning::lessons::content_verification) async fn first_page(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    statement: &str,
    shown: &[EvidencePassage],
) -> Result<Vec<EvidencePassage>> {
    let mut page = page_one(
        llm,
        references,
        statement,
        statement,
        shown,
        &PageBudget::of(llm, AnswerSize::default()),
    )
    .await?
    .0;
    canonicalize_evidence(&mut page);
    Ok(page)
}
