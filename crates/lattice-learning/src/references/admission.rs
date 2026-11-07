//! The admission gate (claim classes design §3.5(4)): what makes a captured
//! page a source. It is a deterministic test, no model: a login, search,
//! error or home page, a directory of links, a page too short to say anything
//! and an abstract page (a pointer to a paper, not the paper) are not
//! sources. It runs on a search result's own facts before the page is
//! fetched, on the captured page before it is saved, and once over every page
//! the library already holds (the sweep).
use lattice_core::domain::provenance::markdown_links;
use std::fmt;
use url::Url;

/// The gate's version: a page the sweep has read under it is not read again.
pub(crate) const POLICY: &str = "admission-v1";

/// Words below which a page holds nothing a statement could rest on.
const MIN_WORDS: usize = 150;

/// Words a scholarly page needs to be the paper and not its abstract.
const MIN_PAPER_WORDS: usize = 600;

/// A site's root, or a page named home or index, is a home page unless it
/// runs this long.
const HOME_WORDS: usize = 1_500;

/// Links, and the share of a page's words that are link text, which make a
/// page a directory.
const DIRECTORY_LINKS: usize = 15;
const DIRECTORY_PERCENT: usize = 45;

/// A title longer than this is a headline, not a notice a site shows.
const NOTICE_WORDS: usize = 8;

/// The opening in which an "Abstract" heading marks an abstract page.
const ABSTRACT_OPENING: usize = 40;

/// Hosts that keep records of papers: a short page there is an abstract.
const SCHOLARLY: [&str; 13] = [
    "arxiv.org",
    "doi.org",
    "dl.acm.org",
    "ieeexplore.ieee.org",
    "link.springer.com",
    "sciencedirect.com",
    "onlinelibrary.wiley.com",
    "tandfonline.com",
    "pubmed.ncbi.nlm.nih.gov",
    "semanticscholar.org",
    "researchgate.net",
    "jstor.org",
    "scopus.com",
];

/// Why a page is not a source, as the sweep's list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    reason: &'static str,
    words: Option<usize>,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.words {
            Some(words) => write!(f, "{}, {words} words", self.reason),
            None => f.write_str(self.reason),
        }
    }
}

/// The page at `url`, looking through an archive's wrapper to the address it
/// holds a copy of.
fn original(url: &str) -> Option<Url> {
    let address = Url::parse(url).ok()?;
    let inner = (address.host_str() == Some("web.archive.org"))
        .then(|| address.path().find("/http"))
        .flatten()
        .and_then(|at| address.path().get(at + 1..))
        .and_then(|inner| Url::parse(inner).ok());
    Some(inner.unwrap_or(address))
}

fn on(host: &str, site: &str) -> bool {
    host.strip_suffix(site)
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
}

fn scholarly(address: &Url) -> bool {
    address
        .host_str()
        .is_some_and(|host| SCHOLARLY.iter().any(|site| on(host, site)))
}

/// What a notice's title says it is: a login, a search, an error or a home
/// page. Only a short title is a notice; a headline that mentions logging in
/// is an article.
fn by_title(title: &str) -> Option<&'static str> {
    let title = title.trim();
    if title.split_whitespace().count() > NOTICE_WORDS {
        return None;
    }
    let lowered = title.to_lowercase();
    let lowered = lowered.trim_matches(|c: char| !c.is_alphanumeric());
    if lazy_regex::regex!(
        r"^(log ?in|sign ?in|sign ?up|single sign[- ]on|institutional access)\b|\b(log ?in|sign ?in|sign ?up|sso)$"
    )
    .is_match(lowered)
    {
        return Some("login page");
    }
    if lazy_regex::regex!(r"\bsearch results?\b|^search\b").is_match(lowered) {
        return Some("search results page");
    }
    if lazy_regex::regex!(
        r"^(sorry|oops|access denied|forbidden|too many requests|just a moment|attention required|are you (a )?(human|robot)|captcha|security check|verify you are|page not found|404|403)\b|^(error|not found)$"
    )
    .is_match(lowered)
    {
        return Some("error page");
    }
    if lazy_regex::regex!(r"^(home|homepage|home ?page|main page|welcome)\b").is_match(lowered) {
        return Some("home page");
    }
    None
}

