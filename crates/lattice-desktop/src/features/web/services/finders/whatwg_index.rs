//! WHATWG Living Standards: in-memory catalogue of official web platform specifications.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FinderDescriptor, FinderId, FinderOutcome, Identity, Lead, LeadKind,
    NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, rank_of, FinderHttp};
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
pub(crate) const ID: FinderId = FinderId::from_static("whatwg_index");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "WHATWG Living Standards",
    finds: &[Role {
        kind: NeedKind::Standard,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Standards,
    auth: Auth::None,
    privacy: Privacy::AnonymousQuery,
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://spec.whatwg.org/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Local,
};

struct WhatwgStandard {
    short: &'static str,
    title: &'static str,
    keywords: &'static [&'static str],
}

const STANDARDS: &[WhatwgStandard] = &[
    WhatwgStandard {
        short: "html",
        title: "HTML Living Standard",
        keywords: &[
            "html", "html5", "canvas", "video", "audio", "script", "element",
        ],
    },
    WhatwgStandard {
        short: "dom",
        title: "DOM Living Standard",
        keywords: &[
            "dom",
            "document object model",
            "events",
            "node",
            "element",
            "mutation observer",
        ],
    },
    WhatwgStandard {
        short: "fetch",
        title: "Fetch Living Standard",
        keywords: &[
            "fetch", "http", "request", "response", "headers", "cors", "body",
        ],
    },
    WhatwgStandard {
        short: "url",
        title: "URL Living Standard",
        keywords: &[
            "url",
            "uri",
            "percent-encoding",
            "query parameter",
            "domain",
            "host",
        ],
    },
    WhatwgStandard {
        short: "streams",
        title: "Streams Living Standard",
        keywords: &[
            "streams",
            "readable stream",
            "writable stream",
            "transform stream",
        ],
    },
    WhatwgStandard {
        short: "encoding",
        title: "Encoding Living Standard",
        keywords: &[
            "encoding",
            "charset",
            "utf-8",
            "textdecoder",
            "textencoder",
            "bytes",
        ],
    },
    WhatwgStandard {
        short: "websockets",
        title: "WebSockets Living Standard",
        keywords: &["websockets", "websocket", "ws", "wss", "socket"],
    },
    WhatwgStandard {
        short: "storage",
        title: "Storage Living Standard",
        keywords: &[
            "storage",
            "indexeddb",
            "quota",
            "persistence",
            "localstorage",
        ],
    },
    WhatwgStandard {
        short: "fs",
        title: "File System Living Standard",
        keywords: &[
            "fs",
            "file system",
            "filesystem",
            "file",
            "directory",
            "handle",
        ],
    },
    WhatwgStandard {
        short: "infra",
        title: "Infra Living Standard",
        keywords: &[
            "infra",
            "infrastructure",
            "primitive types",
            "algorithms",
            "data structures",
        ],
    },
    WhatwgStandard {
        short: "mimesniff",
        title: "MIME Sniffing Living Standard",
        keywords: &[
            "mimesniff",
            "mime sniffing",
            "mime",
            "content-type",
            "media type",
        ],
    },
    WhatwgStandard {
        short: "console",
        title: "Console Living Standard",
        keywords: &["console", "console.log", "debugging", "logging"],
    },
    WhatwgStandard {
        short: "fullscreen",
        title: "Fullscreen API Living Standard",
        keywords: &["fullscreen", "fullscreen api", "requestfullscreen"],
    },
    WhatwgStandard {
        short: "notifications",
        title: "Notifications API Living Standard",
        keywords: &["notifications", "notification api", "web notification"],
    },
    WhatwgStandard {
        short: "quirks",
        title: "Quirks Mode Living Standard",
        keywords: &["quirks", "quirks mode", "doctype"],
    },
    WhatwgStandard {
        short: "xhr",
        title: "XMLHttpRequest Living Standard",
        keywords: &["xhr", "xmlhttprequest", "ajax"],
    },
    WhatwgStandard {
        short: "compat",
        title: "Compatibility Living Standard",
        keywords: &["compat", "compatibility", "web compat"],
    },
];

pub(crate) struct WhatwgIndex {
    descriptor: FinderDescriptor,
}

impl WhatwgIndex {
    pub(crate) fn new(_http: FinderHttp) -> Self {
        Self {
            descriptor: DESCRIPTOR,
        }
    }
}

fn score_standard(standard: &WhatwgStandard, query_lower: &str, subject_lower: &str) -> u32 {
    let mut score = 0u32;
    let s_lower = standard.short.to_ascii_lowercase();
    let t_lower = standard.title.to_ascii_lowercase();

    let matches_token = |input: &str, token: &str| -> bool {
        input
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == token)
    };

    for input in [query_lower, subject_lower] {
        if input.is_empty() {
            continue;
        }
        if input == s_lower {
            score += 100;
        } else if matches_token(input, &s_lower) {
            score += 50;
        } else if t_lower.contains(input) {
            score += 30;
        }
        for kw in standard.keywords {
            let kw_matched = if kw.contains(' ') {
                input.contains(kw)
            } else {
                matches_token(input, kw)
            };
            if kw_matched {
                score += 20;
            }
        }
    }

    score
}

#[async_trait]
impl SourceFinder for WhatwgIndex {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query_lower = need.query.trim().to_ascii_lowercase();
        let subject_lower = need
            .subject
            .as_ref()
            .map(|s| s.name.trim().to_ascii_lowercase())
            .unwrap_or_default();

        if query_lower.is_empty() && subject_lower.is_empty() {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let mut scored: Vec<(&WhatwgStandard, u32)> = STANDARDS
            .iter()
            .filter_map(|std| {
                let score = score_standard(std, &query_lower, &subject_lower);
                (score > 0).then_some((std, score))
            })
            .collect();

        scored.sort_by_key(|b| std::cmp::Reverse(b.1));

        let offset = page.saturating_sub(1) * RESULTS_PER_PAGE;
        let mut leads = Vec::new();

        for (std, _) in scored.into_iter().skip(offset).take(RESULTS_PER_PAGE) {
            let Ok(url) = Url::parse(&format!("https://{}.spec.whatwg.org/", std.short)) else {
                continue;
            };

            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                url,
                title: Some(std.title.to_string()),
                snippet: Some(format!("WHATWG Living Standard for {}.", std.title)),
                published: None,
                kind: LeadKind::OfficialSite,
                owner: Some(Origin::new("spec.whatwg.org", None)),
                rel_canonical: None,
                terms: None,
                found_by: vec![(ID, rank)],
            });
        }

        answered(&self.descriptor, leads, started)
    }
}
