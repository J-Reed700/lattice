//! Settings → Sources: the live re-register from the settings and the
//! keyring, the status over the whole catalogue, and keys that never leave
//! the keyring except for their finder.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use lattice_core::application::contracts::settings::{
    FinderGateDto, FinderNeedDto, KeyServiceDto, SettingsDto, SourcesStatusDto,
};
use lattice_core::application::ports::{
    Budget, CredentialsPort, MockSettingsRepository, SettingsRepositoryPort,
};
use lattice_core::application::services::sourcing::{plan, FinderGate};
use lattice_core::domain::sourcing::{
    FailureCause, FinderId, FinderOutcome, FinderResult, Freshness, HealthState, KeyService,
    NeedKind, SkipReason, SourceNeed, FINDER_IDS,
};
use lattice_core::shared::error::AppError;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{reload, remove_key, save_key};
use crate::features::web::services::web::WebService;

const SECRET: &str = "BSA-test-key-0f3c9a7e";

/// The keyring, in memory.
#[derive(Default)]
struct FakeKeyring {
    keys: parking_lot::Mutex<HashMap<String, String>>,
}

#[async_trait]
impl CredentialsPort for FakeKeyring {
    async fn set_api_key(&self, service: &str, key: &str) -> Result<(), AppError> {
        self.keys
            .lock()
            .insert(service.to_string(), key.to_string());
        Ok(())
    }
    async fn get_api_key(&self, service: &str) -> Result<Option<String>, AppError> {
        Ok(self.keys.lock().get(service).cloned())
    }
    async fn delete_api_key(&self, service: &str) -> Result<(), AppError> {
        self.keys.lock().remove(service);
        Ok(())
    }
    async fn has_api_key(&self, service: &str) -> Result<bool, AppError> {
        Ok(self.keys.lock().contains_key(service))
    }
    async fn clear_all_credentials(&self) -> Result<(), AppError> {
        self.keys.lock().clear();
        Ok(())
    }
    async fn set_custom_endpoint(&self, _endpoint: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_custom_endpoint(&self) -> Result<Option<String>, AppError> {
        Ok(None)
    }
}

struct Fixture {
    _dir: TempDir,
    web: WebService,
    settings: MockSettingsRepository,
    keyring: FakeKeyring,
}

fn fixture(settings: SettingsDto) -> Fixture {
    let dir = TempDir::new().unwrap();
    let web = WebService::new(dir.path()).unwrap();
    Fixture {
        _dir: dir,
        web,
        settings: MockSettingsRepository::with_settings(settings),
        keyring: FakeKeyring::default(),
    }
}

fn gate_of(status: &SourcesStatusDto, id: &str) -> Option<FinderGateDto> {
    status
        .finders
        .iter()
        .find(|finder| finder.id == id)
        .unwrap()
        .gate
}

fn web_need() -> SourceNeed {
    SourceNeed {
        kind: NeedKind::Web,
        query: "rust async".to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

fn budget() -> Budget {
    Budget {
        deadline: Instant::now() + Duration::from_secs(60),
        stop: CancellationToken::new(),
        requests: None,
    }
}

/// Where the router puts `id` for a general web search under the finders in
/// force: `Ok(())` when a wave runs it, else the reason it was skipped.
fn routed(web: &WebService, id: &str) -> Result<(), SkipReason> {
    let need = web_need();
    let lineup = web.sourcing_lineup();
    let descriptors: Vec<_> = lineup.descriptors().into_iter().cloned().collect();
    let refused = BTreeMap::new();
    let gate = FinderGate {
        settings: &lineup.settings,
        keys: &lineup.keys,
        refused: &refused,
    };
    let target = FinderId::new(id);
    for wave in plan(&need, &descriptors, &gate).waves {
        if wave.steps.iter().any(|step| step.finder == target) {
            return Ok(());
        }
        if let Some((_, reason)) = wave
            .skipped
            .into_iter()
            .find(|(finder, _)| *finder == target)
        {
            return Err(reason);
        }
    }
    Err(SkipReason::NotApplicable)
}

#[tokio::test]
async fn the_status_lists_every_catalogue_finder_with_its_disclosure() {
    let f = fixture(SettingsDto::default());
    reload(&f.web, &f.settings, &f.keyring).await;
    let status = f.web.sources_status();

    let listed: BTreeSet<&str> = status
        .finders
        .iter()
        .map(|finder| finder.id.as_str())
        .collect();
    let catalogue: BTreeSet<&str> = FINDER_IDS.iter().copied().collect();
    assert_eq!(listed, catalogue);
    assert_eq!(status.finders.len(), FINDER_IDS.len(), "each finder once");
    assert!(status
        .finders
        .iter()
        .all(|finder| !finder.disclosure.is_empty() && !finder.name.is_empty()));
    assert_eq!(status.keys.len(), KeyService::ALL.len());

    let brave = status
        .finders
        .iter()
        .find(|finder| finder.id == "brave")
        .unwrap();
    assert_eq!(
        brave.needs,
        FinderNeedDto::RequiredKey {
            service: KeyServiceDto::Brave
        }
    );
    assert_eq!(gate_of(&status, "searxng"), Some(FinderGateDto::NoInstance));
    assert_eq!(
        gate_of(&status, "unpaywall"),
        Some(FinderGateDto::NoContact)
    );
    assert_eq!(gate_of(&status, "duckduckgo"), None);
    assert_eq!(gate_of(&status, "wayback_cdx"), None);
}

#[tokio::test]
async fn wikibooks_is_on_by_default_and_joins_a_general_web_search() {
    let f = fixture(SettingsDto::default());
    reload(&f.web, &f.settings, &f.keyring).await;
    let status = f.web.sources_status();
    assert_eq!(gate_of(&status, "wikibooks"), None);
    let wikibooks = status
        .finders
        .iter()
        .find(|finder| finder.id == "wikibooks")
        .unwrap();
    assert_eq!(wikibooks.needs, FinderNeedDto::Nothing);
    assert_eq!(routed(&f.web, "wikibooks"), Ok(()));
}

#[tokio::test]
async fn a_stored_key_makes_its_finder_eligible_live_and_removing_it_skips_it_again() {
    let f = fixture(SettingsDto::default());
    reload(&f.web, &f.settings, &f.keyring).await;
    assert_eq!(
        gate_of(&f.web.sources_status(), "brave"),
        Some(FinderGateDto::NoKey)
    );
    assert_eq!(
        routed(&f.web, "brave"),
        Err(SkipReason::NoKey {
            service: KeyService::Brave
        })
    );

    let status = save_key(
        &f.web,
        &f.settings,
        &f.keyring,
        KeyServiceDto::Brave,
        SECRET,
    )
    .await
    .unwrap();
    assert_eq!(gate_of(&status, "brave"), None);
    assert!(status
        .keys
        .iter()
        .any(|key| key.service == KeyServiceDto::Brave && key.stored));
    assert_eq!(routed(&f.web, "brave"), Ok(()));
    assert_eq!(
        f.keyring.get_api_key("brave").await.unwrap().as_deref(),
        Some(SECRET)
    );

    let status = remove_key(&f.web, &f.settings, &f.keyring, KeyServiceDto::Brave)
        .await
        .unwrap();
    assert_eq!(gate_of(&status, "brave"), Some(FinderGateDto::NoKey));
    assert!(status.keys.iter().all(|key| !key.stored));
    assert_eq!(
        routed(&f.web, "brave"),
        Err(SkipReason::NoKey {
            service: KeyService::Brave
        })
    );
}

#[tokio::test]
async fn a_blank_key_is_refused_without_touching_the_keyring() {
    let f = fixture(SettingsDto::default());
    let error = save_key(&f.web, &f.settings, &f.keyring, KeyServiceDto::Exa, "   ")
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidInput(_)), "{error}");
    assert!(f.keyring.keys.lock().is_empty());
}

#[tokio::test]
async fn switches_instance_contact_and_archive_apply_on_reload() {
    let f = fixture(SettingsDto::default());
    reload(&f.web, &f.settings, &f.keyring).await;

    let mut updates = HashMap::new();
    updates.insert(
        "finders".to_string(),
        serde_json::json!({ "marginalia": { "enabled": false } }),
    );
    updates.insert(
        "searxngUrl".to_string(),
        serde_json::json!("https://search.example.org"),
    );
    updates.insert("contact".to_string(), serde_json::json!("me@example.org"));
    updates.insert("archiveFallback".to_string(), serde_json::json!(false));
    f.settings
        .update(
            Some(lattice_core::application::contracts::settings::SettingsCategory::Sourcing),
            updates,
        )
        .await
        .unwrap();
    reload(&f.web, &f.settings, &f.keyring).await;

    let status = f.web.sources_status();
    let marginalia = status
        .finders
        .iter()
        .find(|finder| finder.id == "marginalia")
        .unwrap();
    assert!(!marginalia.enabled);
    assert_eq!(marginalia.gate, Some(FinderGateDto::Disabled));
    assert_eq!(gate_of(&status, "searxng"), None);
    assert_eq!(gate_of(&status, "unpaywall"), None);
    assert_eq!(
        gate_of(&status, "wayback_cdx"),
        Some(FinderGateDto::Disabled)
    );

    let health = f.web.source_finding().health();
    let marginalia = health
        .iter()
        .find(|finder| finder.id == FinderId::new("marginalia"))
        .unwrap();
    assert_eq!(
        marginalia.state,
        HealthState::Unavailable(SkipReason::Disabled)
    );
}

#[tokio::test]
async fn a_url_contact_leaves_unpaywall_off_and_says_it_needs_an_email() {
    let f = fixture(SettingsDto::default());
    let mut updates = HashMap::new();
    updates.insert(
        "contact".to_string(),
        serde_json::json!("https://example.org/about"),
    );
    f.settings
        .update(
            Some(lattice_core::application::contracts::settings::SettingsCategory::Sourcing),
            updates,
        )
        .await
        .unwrap();
    reload(&f.web, &f.settings, &f.keyring).await;

    let status = f.web.sources_status();
    assert_eq!(gate_of(&status, "unpaywall"), Some(FinderGateDto::NoEmail));

    let health = f.web.source_finding().health();
    let unpaywall = health
        .iter()
        .find(|finder| finder.id == FinderId::new("unpaywall"))
        .unwrap();
    assert_eq!(
        unpaywall.state,
        HealthState::Unavailable(SkipReason::NoEmail)
    );
}

#[tokio::test]
async fn no_status_or_log_line_carries_a_key() {
    let logs = lattice_core::shared::testing::logs::capture_logs(tracing::Level::TRACE);

    let f = fixture(SettingsDto::default());
    let saved = save_key(
        &f.web,
        &f.settings,
        &f.keyring,
        KeyServiceDto::Brave,
        SECRET,
    )
    .await
    .unwrap();
    reload(&f.web, &f.settings, &f.keyring).await;
    let status = f.web.sources_status();
    let removed = remove_key(&f.web, &f.settings, &f.keyring, KeyServiceDto::Brave)
        .await
        .unwrap();

    for dto in [&saved, &status, &removed] {
        let json = serde_json::to_string(dto).unwrap();
        assert!(!json.contains(SECRET), "{json}");
    }
    let logs = logs.text();
    assert!(logs.contains("Source finders registered"), "{logs}");
    assert!(!logs.contains(SECRET), "{logs}");
}

/// Settings naming a SearXNG instance on this machine, by name, so the
/// public client's resolver would refuse it.
fn with_local_searxng(server: &MockServer) -> SettingsDto {
    let mut settings = SettingsDto::default();
    settings.sourcing.searxng_url = Some(format!("http://localhost:{}", server.address().port()));
    settings
}

/// One page from the SearXNG finder in force.
async fn searxng_page(web: &WebService) -> FinderOutcome {
    let lineup = web.sourcing_lineup();
    let entry = lineup
        .registry
        .iter()
        .find(|entry| entry.descriptor().id == FinderId::new("searxng"))
        .unwrap();
    entry.finder.find(&web_need(), 1, &budget()).await
}

#[tokio::test]
async fn searxng_reaches_the_users_own_server_and_no_other_finder_does() {
    let server = MockServer::start().await;
    Mock::given(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "url": "https://example.org/a", "title": "A", "content": "a" }]
        })))
        .mount(&server)
        .await;
    let f = fixture(with_local_searxng(&server));
    reload(&f.web, &f.settings, &f.keyring).await;

    let outcome = searxng_page(&f.web).await;
    assert_eq!(
        outcome.result,
        FinderResult::Answered,
        "{:?}",
        outcome.result
    );
    assert_eq!(outcome.leads.len(), 1);

    // Following a page's links uses the public client every other finder
    // shares: the same server, by the same name, is refused unconnected.
    let page = Url::parse(&format!(
        "http://localhost:{}/page",
        server.address().port()
    ))
    .unwrap();
    let refused = f
        .web
        .source_finding()
        .leads_from_page(&page, None, budget())
        .await;
    assert!(refused.is_err());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/search");
}

#[tokio::test]
async fn searxng_follows_no_redirect() {
    let server = MockServer::start().await;
    Mock::given(path("/search"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/elsewhere", server.uri()).as_str()),
        )
        .mount(&server)
        .await;
    let f = fixture(with_local_searxng(&server));
    reload(&f.web, &f.settings, &f.keyring).await;

    let outcome = searxng_page(&f.web).await;
    assert!(
        matches!(
            outcome.result,
            FinderResult::Failed(FailureCause::Status { code: 302, .. })
        ),
        "{:?}",
        outcome.result
    );
    let requests = server.received_requests().await.unwrap();
    assert!(requests
        .iter()
        .all(|request| request.url.path() == "/search"));
}
