//! Subject owner resolution (Wikidata, package registries, GitHub code forge)
//! and caching (memory and SQLite). Design 2026-10-18 §3.4, §3.7.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use lattice_core::application::ports::Budget;
use lattice_core::application::services::sourcing::cache_policy;
use lattice_core::domain::sourcing::{
    Identifier, Origin, OwnerResolution, OwnerSource, SourcingPolicy, SubjectHint,
};
use lattice_core::shared::persistence::Database;
use url::Url;

use super::owner_cache_repository;
use super::runner::Clock;
use crate::features::web::services::finders::wikidata::Wikidata;
use crate::features::web::services::finders::wikipedia::Wikipedia;

type CachedOwnerEntry = (DateTime<Utc>, Vec<Origin>, Option<OwnerSource>);

pub(crate) struct OwnerResolver {
    wikidata: Arc<Wikidata>,
    _wikipedia: Arc<Wikipedia>,
    cache: parking_lot::Mutex<HashMap<String, CachedOwnerEntry>>,
    database: Option<Database>,
    policy: SourcingPolicy,
    clock: Arc<dyn Clock>,
}

impl OwnerResolver {
    pub(crate) fn new(
        wikidata: Arc<Wikidata>,
        wikipedia: Arc<Wikipedia>,
        database: Option<Database>,
        policy: SourcingPolicy,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            wikidata,
            _wikipedia: wikipedia,
            cache: parking_lot::Mutex::new(HashMap::new()),
            database,
            policy,
            clock,
        }
    }

    pub(crate) fn with_system_clock(
        wikidata: Arc<Wikidata>,
        wikipedia: Arc<Wikipedia>,
        database: Option<Database>,
        policy: SourcingPolicy,
    ) -> Self {
        Self::new(
            wikidata,
            wikipedia,
            database,
            policy,
            Arc::new(super::runner::SystemClock),
        )
    }

    pub(crate) async fn resolve(&self, subject: &SubjectHint, _budget: &Budget) -> OwnerResolution {
        // 1. Direct identifiers short-circuit
        for id in &subject.identifiers {
            match id {
                Identifier::Repo { host, owner, name } => {
                    let origin = Origin::new(host, Some(&format!("{owner}/{name}")));
                    return OwnerResolution {
                        origins: vec![origin],
                        resolved_by: Some(OwnerSource::GitHub),
                        outcomes: Vec::new(),
                    };
                }
                Identifier::Url(url) => {
                    if let Some(origin) = origin_from_url(url) {
                        return OwnerResolution {
                            origins: vec![origin],
                            resolved_by: Some(OwnerSource::Registry),
                            outcomes: Vec::new(),
                        };
                    }
                }
                _ => {}
            }
        }

        let key = cache_policy::owner_key(subject);
        let now = self.clock.now();

        // 2. In-memory cache check
        {
            let cache = self.cache.lock();
            if let Some((stored_at, origins, source)) = cache.get(&key) {
                if cache_policy::is_fresh(*stored_at, now, self.policy.owner_ttl) {
                    return OwnerResolution {
                        origins: origins.clone(),
                        resolved_by: *source,
                        outcomes: Vec::new(),
                    };
                }
            }
        }

        // 3. SQLite persistent cache check
        if let Some(ref database) = self.database {
            if let Ok(Some(row)) = owner_cache_repository::find(database, &key).await {
                let origins_json = row.origins_json;
                let resolved_by_str = row.resolved_by;
                if let Some(stored_at) = DateTime::from_timestamp(row.updated_at, 0) {
                    if cache_policy::is_fresh(stored_at, now, self.policy.owner_ttl) {
                        if let Ok(origin_strs) = serde_json::from_str::<Vec<String>>(&origins_json)
                        {
                            let origins: Vec<Origin> = origin_strs
                                .into_iter()
                                .filter_map(|s| Origin::parse(&s))
                                .collect();
                            let source = match resolved_by_str.as_deref() {
                                Some("wikidata") => Some(OwnerSource::Wikidata),
                                Some("registry") => Some(OwnerSource::Registry),
                                Some("github") => Some(OwnerSource::GitHub),
                                _ => None,
                            };
                            self.cache
                                .lock()
                                .insert(key.clone(), (stored_at, origins.clone(), source));
                            return OwnerResolution {
                                origins,
                                resolved_by: source,
                                outcomes: Vec::new(),
                            };
                        }
                    }
                }
            }
        }

        // 4. Resolve via Wikidata
        let (origins, outcome) = self.wikidata.resolve_subject(subject).await;
        let source = if !origins.is_empty() {
            Some(OwnerSource::Wikidata)
        } else {
            None
        };

        if let Some(source) = source {
            self.cache
                .lock()
                .insert(key.clone(), (now, origins.clone(), Some(source)));
            if let Some(ref database) = self.database {
                let origin_strs: Vec<String> = origins.iter().map(|o| o.to_string()).collect();
                if let Ok(json) = serde_json::to_string(&origin_strs) {
                    let updated_at = now.timestamp();
                    let _ = owner_cache_repository::upsert(
                        database,
                        &key,
                        &subject.name,
                        subject.kind.as_str(),
                        &json,
                        source.as_str(),
                        updated_at,
                    )
                    .await;
                }
            }
        }

        OwnerResolution {
            origins,
            resolved_by: source,
            outcomes: vec![outcome],
        }
    }
}

fn origin_from_url(url: &Url) -> Option<Origin> {
    let host = url.host_str()?;
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "github.com" || host_lower == "gitlab.com" {
        let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
        match segments.as_slice() {
            [owner, repo_raw, ..] => {
                let repo = repo_raw.trim_end_matches(".git");
                if repo.is_empty() {
                    Some(Origin::new(host, Some(owner)))
                } else {
                    Some(Origin::new(host, Some(&format!("{owner}/{repo}"))))
                }
            }
            [owner] => Some(Origin::new(host, Some(owner))),
            [] => Some(Origin::new(host, None)),
        }
    } else {
        Some(Origin::new(host, None))
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
