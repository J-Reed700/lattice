//! The course's first page for a claim (course evidence index §5.2–§5.4): the
//! claim read against the course by relevance, its first page only. A page is a
//! slice of the claim's ranked list, sized by the page budget and the window; a
//! claim it leaves unsupported goes to web research.
use super::*;
use crate::reference_collection::Scope;

/// Where a page of the course was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reach {
    /// The whole course, by relevance.
    Course,
    /// The versions research saved for this claim, which lead its page.
    Researched,
}

/// The tokens a page of the course holds (§5.3), or why it has no budget and
/// the window alone bounds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::lessons::content_verification) struct PageBudget {
    pub(in crate::lessons::content_verification) tokens: Option<usize>,
    pub(in crate::lessons::content_verification) unavailable: Option<String>,
}

impl PageBudget {
    /// The reading that costs as long as one more answer, at the backend's
    /// measured pace, for `answers` per claim. Without a measured pace, or
    /// before the job has measured an answer, there is none: the window
    /// alone bounds the page, and the log and the report say why.
    pub(in crate::lessons::content_verification) fn of(
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
    pub(in crate::lessons::content_verification) fn text(&self) -> String {
        match (self.tokens, &self.unavailable) {
            (Some(tokens), _) => format!("{tokens} tokens"),
            (None, Some(why)) => format!("page budget unavailable: {why}"),
            (None, None) => "page budget unavailable".into(),
        }
    }
}

/// The tokens `page` holds as a passage bank: what its budget counts.
fn page_tokens(llm: &dyn LLMPort, page: &[EvidencePassage]) -> usize {
    page.iter()
        .map(|passage| bank_tokens(llm, &passage.text))
        .sum()
}

/// Whether `page` stays within `budget` tokens of a passage bank and a check
/// of `claim` against it alone fits the window.
pub(in crate::lessons::content_verification) fn within_page(
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
pub(in crate::lessons::content_verification) fn page_of(
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

/// The first page of the course for `statement`, as a claim with no saved
/// selection starts it: the course's list for it, without what `shown` holds,
/// sliced by `budget` and the window. The reach is `None` when the course has
/// nothing on the claim's subject.
pub(in crate::lessons::content_verification) async fn page_one(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    statement: &str,
    checked: &str,
    shown: &[EvidencePassage],
    budget: &PageBudget,
) -> Result<(Vec<EvidencePassage>, Option<Reach>)> {
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
    Ok((page, Some(Reach::Course)))
}
