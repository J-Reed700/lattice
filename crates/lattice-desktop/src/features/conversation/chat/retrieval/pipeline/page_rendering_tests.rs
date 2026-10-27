use super::*;

fn read(clipped_at: Option<usize>) -> PageOutcome {
    PageOutcome::Read(FetchedPage {
        text: "Juliette walks past the hill.".to_string(),
        word_count: 3521,
        clipped_at,
    })
}

/// The old label said only "truncated", or nothing. A model that wants
/// detail reads silence as "there may be more" and fetches the page again.
#[test]
fn a_whole_page_is_called_complete_in_so_many_words() {
    let body = render_page_body(Some(&read(None)));
    assert!(body.contains("3521 words"), "{body}");
    assert!(body.contains("complete"), "{body}");
    assert!(body.contains("nothing more to fetch"), "{body}");
    assert!(body.ends_with("Juliette walks past the hill."), "{body}");
}

#[test]
fn a_clipped_page_says_how_much_is_shown_and_how_to_get_the_rest() {
    let body = render_page_body(Some(&read(Some(12_000))));
    assert!(body.contains("first 12000 characters"), "{body}");
    assert!(body.contains("fetch_url_content"), "{body}");
    assert!(body.contains("offset=12000"), "{body}");
    assert!(!body.contains("complete"), "{body}");
}

/// How many results are opened comes from the page budget and a useful share
/// of a page: a large window opens many, a small one at least one.
#[test]
fn the_pages_read_follow_the_page_budget() {
    assert_eq!(pages_the_budget_carries(0), 0);
    assert_eq!(pages_the_budget_carries(MIN_USEFUL_PAGE_CHARS - 1), 0);
    assert_eq!(pages_the_budget_carries(MIN_USEFUL_PAGE_CHARS * 2), 1);
    assert_eq!(pages_the_budget_carries(USEFUL_PAGE_SHARE_CHARS * 5), 5);
    let large = super::super::page_budget::page_budget_chars(
        &lattice_core::application::services::context_assembler::EvidenceBudget::new(98_000),
    );
    assert!(
        pages_the_budget_carries(large) >= 10,
        "a 131k-class budget has a useful share for every result of a deep search"
    );
}

/// Turning page reading off answers from snippets: no page is opened, however
/// much room the window has.
#[test]
fn with_page_reading_off_no_page_is_opened() {
    let large = MIN_USEFUL_PAGE_CHARS * 20;
    assert_eq!(super::web_pages::pages_wanted(false, large), 0);
    assert_eq!(
        super::web_pages::pages_wanted(true, large),
        pages_the_budget_carries(large)
    );
    assert!(
        lattice_core::application::contracts::settings::RetrievalTuningSettingsDto::default()
            .read_web_pages
    );
}

/// A page that already failed is said so in the prompt, so it is not requested
/// again.
#[test]
fn a_page_that_could_not_be_read_says_why_and_not_to_try_again() {
    let body = render_page_body(Some(&PageOutcome::Unreadable(
        "HTTP 403 Forbidden".to_string(),
    )));
    assert!(body.contains("HTTP 403 Forbidden"), "{body}");
    assert!(body.contains("Do not fetch this URL"), "{body}");
}

#[test]
fn a_result_that_was_never_opened_adds_nothing() {
    assert_eq!(render_page_body(None), "");
}
