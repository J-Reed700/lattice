//! ECMA Standards: matches ECMA standard numbers and keywords to canonical specifications.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FinderDescriptor, FinderId, FinderOutcome, Identity, Lead, LeadKind,
    NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

use super::{answered, rank_of, FinderHttp};
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
pub(crate) const ID: FinderId = FinderId::from_static("ecma_pattern");

static ECMA_RE: Lazy<Regex> = Lazy::new(|| match Regex::new(r"(?i)\becma[\s-]*(\d+)\b") {
    Ok(re) => re,
    Err(_) => match Regex::new("$^") {
        Ok(empty) => empty,
        Err(_) => unreachable!(),
    },
});

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "ECMA Standards",
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
        source: "https://ecma-international.org/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Local,
};

pub(crate) struct EcmaPattern {
    descriptor: FinderDescriptor,
}

impl EcmaPattern {
    pub(crate) fn new(_http: FinderHttp) -> Self {
        Self {
            descriptor: DESCRIPTOR,
        }
    }
}

fn contains_word(text: &str, target: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| w.eq_ignore_ascii_case(target))
}

fn contains_csharp(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("c#") || contains_word(&lower, "csharp")
}

fn extract_ecma_standards(need: &SourceNeed) -> Vec<u32> {
    let mut numbers = Vec::new();

    let add_standard = |num: u32, list: &mut Vec<u32>| {
        if !list.contains(&num) {
            list.push(num);
        }
    };

    let scan_text = |text: &str, list: &mut Vec<u32>| {
        for caps in ECMA_RE.captures_iter(text) {
            if let Some(m) = caps.get(1) {
                if let Ok(n) = m.as_str().parse::<u32>() {
                    add_standard(n, list);
                }
            }
        }

        let lower = text.to_ascii_lowercase();
        if contains_word(&lower, "tc39")
            || contains_word(&lower, "ecmascript")
            || contains_word(&lower, "javascript")
        {
            add_standard(262, list);
        }
        if contains_word(&lower, "intl") {
            add_standard(402, list);
        }
        if contains_word(&lower, "json") {
            add_standard(404, list);
        }
        if contains_csharp(&lower) {
            add_standard(334, list);
        }
        if contains_word(&lower, "cli") || lower.contains("common language infrastructure") {
            add_standard(335, list);
        }
    };

    if let Some(subject) = &need.subject {
        scan_text(&subject.name, &mut numbers);
    }
    scan_text(&need.query, &mut numbers);

    numbers
}

fn standard_info(n: u32) -> (String, String, Origin) {
    if n == 262 {
        (
            "https://tc39.es/ecma262/".to_string(),
            "ECMA-262: ECMAScript Language Specification".to_string(),
            Origin::new("tc39.es", None),
        )
    } else {
        let title = match n {
            402 => "ECMA-402: ECMAScript Internationalization API Specification",
            404 => "ECMA-404: The JSON Data Interchange Syntax",
            334 => "ECMA-334: C# Language Specification",
            335 => "ECMA-335: Common Language Infrastructure (CLI)",
            _ => "",
        };
        let title_str = if title.is_empty() {
            format!("ECMA-{n} Standard")
        } else {
            title.to_string()
        };
        (
            format!(
                "https://ecma-international.org/publications-and-standards/standards/ecma-{n}/"
            ),
            title_str,
            Origin::new("ecma-international.org", None),
        )
    }
}

#[async_trait]
impl SourceFinder for EcmaPattern {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        if page > 1 {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let standards = extract_ecma_standards(need);
        if standards.is_empty() {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let mut leads = Vec::new();
        for n in standards {
            let (url_str, title, owner) = standard_info(n);
            let Ok(url) = Url::parse(&url_str) else {
                continue;
            };

            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                url,
                title: Some(title.clone()),
                snippet: Some(format!("Canonical specification for {title}.")),
                published: None,
                kind: LeadKind::OfficialSite,
                owner: Some(owner),
                rel_canonical: None,
                terms: None,
                found_by: vec![(ID, rank)],
            });
        }

        answered(&self.descriptor, leads, started)
    }
}
