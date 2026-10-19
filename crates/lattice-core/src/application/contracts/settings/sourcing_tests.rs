//! Sourcing settings: validation, the settings-file round trip, and the
//! Settings → Sources status built from descriptors (design §3.14).

use std::collections::BTreeSet;
use std::time::Duration;

use chrono::{TimeZone, Utc};

use super::super::{
    FinderGateDto, FinderHealthStateDto, FinderNeedDto, FinderSettingDto, KeyServiceDto,
    SettingsCategory, SettingsDto, SourcesStatusDto, SourcingSettingsDto,
};
use crate::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderHealth, FinderId, HealthState,
    Identity, InstanceSetting, KeyPlan, KeyService, NeedKind, Privacy, RatePolicy, Reach, Role,
    WaveSlot, FINDER_IDS,
};

fn descriptor(id: &'static str, reaches: Reach, auth: Auth, privacy: Privacy) -> FinderDescriptor {
    FinderDescriptor {
        id: FinderId::from_static(id),
        name: id,
        finds: &[Role {
            kind: NeedKind::Web,
            wave: WaveSlot::Second,
        }],
        reaches,
        auth,
        privacy,
        cost: Cost::Free,
        rate: RatePolicy {
            min_interval: Duration::from_secs(1),
            quota: None,
            honours_retry_after: true,
            trip_on_first_refusal: false,
            source: "https://example.com/policy",
        },
        supports_scope: true,
        identity: Identity::DeclaredClient,
        sends_to: Destination::Host("example.com"),
    }
}

fn brave() -> FinderDescriptor {
    descriptor(
        "brave",
        Reach::OpenWeb,
        Auth::RequiredKey {
            service: KeyService::Brave,
            plan: KeyPlan::Credits,
        },
        Privacy::IdentifiedByKey { by: "brave.com" },
    )
}

fn healthy(id: &'static str) -> FinderHealth {
    FinderHealth {
        id: FinderId::from_static(id),
        state: HealthState::Healthy,
        calls: 0,
        answered: 0,
        empty: 0,
        failed: 0,
        skipped: 0,
        last_cause: None,
        next_allowed_at: None,
        median_ms: None,
    }
}

#[test]
fn the_defaults_are_valid() {
    assert!(SourcingSettingsDto::default().problems().is_empty());
}

#[test]
fn searxng_must_be_an_http_address() {
    for good in [
        "https://search.example.org",
        "http://searx.example.net:8080/",
    ] {
        let settings = SourcingSettingsDto {
            searxng_url: Some(good.to_string()),
            ..SourcingSettingsDto::default()
        };
        assert!(settings.problems().is_empty(), "{good}");
    }
    for bad in [
        "",
        "search.example.org",
        "ftp://search.example.org",
        "https://",
    ] {
        let settings = SourcingSettingsDto {
            searxng_url: Some(bad.to_string()),
            ..SourcingSettingsDto::default()
        };
        assert_eq!(
            settings.problems(),
            vec!["searxngUrl must be an http:// or https:// address".to_string()],
            "{bad:?}"
        );
    }
}

#[test]
fn a_contact_is_an_email_or_an_http_address() {
    for good in [
        "me@example.org",
        " me@example.org ",
        "https://example.org/about",
    ] {
        let settings = SourcingSettingsDto {
            contact: Some(good.to_string()),
            ..SourcingSettingsDto::default()
        };
        assert!(settings.problems().is_empty(), "{good:?}");
    }
    for bad in [
        "",
        "   ",
        "me",
        "me@",
        "@example.org",
        "me@example",
        "me@.org",
        "me@@example.org",
        "me @example.org",
        "ftp://example.org",
    ] {
        let settings = SourcingSettingsDto {
            contact: Some(bad.to_string()),
            ..SourcingSettingsDto::default()
        };
        assert_eq!(settings.problems().len(), 1, "{bad:?}");
    }
}

#[test]
fn a_switch_names_a_registered_finder_and_a_wave_runs_one() {
    let mut settings = SourcingSettingsDto {
        finder_concurrency: 0,
        ..SourcingSettingsDto::default()
    };
    settings
        .finders
        .insert("brave".to_string(), FinderSettingDto { enabled: false });
    settings.finders.insert(
        "no_such_finder".to_string(),
        FinderSettingDto { enabled: true },
    );
    assert_eq!(
        settings.problems(),
        vec![
            "finders names a finder that does not exist: no_such_finder".to_string(),
            "finderConcurrency must be at least 1".to_string(),
        ]
    );
    assert!(FINDER_IDS.contains(&"brave"));
}

#[test]
fn sourcing_round_trips_through_the_settings_file_and_defaults_when_absent() {
    let mut settings = SettingsDto::default();
    settings.sourcing.searxng_url = Some("https://search.example.org".to_string());
    settings.sourcing.contact = Some("me@example.org".to_string());
    settings.sourcing.archive_fallback = false;
    settings
        .sourcing
        .finders
        .insert("bing_html".to_string(), FinderSettingDto { enabled: false });

    let json = serde_json::to_value(&settings).unwrap();
    assert_eq!(json["sourcing"]["searxngUrl"], "https://search.example.org");
    let back: SettingsDto = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(back.sourcing, settings.sourcing);

    let mut older = json;
    older.as_object_mut().unwrap().remove("sourcing");
    let loaded: SettingsDto = serde_json::from_value(older).unwrap();
    assert_eq!(loaded.sourcing, SourcingSettingsDto::default());
    assert!(loaded.sourcing.archive_fallback);

    assert_eq!(SettingsCategory::Sourcing.as_str(), "sourcing");
    assert_eq!(
        "sourcing".parse::<SettingsCategory>(),
        Ok(SettingsCategory::Sourcing)
    );
    assert!(SettingsCategory::all().contains(&SettingsCategory::Sourcing));
}

