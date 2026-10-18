//! EUR-Lex legal document resolver via CELEX numbers.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FinderDescriptor, FinderId, FinderOutcome, Identifier, Identity, Lead,
    LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, rank_of, FinderHttp};

pub(crate) const RESULTS_PER_PAGE: usize = 10;
pub(crate) const ID: FinderId = FinderId::from_static("eurlex_celex");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "EUR-Lex",
    finds: &[Role {
        kind: NeedKind::Law,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Law,
    auth: Auth::None,
    privacy: Privacy::AnonymousQuery,
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: Duration::ZERO,
        quota: None,
        honours_retry_after: false,
        trip_on_first_refusal: false,
        source: "https://eur-lex.europa.eu",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Local,
};

pub(crate) struct EurlexCelex {
    descriptor: FinderDescriptor,
}

impl EurlexCelex {
    pub(crate) fn new(_http: FinderHttp) -> Self {
        Self {
            descriptor: DESCRIPTOR,
        }
    }
}

fn extract_celex(need: &SourceNeed) -> Option<String> {
    if let Some(subject) = &need.subject {
        for id in &subject.identifiers {
            if let Identifier::Celex(celex) = id {
                let trimmed = celex.trim().to_ascii_uppercase();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }

    let re = regex::Regex::new(r"(?i)\b([0-9]{5}[A-Za-z]{1,2}[0-9]{4})\b").ok()?;
    let captures = re.captures(&need.query)?;
    captures.get(1).map(|m| m.as_str().to_ascii_uppercase())
}

#[async_trait]
impl SourceFinder for EurlexCelex {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let Some(celex) = extract_celex(need) else {
            return answered(&self.descriptor, Vec::new(), started);
        };

        let url_str = format!("https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:{celex}");
        let Ok(url) = Url::parse(&url_str) else {
            return answered(&self.descriptor, Vec::new(), started);
        };

        let rank = rank_of(page, RESULTS_PER_PAGE, 0);
        let lead = Lead {
            title: Some(format!("CELEX {celex}")),
            kind: LeadKind::Document(Identifier::Celex(celex)),
            owner: Some(Origin::new("eur-lex.europa.eu", None)),
            ..Lead::page(url, ID, rank)
        };

        answered(&self.descriptor, vec![lead], started)
    }
}
