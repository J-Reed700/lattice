//! Settings → Sources: the live re-register from the settings and the
//! keyring, the status over the whole catalogue, and keys that never leave
//! the keyring except for their finder.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::application::contracts::settings::{
    FinderGateDto, FinderNeedDto, KeyServiceDto, SettingsDto, SourcesStatusDto,
};
use lattice_core::application::ports::{
    CredentialsPort, MockSettingsRepository, SettingsRepositoryPort,
};
use lattice_core::application::services::sourcing::{plan, FinderGate};
use lattice_core::domain::sourcing::{
    FinderId, Freshness, HealthState, KeyService, NeedKind, SkipReason, SourceNeed, FINDER_IDS,
};
use lattice_core::shared::error::AppError;
use tempfile::TempDir;

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

/// Where the router puts `id` for a general web search under the finders in
/// force: `Ok(())` when a wave runs it, else the reason it was skipped.
fn routed(web: &WebService, id: &str) -> Result<(), SkipReason> {
    let need = SourceNeed {
        kind: NeedKind::Web,
        query: "rust async".to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    };
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
        serde_json::json!({ "bing_html": { "enabled": false } }),
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
    let bing = status
        .finders
        .iter()
        .find(|finder| finder.id == "bing_html")
        .unwrap();
    assert!(!bing.enabled);
    assert_eq!(bing.gate, Some(FinderGateDto::Disabled));
    assert_eq!(gate_of(&status, "searxng"), None);
    assert_eq!(gate_of(&status, "unpaywall"), None);
    assert_eq!(
        gate_of(&status, "wayback_cdx"),
        Some(FinderGateDto::Disabled)
    );

    let health = f.web.source_finding().health();
    let bing = health
        .iter()
        .find(|finder| finder.id == FinderId::new("bing_html"))
        .unwrap();
    assert_eq!(bing.state, HealthState::Unavailable(SkipReason::Disabled));
}

#[derive(Clone, Default)]
struct Captured(Arc<parking_lot::Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn no_status_or_log_line_carries_a_key() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _logs = tracing::subscriber::set_default(subscriber);

    let f = fixture(SettingsDto::default());
    // Tests running alongside call `reload` too. A callsite first hit on
    // their threads while this subscriber was being installed can keep the
    // "nobody listens" interest it computed then; recompute it now that this
    // thread's subscriber is in place.
    tracing::callsite::rebuild_interest_cache();
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
    let logs = String::from_utf8(captured.0.lock().clone()).unwrap();
    assert!(logs.contains("Source finders registered"), "{logs}");
    assert!(!logs.contains(SECRET), "{logs}");
}
