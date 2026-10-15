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
//! cannot hold what the old constant handed it. So the total is the web pages'
//! share of the turn's evidence budget, divided between the pages.

use lattice_core::application::services::context_assembler::{EvidenceBudget, EvidenceShare};

/// The characters of page text the turn's evidence budget lets fetched pages
/// carry, erring short so dense text under-fills rather than overflows.
pub(super) fn page_budget_chars(evidence: &EvidenceBudget) -> usize {
    lattice_core::application::ports::llm_port::chars_within_tokens(
        evidence.allowance(EvidenceShare::WEB_PAGES),
        lattice_core::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
    )
}

/// Split `total` characters between pages of the given lengths.
///
/// Every page is offered an equal share. A page shorter than its share keeps
/// only what it needs and the remainder is offered round again to the pages
/// still wanting more, so a short page never strands budget a long one could
/// use and one long page can never starve the others. No page is given more
/// than its own length; there is no per-page ceiling, so a budget with room
/// carries a long page whole.
pub(in crate::features::conversation::chat) fn allocate(
    lengths: &[usize],
    total: usize,
) -> Vec<usize> {
    let wants: Vec<usize> = lengths.to_vec();
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
        let budget = page_budget_chars(&EvidenceBudget::new(98_000));
        let given = allocate(&[21_000, 8_400], budget);
        assert_eq!(given, vec![21_000, 8_400]);
    }

    /// The old fixed allowance was 18,000 characters whatever the model. An
    /// 8k-token model has nowhere near that much room.
    #[test]
    fn a_small_context_is_given_less_than_the_old_fixed_allowance() {
        let budget = page_budget_chars(&EvidenceBudget::new(5_900));
        assert!(budget < 18_000, "got {budget}");
        let given = allocate(&[21_000, 8_400, 12_000], budget);
        assert!(given.iter().sum::<usize>() <= budget);
    }

    #[test]
    fn what_a_short_page_does_not_need_goes_to_the_long_ones() {
        let given = allocate(&[1_000, 40_000, 40_000], 30_000);
        assert_eq!(given, vec![1_000, 14_500, 14_500]);
    }

    #[test]
    fn one_long_page_cannot_starve_the_others() {
        let given = allocate(&[50_000, 5_000, 5_000], 20_000);
        assert_eq!(given, vec![10_000, 5_000, 5_000]);
    }

    /// There used to be a 50,000-character ceiling on any one page on top of
    /// the budget. A budget with room now carries a long page whole.
    #[test]
    fn a_page_past_the_old_ceiling_is_carried_whole_when_the_budget_has_room() {
        let given = allocate(&[180_000, 4_000], 200_000);
        assert_eq!(given, vec![180_000, 4_000]);
    }

    /// Without room, the long page is clipped to its share, not dropped, and
    /// the pipeline's prompt says how much of it is shown.
    #[test]
    fn a_page_past_the_budget_is_clipped_to_its_share() {
        let given = allocate(&[180_000, 4_000], 40_000);
        assert_eq!(given, vec![36_000, 4_000]);
    }

    #[test]
    fn the_total_is_never_exceeded() {
        for total in [0, 1, 7, 399, 1_000, 33_333] {
            let given = allocate(&[9_000, 120, 45_000, 3_000], total);
            assert!(
                given.iter().sum::<usize>() <= total,
                "total {total} gave {given:?}"
            );
        }
    }

    #[test]
    fn nothing_to_divide_gives_nothing() {
        assert_eq!(allocate(&[5_000, 5_000], 0), vec![0, 0]);
        assert_eq!(allocate(&[], 10_000), Vec::<usize>::new());
        assert_eq!(page_budget_chars(&EvidenceBudget::default()), 0);
    }
}
