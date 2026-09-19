//! How much of each fetched web page the prompt can carry.
//!
//! The pipeline used to clip every page to one fixed size. A long recap lost
//! most of its text, the prompt labelled it truncated, and the model — quite
//! reasonably — spent its first round asking for the page again through a tool
//! whose allowance was eight times larger. That round is a full generation; on a
//! remote model it was over two minutes to re-read pages already in hand.
//!
//! The room a page gets is a property of the model's context window, not a
//! constant: a 131k-token model can hold several whole articles, an 8k one
//! cannot hold what the old constant handed it. So the total is taken as a share
//! of the turn's retrieval token budget and divided between the pages.

/// Share of the turn's retrieval token budget that fetched pages may fill. The
/// rest stays with the user's own documents, which the prompt ranks first.
const WEB_PAGE_BUDGET_SHARE: f64 = 0.4;

/// Characters per token, rounded down. English prose tokenizes nearer four, so
/// this under-fills rather than overflows.
const CHARS_PER_TOKEN: usize = 3;

/// The characters of page text a turn with `available_for_rag_tokens` can carry.
pub(super) fn page_budget_chars(available_for_rag_tokens: usize) -> usize {
    let tokens = (available_for_rag_tokens as f64 * WEB_PAGE_BUDGET_SHARE) as usize;
    tokens.saturating_mul(CHARS_PER_TOKEN)
}

/// Split `total` characters between pages of the given lengths.
///
/// Every page is offered an equal share. A page shorter than its share keeps
/// only what it needs and the remainder is offered round again to the pages
/// still wanting more, so a short page never strands budget a long one could
/// use and one long page can never starve the others. No page is given more
/// than `per_page_cap` or more than its own length.
pub(super) fn allocate(lengths: &[usize], total: usize, per_page_cap: usize) -> Vec<usize> {
    let wants: Vec<usize> = lengths
        .iter()
        .map(|length| (*length).min(per_page_cap))
        .collect();
    let mut given = vec![0usize; wants.len()];
    let mut remaining = total;

    loop {
        let wanting = wants
            .iter()
            .zip(&given)
            .filter(|(want, got)| got < want)
            .count();
        if wanting == 0 {
            break;
        }
        let share = remaining / wanting;
        if share == 0 {
            break;
        }
        for (want, got) in wants.iter().zip(given.iter_mut()) {
            let grant = want.saturating_sub(*got).min(share);
            *got += grant;
            remaining -= grant;
        }
    }
    given
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The measured failure: a 3,521-word page and a 1,394-word page on a
    /// 131k-token model were each clipped to 6,000 characters. Both fit whole.
    #[test]
    fn a_large_context_carries_long_pages_whole() {
        let budget = page_budget_chars(98_000);
        let given = allocate(&[21_000, 8_400], budget, 50_000);
        assert_eq!(given, vec![21_000, 8_400]);
    }

    /// The old fixed allowance was 18,000 characters whatever the model. An
    /// 8k-token model has nowhere near that much room.
    #[test]
    fn a_small_context_is_given_less_than_the_old_fixed_allowance() {
        let budget = page_budget_chars(5_900);
        assert!(budget < 18_000, "got {budget}");
        let given = allocate(&[21_000, 8_400, 12_000], budget, 50_000);
        assert!(given.iter().sum::<usize>() <= budget);
    }

    #[test]
    fn what_a_short_page_does_not_need_goes_to_the_long_ones() {
        let given = allocate(&[1_000, 40_000, 40_000], 30_000, 50_000);
        assert_eq!(given, vec![1_000, 14_500, 14_500]);
    }

    #[test]
    fn one_long_page_cannot_starve_the_others() {
        let given = allocate(&[50_000, 5_000, 5_000], 20_000, 50_000);
        assert_eq!(given, vec![10_000, 5_000, 5_000]);
    }

    #[test]
    fn no_page_is_given_more_than_the_per_page_cap() {
        let given = allocate(&[40_000, 40_000], 100_000, 12_000);
        assert_eq!(given, vec![12_000, 12_000]);
    }

    #[test]
    fn the_total_is_never_exceeded() {
        for total in [0, 1, 7, 399, 1_000, 33_333] {
            let given = allocate(&[9_000, 120, 45_000, 3_000], total, 50_000);
            assert!(
                given.iter().sum::<usize>() <= total,
                "total {total} gave {given:?}"
            );
        }
    }

    #[test]
    fn nothing_to_divide_gives_nothing() {
        assert_eq!(allocate(&[5_000, 5_000], 0, 50_000), vec![0, 0]);
        assert_eq!(allocate(&[], 10_000, 50_000), Vec::<usize>::new());
        assert_eq!(page_budget_chars(0), 0);
    }
}