#[test]
fn a_stored_key_lifts_the_gate_and_a_switch_reports_disabled() {
    let brave = brave();
    let health = healthy("brave");
    let finders = [(&brave, &health)];
    let mut settings = SourcingSettingsDto::default();

    let without = SourcesStatusDto::new(&finders, &settings, &BTreeSet::new());
    let status = &without.finders[0];
    assert_eq!(status.gate, Some(FinderGateDto::NoKey));
    assert!(status.enabled);
    assert_eq!(
        status.needs,
        FinderNeedDto::RequiredKey {
            service: KeyServiceDto::Brave
        }
    );
    assert_eq!(
        status.disclosure,
        "Your query is sent to brave.com under your key"
    );

    let keys = BTreeSet::from([KeyService::Brave]);
    let with = SourcesStatusDto::new(&finders, &settings, &keys);
    assert_eq!(with.finders[0].gate, None);
    let brave_key = with
        .keys
        .iter()
        .find(|key| key.service == KeyServiceDto::Brave)
        .unwrap();
    assert!(brave_key.stored);
    assert_eq!(brave_key.key_page_url, "https://brave.com/search/api/");

    settings
        .finders
        .insert("brave".to_string(), FinderSettingDto { enabled: false });
    let off = SourcesStatusDto::new(&finders, &settings, &keys);
    assert_eq!(off.finders[0].gate, Some(FinderGateDto::Disabled));
    assert!(!off.finders[0].enabled);
}

#[test]
fn instance_contact_and_archive_gates_are_named() {
    let searxng = descriptor(
        "searxng",
        Reach::OpenWeb,
        Auth::UserInstance {
            setting: InstanceSetting::SearxngUrl,
        },
        Privacy::SentToUserInstance,
    );
    let crossref = descriptor(
        "crossref",
        Reach::Scholarly,
        Auth::Contact,
        Privacy::IdentifiedByContact { by: "Crossref" },
    );
    let wayback = descriptor(
        "wayback_cdx",
        Reach::Archive,
        Auth::None,
        Privacy::QueryLogged {
            by: "the Internet Archive",
        },
    );
    let health = healthy("any");
    let finders = [
        (&searxng, &health),
        (&crossref, &health),
        (&wayback, &health),
    ];
    let settings = SourcingSettingsDto {
        archive_fallback: false,
        ..SourcingSettingsDto::default()
    };
    let status = SourcesStatusDto::new(&finders, &settings, &BTreeSet::new());
    let gates: Vec<_> = status.finders.iter().map(|finder| finder.gate).collect();
    assert_eq!(
        gates,
        vec![
            Some(FinderGateDto::NoInstance),
            Some(FinderGateDto::NoContact),
            Some(FinderGateDto::Disabled),
        ]
    );
    assert_eq!(status.finders[1].needs, FinderNeedDto::Contact);
    assert_eq!(status.finders[0].needs, FinderNeedDto::InstanceUrl);
}

#[test]
fn health_reports_a_pause_and_the_last_error() {
    let brave = brave();
    let until = Utc.with_ymd_and_hms(2026, 10, 18, 14, 2, 0).unwrap();
    let health = FinderHealth {
        state: HealthState::Open {
            until,
            cause: FailureCause::Status {
                code: 429,
                retry_after: Some(Duration::from_secs(60)),
            },
        },
        calls: 3,
        failed: 1,
        last_cause: Some(FailureCause::Status {
            code: 429,
            retry_after: Some(Duration::from_secs(60)),
        }),
        next_allowed_at: Some(until),
        median_ms: Some(1900),
        ..healthy("brave")
    };
    let status = SourcesStatusDto::new(
        &[(&brave, &health)],
        &SourcingSettingsDto::default(),
        &BTreeSet::from([KeyService::Brave]),
    );
    let health = &status.finders[0].health;
    assert_eq!(health.state, FinderHealthStateDto::CoolingDown);
    assert_eq!(health.until, Some(until));
    assert_eq!(
        health.last_error.as_deref(),
        Some("HTTP 429, Retry-After 60 s")
    );
    assert_eq!(health.calls, 3);
    assert_eq!(health.median_ms, Some(1900));
}

#[test]
fn every_key_service_is_listed_with_its_wire_name() {
    let status = SourcesStatusDto::new(&[], &SourcingSettingsDto::default(), &BTreeSet::new());
    assert_eq!(status.keys.len(), KeyService::ALL.len());
    assert!(status.keys.iter().all(|key| !key.stored));
    assert!(status
        .keys
        .iter()
        .all(|key| key.key_page_url.starts_with("https://")));
    let wire: Vec<String> = status
        .keys
        .iter()
        .map(|key| serde_json::to_value(key.service).unwrap().to_string())
        .collect();
    assert_eq!(
        wire,
        vec![
            "\"brave\"",
            "\"tavily\"",
            "\"exa\"",
            "\"github\"",
            "\"semanticScholar\"",
            "\"apiDataGov\"",
            "\"stackExchange\"",
        ]
    );
    for service in KeyService::ALL {
        assert_eq!(KeyService::from(KeyServiceDto::from(service)), service);
    }
}
