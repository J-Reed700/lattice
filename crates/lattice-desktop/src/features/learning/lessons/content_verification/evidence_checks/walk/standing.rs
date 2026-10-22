//! The standing walk (claim sources design §7.4): a claim the gate left
//! unsupported, insufficient or contradicted reads the course's own
//! highest-standing sources on its subject, the standing scope, before it
//! waits. It pages like pass B (the page budget, the break-even rule past its
//! first page, a receipt per page) over that scope alone.
use super::*;

impl Walk {
    /// A standing walk for the claim at `ordinal`: shown `shown`, reading
    /// `list`, the claim's list over the standing scope's versions
    /// (`scope`), by `budget`. Its first page is read whenever the list holds
    /// a passage on the subject; deeper pages by break-even.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::features::learning::lessons::content_verification) fn standing(
        (ordinal, unit): (usize, usize),
        claim: Claim,
        checked: String,
        shown: Vec<EvidencePassage>,
        scope: HashSet<String>,
        list: ClaimList,
        budget: PageBudget,
    ) -> Self {
        let mut walk = Self::new(
            (ordinal, unit),
            claim,
            checked,
            shown,
            (&[Reach::Standing], None),
            Vec::new(),
            budget,
            0,
        );
        walk.standing = Some(scope);
        walk.list = Some(list);
        walk
    }
}

/// Every page a standing walk over `scope` reads for `statement` if no page
/// settles it, as [`walk_pages`] reports pass B's: the page-recall eval's
/// `standing` order.
#[cfg(test)]
pub(in crate::features::learning) async fn walk_standing_pages(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    (unit, statement): (usize, &str),
    cited: Vec<EvidencePassage>,
    scope: HashSet<String>,
    budget: Option<usize>,
) -> Result<Vec<WalkedPage>> {
    let citations = Citations::new(HashMap::from([(unit, cited.clone())]), HashMap::new());
    let claim = Claim {
        quote: statement.into(),
        statement: statement.into(),
    };
    let mut walk = Walk::new(
        (0, unit),
        claim,
        statement.into(),
        cited,
        (&[Reach::Standing], None),
        Vec::new(),
        PageBudget {
            tokens: budget,
            unavailable: budget.is_none().then(|| "no pace given".into()),
        },
        0,
    );
    walk.standing = Some(scope);
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