/// What an address says it is: a login, a search or an error page.
fn by_address(address: &Url) -> Option<&'static str> {
    let host = address.host_str().unwrap_or_default();
    let path = address.path().to_lowercase();
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let named = |names: &[&str]| segments.iter().any(|part| names.contains(part));
    if named(&[
        "login",
        "logon",
        "signin",
        "sign-in",
        "sign_in",
        "sso",
        "shibboleth",
    ]) || ["login.", "idp.", "sso.", "accounts."]
        .iter()
        .any(|start| host.starts_with(start))
    {
        return Some("login page");
    }
    if named(&["sorry", "captcha", "blocked", "denied"]) {
        return Some("error page");
    }
    let searched = address
        .query_pairs()
        .any(|(key, _)| matches!(&*key, "q" | "query" | "search" | "s"));
    if named(&["search"]) || searched || (on(host, "scholar.google.com") && named(&["scholar"])) {
        return Some("search results page");
    }
    None
}

/// Whether the address is its site's front page: the root, or a page named
/// home or index at most one level down.
fn is_home(address: &Url) -> bool {
    if address.query().is_some() {
        return false;
    }
    let path = address.path().to_lowercase();
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    match segments.last() {
        None => true,
        Some(last) => {
            segments.len() <= 2
                && matches!(
                    last.split('.').next(),
                    Some("index" | "home" | "default" | "main" | "welcome")
                )
        }
    }
}

/// Whether an "Abstract" heads the text and nothing of a paper's body
/// follows it.
fn opens_with_abstract(text: &str) -> bool {
    let opening: String = text
        .split_whitespace()
        .take(ABSTRACT_OPENING)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let lowered = text.to_lowercase();
    opening.contains("abstract")
        && !["introduction", "references", "conclusion"]
            .iter()
            .any(|section| lowered.contains(section))
}

/// Whether most of a page is links to other pages.
fn is_directory(text: &str, words: usize) -> bool {
    let links = markdown_links(text);
    let spoken: usize = links
        .iter()
        .map(|link| link.anchor.split_whitespace().count())
        .sum();
    links.len() >= DIRECTORY_LINKS && spoken * 100 >= words * DIRECTORY_PERCENT
}

/// A search result before it is fetched: its title and address are facts the
/// search gave, and a notice page shows itself in them.
pub(crate) fn admit_address(url: &str, title: &str) -> Result<(), Refusal> {
    let refused =
        by_title(title).or_else(|| original(url).and_then(|address| by_address(&address)));
    match refused {
        Some(reason) => Err(Refusal {
            reason,
            words: None,
        }),
        None => Ok(()),
    }
}

