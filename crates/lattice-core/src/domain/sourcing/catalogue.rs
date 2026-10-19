//! The ids of every finder the desktop registers: its finder catalogue
//! (`finders::registry`) and the Internet Archive's three endpoints.
//!
//! Settings validation reads this list, since a switch for an id nothing
//! registers would never take effect. The descriptors themselves live with
//! their adapters in the desktop crate; its registry test keeps the two in
//! step.

/// Every registered finder's id.
pub const FINDER_IDS: &[&str] = &[
    // Open web engines
    "duckduckgo",
    "bing_html",
    "brave",
    "tavily",
    "exa",
    "searxng",
    "marginalia",
    // Encyclopedia and owner sites
    "wikipedia",
    "wikidata",
    "site_discovery",
    // Code host and registries
    "github",
    "crates_io",
    "pypi",
    "npm",
    "nuget",
    "rubygems",
    "maven_pom",
    "go_module",
    // Scholarly
    "openalex",
    "crossref",
    "semantic_scholar",
    "arxiv",
    "unpaywall",
    "europe_pmc",
    // Standards
    "rfc_index",
    "ietf_datatracker",
    "w3c",
    "whatwg_index",
    "ecma_pattern",
    // Law and government
    "federal_register",
    "legislation_uk",
    "eurlex_celex",
    "govinfo",
    // Community
    "stack_exchange",
    "hn_algolia",
    "discourse_json",
    // Archive
    "wayback_available",
    "wayback_cdx",
    "wayback_replay",
];

/// Whether `id` names a registered finder.
pub fn is_known_finder(id: &str) -> bool {
    FINDER_IDS.contains(&id)
}
