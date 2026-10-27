//! How much of each fetched web page the prompt can carry: the web pages'
//! share of the turn's evidence budget, divided between the pages, so the room
//! follows the model's context window rather than a constant.

use lattice_core::application::services::context_assembler::{EvidenceBudget, EvidenceShare};

/// The characters of page text the turn's evidence budget lets fetched pages
/// carry, erring short so dense text under-fills rather than overflows.
pub(super) fn page_budget_chars(evidence: &EvidenceBudget) -> usize {
    lattice_core::application::ports::llm_port::chars_within_tokens(
        evidence.allowance(EvidenceShare::WEB_PAGES),
        lattice_core::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
    )
}

/// Splits `total` characters between pages of the given lengths: an equal
/// share each, with what a short page does not need offered round again to the
/// pages still wanting more. No page gets more than its length, and there is
/// no per-page ceiling.
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

/// Splits `total` characters between pages in rank order, best first: each
/// page gets its whole text when it fits what is left, otherwise what is left
/// when that is at least `useful`, otherwise nothing.
pub(in crate::features::conversation::chat) fn by_rank(
    lengths: &[usize],
    total: usize,
    useful: usize,
) -> Vec<usize> {
    let mut remaining = total;
    lengths
        .iter()
        .map(|&length| {
            let grant = if length <= remaining {
                length
            } else if remaining >= useful {
                remaining
            } else {
                0
            };
            remaining -= grant;
            grant
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The best page goes whole, the next whole while it fits, the one after
    /// gets what is left when that is a useful share; a short page further
    /// down still goes whole into what remains.
    #[test]
    fn pages_are_given_room_in_rank_order() {
        assert_eq!(
            by_rank(&[13_000, 9_000, 30_000, 900], 40_000, 8_000),
            vec![13_000, 9_000, 18_000, 0]
        );
        assert_eq!(
            by_rank(&[13_000, 30_000, 900], 20_000, 8_000),
            vec![13_000, 0, 900],
            "7,000 left is not a useful share of a long page"
        );
        assert_eq!(by_rank(&[50_000], 6_000, 6_000), vec![6_000]);
    }

    /// A 3,521-word page and a 1,394-word page both fit whole on a 131k-token
    /// model.
    #[test]
    fn a_large_context_carries_long_pages_whole() {
        let budget = page_budget_chars(&EvidenceBudget::new(98_000));
        let given = allocate(&[21_000, 8_400], budget);
        assert_eq!(given, vec![21_000, 8_400]);
    }

    /// An 8k-token model's pages get only the room its window has.
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

    /// A budget with room carries a long page whole; there is no per-page ceiling.
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