/// A captured page, by its address, its title and its text.
pub(crate) fn admit(
    url: &str,
    title: &str,
    text: &str,
) -> Result<(), Refusal> {
    let words = text.split_whitespace().count();
    let refuse = |reason| {
        Err(Refusal {
            reason,
            words: Some(words),
        })
    };
    let address = original(url);
    if let Some(reason) = by_title(title).or_else(|| address.as_ref().and_then(by_address)) {
        return refuse(reason);
    }
    if words < MIN_WORDS {
        return refuse("too short");
    }
    if words < HOME_WORDS && address.as_ref().is_some_and(is_home) {
        return refuse("home page");
    }
    if words < MIN_PAPER_WORDS
        && (address.as_ref().is_some_and(scholarly) || opens_with_abstract(text))
    {
        return refuse("abstract only");
    }
    if is_directory(text, words) {
        return refuse("directory of links");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(count: usize) -> String {
        "word ".repeat(count)
    }

    fn refused(url: &str, title: &str, text: &str) -> String {
        admit(url, title, text).unwrap_err().to_string()
    }

    /// The pages that stood among a course's sources (claim classes design
    /// §3.5(4)), by the size the library recorded for each.
    #[test]
    fn the_pages_that_are_not_sources_are_refused_with_their_reason() {
        assert_eq!(
            refused(
                "https://access.clarivate.com/login",
                "Sign in | Clarivate",
                &words(65)
            ),
            "login page, 65 words"
        );
        assert_eq!(
            refused(
                "https://ieeexplore.ieee.org/Xplore/home.jsp",
                "IEEE Xplore",
                &words(126)
            ),
            "too short, 126 words",
            "the floor comes before the home page"
        );
        assert_eq!(
            refused(
                "https://www.google.com/sorry/index?continue=https://scholar.google.com/",
                "https://scholar.google.com/",
                &words(160)
            ),
            "error page, 160 words"
        );
        assert_eq!(
            refused(
                "https://www.scopus.com/home.uri",
                "Welcome to Scopus",
                &words(162)
            ),
            "home page, 162 words"
        );
        assert_eq!(
            refused(
                "https://web.archive.org/web/20230101000000/https://dl.acm.org/",
                "ACM Digital Library",
                &words(420)
            ),
            "home page, 420 words",
            "an archive's copy is read as the page it copies"
        );
        assert_eq!(
            refused(
                "https://arxiv.org/abs/2401.00001",
                "[2401.00001] A paper",
                &words(297)
            ),
            "abstract only, 297 words"
        );
    }

    #[test]
    fn a_page_that_says_something_is_admitted() {
        let article = format!("# Reading files\n{}", words(1_200));
        assert!(admit(
            "https://docs.python.org/3/library/csv.html",
            "csv",
            &article
        )
        .is_ok());
        // A front page that is the document, a paper in full, a short page
        // with its own reference text, and a headline that mentions logging in.
        assert!(admit("https://example.org/", "Example", &words(2_000)).is_ok());
        assert!(admit("https://arxiv.org/pdf/2401.00001", "A paper", &words(4_000)).is_ok());
        assert!(admit(
            "https://www.rfc-editor.org/rfc/rfc9110#section-9",
            "RFC 9110",
            &words(400)
        )
        .is_ok());
        assert!(admit(
            "https://docs.example.org/linux/ssh",
            "How to log in to Linux with SSH keys",
            &words(500)
        )
        .is_ok());
    }

    #[test]
    fn an_abstract_is_a_pointer_to_the_paper_wherever_it_is_found() {
        let abstract_page = format!("Abstract {}", words(300));
        assert_eq!(
            refused("https://journal.example.org/a1", "A study", &abstract_page),
            "abstract only, 301 words"
        );
        // The same opening in a text that goes on to the paper is the paper.
        let paper = format!("Abstract {} 1. Introduction {}", words(100), words(100));
        assert!(admit("https://journal.example.org/a1", "A study", &paper).is_ok());
    }

    #[test]
    fn a_page_of_links_is_a_directory() {
        let links: String = (0..30)
            .map(|index| format!("[Topic number {index}](https://example.org/topic/{index})\n"))
            .collect();
        let listing = format!("{links}{}", words(60));
        assert_eq!(
            refused("https://example.org/topics/all", "Topics", &listing),
            "directory of links, 150 words"
        );
        // The same links among a page of prose are references.
        let article = format!("{links}{}", words(2_000));
        assert!(admit("https://example.org/topics/all", "Topics", &article).is_ok());
    }

    #[test]
    fn a_search_result_is_judged_by_its_title_and_address_before_it_is_fetched() {
        for (url, title, reason) in [
            ("https://example.org/page", "Login", "login page"),
            (
                "https://example.org/account/signin",
                "Account",
                "login page",
            ),
            (
                "https://example.org/search?q=ocr",
                "Results",
                "search results page",
            ),
            (
                "https://example.org/page",
                "Search results for ocr",
                "search results page",
            ),
            ("https://example.org/sorry", "Blocked", "error page"),
            ("https://example.org/page", "Page not found", "error page"),
            (
                "https://example.org/page",
                "Welcome to the Library",
                "home page",
            ),
        ] {
            assert_eq!(
                admit_address(url, title).unwrap_err().to_string(),
                reason,
                "{url} {title}"
            );
        }
        assert!(admit_address("https://docs.rs/serde/latest/serde/", "serde - Rust").is_ok());
        assert!(admit_address("https://example.org/errors", "Error handling in Rust").is_ok());
    }
}
