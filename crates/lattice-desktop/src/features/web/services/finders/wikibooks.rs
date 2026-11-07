//! English Wikibooks' search: teaching material on a MediaWiki wiki, asked
//! as a declared client exactly as Wikipedia is (`mediawiki`).

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FinderDescriptor, FinderId, FinderOutcome, Identity, NeedKind,
    Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};

use super::mediawiki;
use super::FinderHttp;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) use mediawiki::RESULTS_PER_PAGE;

const API: &str = "https://en.wikibooks.org/w/api.php";
const ARTICLE_BASE: &str = "https://en.wikibooks.org/wiki/";

pub(crate) const ID: FinderId = FinderId::from_static("wikibooks");

/// The same etiquette as Wikipedia's: requests in series, which the per-host
/// pacer guarantees, and `Retry-After` honoured. It joins the general web
/// search beside the engines.
pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Wikibooks",
    finds: &[Role {
        kind: NeedKind::Web,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Encyclopedia,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "the Wikimedia Foundation",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://www.mediawiki.org/wiki/API:Etiquette",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("en.wikibooks.org"),
};

pub(crate) struct Wikibooks {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Wikibooks {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self::with_api(http, API.to_string())
    }

    /// The finder against another API root (a fixture server in tests).
    pub(crate) fn with_api(http: FinderHttp, api: String) -> Self {
        Self {
            http,
            api,
            descriptor: DESCRIPTOR,
        }
    }
}

#[async_trait]
impl SourceFinder for Wikibooks {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        mediawiki::Search {
            http: &self.http,
            api: &self.api,
            article_base: ARTICLE_BASE,
            descriptor: &self.descriptor,
        }
        .run(&need.query, page)
        .await
    }
}
