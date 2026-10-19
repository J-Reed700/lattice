//! Routing emits §3.4's waves for each kind over a registry shaped like the
//! finder catalogue (§3.10), and gates finders honestly.

#![cfg(test)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use chrono::{NaiveDate, TimeZone, Utc};

use super::*;
use crate::domain::sourcing::{
    Auth, Cost, Destination, Ecosystem, FailureCause, Identifier, Identity, InstanceSetting,
    KeyPlan, Origin, Privacy, RatePolicy, Role, SubjectHint, SubjectKind,
};

const fn role(kind: NeedKind, wave: WaveSlot) -> Role {
    Role { kind, wave }
}

use crate::domain::sourcing::NeedKind as K;
use crate::domain::sourcing::WaveSlot::{First, Second, Third};

const WEB_FIRST: &[Role] = &[role(K::Web, First)];
const WEB_SECOND: &[Role] = &[role(K::Web, Second)];
const EXA: &[Role] = &[role(K::Web, Second), role(K::Paper, Third)];
const ENCYCLOPEDIA_FIRST: &[Role] = &[role(K::Encyclopedia, First)];
const ENCYCLOPEDIA_SECOND: &[Role] = &[role(K::Encyclopedia, Second)];
const SITE_DISCOVERY: &[Role] = &[role(K::OfficialDocs, First), role(K::Package, Second)];
const PACKAGE_FIRST: &[Role] = &[role(K::Package, First)];
const SCHOLARLY_FIRST: &[Role] = &[role(K::Paper, First), role(K::Leads, Second)];
const PAPER_SECOND: &[Role] = &[role(K::Paper, Second)];
const PAPER_THIRD: &[Role] = &[role(K::Paper, Third)];
const STANDARD_FIRST: &[Role] = &[role(K::Standard, First)];
const LAW_FIRST: &[Role] = &[role(K::Law, First)];
const LAW_SECOND: &[Role] = &[role(K::Law, Second)];
const COMMUNITY_FIRST: &[Role] = &[role(K::Community, First)];
const COMMUNITY_SECOND: &[Role] = &[role(K::Community, Second)];
const ARCHIVE_FIRST: &[Role] = &[role(K::Archive, First)];
const ARCHIVE_SECOND: &[Role] = &[role(K::Archive, Second)];
const LEADS_FIRST: &[Role] = &[role(K::Leads, First)];

fn finder(
    id: &'static str,
    finds: &'static [Role],
    reaches: Reach,
    auth: Auth,
    supports_scope: bool,
) -> FinderDescriptor {
    FinderDescriptor {
        id: FinderId::from_static(id),
        name: id,
        finds,
        reaches,
        auth,
        privacy: Privacy::QueryLogged { by: "test" },
        cost: Cost::Free,
        rate: RatePolicy {
            min_interval: Duration::from_secs(1),
            quota: None,
            honours_retry_after: true,
            trip_on_first_refusal: false,
            source: "https://example.com/policy",
        },
        supports_scope,
        identity: Identity::DeclaredClient,
        sends_to: Destination::Host("example.com"),
    }
}

fn key(service: KeyService) -> Auth {
    Auth::RequiredKey {
        service,
        plan: KeyPlan::Credits,
    }
}

/// The catalogue's shape: ids, roles, reach and auth as §3.10 lists them.
fn registry() -> Vec<FinderDescriptor> {
    use crate::domain::sourcing::Reach::*;
    vec![
        finder("duckduckgo", WEB_FIRST, OpenWeb, Auth::None, true),
        finder("bing_html", WEB_FIRST, OpenWeb, Auth::None, true),
        finder("brave", WEB_SECOND, OpenWeb, key(KeyService::Brave), true),
        finder("tavily", WEB_SECOND, OpenWeb, key(KeyService::Tavily), true),
        finder("exa", EXA, OpenWeb, key(KeyService::Exa), true),
        finder(
            "searxng",
            WEB_SECOND,
            OpenWeb,
            Auth::UserInstance {
                setting: InstanceSetting::SearxngUrl,
            },
            true,
        ),
        finder(
            "wikipedia",
            ENCYCLOPEDIA_FIRST,
            Encyclopedia,
            Auth::None,
            false,
        ),
        finder(
            "wikidata",
            ENCYCLOPEDIA_SECOND,
            Encyclopedia,
            Auth::None,
            false,
        ),
        finder(
            "site_discovery",
            SITE_DISCOVERY,
            OwnerSites,
            Auth::None,
            true,
        ),
        finder(
            "github",
            PACKAGE_FIRST,
            CodeHost,
            Auth::OptionalKey {
                service: KeyService::GitHub,
            },
            false,
        ),
        finder(
            "crates_io",
            PACKAGE_FIRST,
            Registry(Ecosystem::Crates),
            Auth::None,
            false,
        ),
        finder(
            "npm",
            PACKAGE_FIRST,
            Registry(Ecosystem::Npm),
            Auth::None,
            false,
        ),
        finder("openalex", SCHOLARLY_FIRST, Scholarly, Auth::None, false),
        finder("crossref", SCHOLARLY_FIRST, Scholarly, Auth::None, false),
        finder(
            "semantic_scholar",
            PAPER_SECOND,
            Scholarly,
            Auth::OptionalKey {
                service: KeyService::SemanticScholar,
            },
            false,
        ),
        finder("arxiv", PAPER_SECOND, Scholarly, Auth::None, false),
        finder("unpaywall", PAPER_THIRD, Scholarly, Auth::Contact, false),
        finder("rfc_index", STANDARD_FIRST, Standards, Auth::None, false),
        finder("w3c", STANDARD_FIRST, Standards, Auth::None, false),
        finder("federal_register", LAW_FIRST, Law, Auth::None, false),
        finder(
            "govinfo",
            LAW_SECOND,
            Law,
            Auth::RequiredKey {
                service: KeyService::ApiDataGov,
                plan: KeyPlan::Free,
            },
            false,
        ),
        finder(
            "stack_exchange",
            COMMUNITY_FIRST,
            Community,
            Auth::None,
            false,
        ),
        finder("hn_algolia", COMMUNITY_FIRST, Community, Auth::None, false),
        finder(
            "discourse_json",
            COMMUNITY_SECOND,
            Community,
            Auth::None,
            false,
        ),
        finder("wayback", ARCHIVE_FIRST, Archive, Auth::None, false),
        finder(
            "common_crawl_index",
            ARCHIVE_SECOND,
            Archive,
            Auth::None,
            false,
        ),
        finder("page_links", LEADS_FIRST, LinkedPages, Auth::None, false),
    ]
}

fn need(kind: NeedKind) -> SourceNeed {
    SourceNeed {
        kind,
        query: "q".into(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

fn with_subject(mut need: SourceNeed, name: &str, identifiers: Vec<Identifier>) -> SourceNeed {
    need.subject = Some(SubjectHint {
        name: name.into(),
        kind: SubjectKind::Software,
        identifiers,
    });
    need
}

/// Each wave as (steps, skipped, reformulated): a step is `id` or
/// `id@owner`; a skip is `id:reason`.
type Summary = Vec<(Vec<String>, Vec<String>, bool)>;

fn summarise(plan: &Plan) -> Summary {
    plan.waves
        .iter()
        .map(|wave| {
            let steps = wave
                .steps
                .iter()
                .map(|step| match step.scope {
                    StepScope::AsAsked => step.finder.to_string(),
                    StepScope::Owner => format!("{}@owner", step.finder),
                })
                .collect();
            let skipped = wave
                .skipped
                .iter()
                .map(|(finder, reason)| {
                    let reason = match reason {
                        SkipReason::Disabled => "disabled",
                        SkipReason::NoKey { .. } => "no_key",
                        SkipReason::NoInstance { .. } => "no_instance",
                        SkipReason::NoContact => "no_contact",
                        SkipReason::CircuitOpen { .. } => "circuit_open",
                        SkipReason::QuotaSpent { .. } => "quota_spent",
                        SkipReason::NotApplicable => "not_applicable",
                        SkipReason::OutOfBudget => "out_of_budget",
                    };
                    format!("{finder}:{reason}")
                })
                .collect();
            (steps, skipped, wave.reformulated)
        })
        .collect()
}

fn w(steps: &[&str], skipped: &[&str], reformulated: bool) -> (Vec<String>, Vec<String>, bool) {
    (
        steps.iter().map(|s| s.to_string()).collect(),
        skipped.iter().map(|s| s.to_string()).collect(),
        reformulated,
    )
}

const OPT_IN_SKIPS: [&str; 4] = [
    "brave:no_key",
    "tavily:no_key",
    "exa:no_key",
    "searxng:no_instance",
];

struct Setup {
    settings: SourcingSettingsDto,
    keys: BTreeSet<KeyService>,
    refused: BTreeMap<FinderId, SkipReason>,
}

impl Setup {
    fn fresh() -> Self {
        Self {
            settings: SourcingSettingsDto::default(),
            keys: BTreeSet::new(),
            refused: BTreeMap::new(),
        }
    }

    fn plan(&self, need: &SourceNeed) -> Plan {
        let gate = FinderGate {
            settings: &self.settings,
            keys: &self.keys,
            refused: &self.refused,
        };
        plan(need, &registry(), &gate)
    }
}

#[test]
fn each_kind_gets_its_waves() {
    let crates = Identifier::PackageName {
        ecosystem: Ecosystem::Crates,
        name: "tokio".into(),
    };
    let mut web_reworded = need(K::Web);
    web_reworded.alternatives = vec!["other words".into()];
    web_reworded.also = vec![K::Encyclopedia];

    let cases: Vec<(&str, SourceNeed, bool, Summary)> = vec![
        (
            "web: keyless engines, then opt-in engines; no rewordings, no third wave",
            need(K::Web),
            false,
            vec![
                w(&["duckduckgo", "bing_html"], &[], false),
                w(&[], &OPT_IN_SKIPS, false),
            ],
        ),
        (
            "web with rewordings and Wikipedia asked for",
            web_reworded,
            false,
            vec![
                w(&["duckduckgo", "bing_html", "wikipedia"], &[], false),
                w(&[], &OPT_IN_SKIPS, false),
                w(&["duckduckgo", "bing_html"], &[], true),
            ],
        ),
        (
            "official docs with a subject: owner first, then the open web, then opt-in",
            with_subject(need(K::OfficialDocs), ".NET", Vec::new()),
            true,
            vec![
                w(
                    &[
                        "site_discovery@owner",
                        "duckduckgo@owner",
                        "bing_html@owner",
                    ],
                    &[],
                    false,
                ),
                w(&["duckduckgo", "bing_html"], &[], true),
                w(&[], &OPT_IN_SKIPS, true),
            ],
        ),
        (
            "official docs with nothing to own it: routed like the web",
            need(K::OfficialDocs),
            false,
            vec![
                w(&["duckduckgo", "bing_html"], &[], false),
                w(&[], &OPT_IN_SKIPS, false),
            ],
        ),
        (
            "package with a known ecosystem: that registry exactly, plus GitHub",
            with_subject(need(K::Package), "tokio", vec![crates]),
            false,
            vec![
                w(&["github", "crates_io"], &[], false),
                w(&["site_discovery@owner"], &[], false),
                w(&["duckduckgo@owner", "bing_html@owner"], &[], false),
            ],
        ),
        (
            "package without an ecosystem: every registry",
            need(K::Package),
            false,
            vec![
                w(&["github", "crates_io", "npm"], &[], false),
                w(&["site_discovery@owner"], &[], false),
                w(&["duckduckgo@owner", "bing_html@owner"], &[], false),
            ],
        ),
        (
            "package named without its registry: its owner first, then every registry",
            with_subject(need(K::Package), "requests", Vec::new()),
            true,
            vec![
                w(&["github", "crates_io", "npm"], &[], false),
                w(&["site_discovery@owner"], &[], false),
                w(&["duckduckgo@owner", "bing_html@owner"], &[], false),
            ],
        ),
        (
            "paper: OpenAlex + Crossref, then S2 + arXiv, then Unpaywall and the engines",
            need(K::Paper),
            false,
            vec![
                w(&["openalex", "crossref"], &[], false),
                w(&["semantic_scholar", "arxiv"], &[], false),
                w(
                    &["duckduckgo", "bing_html"],
                    &["exa:no_key", "unpaywall:no_contact"],
                    false,
                ),
            ],
        ),
        (
            "standard: the bodies, then engines on the body, then engines",
            need(K::Standard),
            false,
            vec![
                w(&["rfc_index", "w3c"], &[], false),
                w(&["duckduckgo@owner", "bing_html@owner"], &[], false),
                w(&["duckduckgo", "bing_html"], &[], false),
            ],
        ),
        (
            "law: the publishers, then keyed govinfo, then engines",
            need(K::Law),
            false,
            vec![
                w(&["federal_register"], &[], false),
                w(&[], &["govinfo:no_key"], false),
                w(&["duckduckgo", "bing_html"], &[], false),
            ],
        ),
        (
            "community: Stack Exchange + HN, then Discourse, then engines",
            need(K::Community),
            false,
            vec![
                w(&["stack_exchange", "hn_algolia"], &[], false),
                w(&["discourse_json"], &[], false),
                w(&["duckduckgo", "bing_html"], &[], false),
            ],
        ),
        (
            "encyclopedia: Wikipedia, then Wikidata",
            need(K::Encyclopedia),
            false,
            vec![w(&["wikipedia"], &[], false), w(&["wikidata"], &[], false)],
        ),
        (
            "archive: Wayback, then Common Crawl",
            need(K::Archive),
            false,
            vec![
                w(&["wayback"], &[], false),
                w(&["common_crawl_index"], &[], false),
            ],
        ),
        (
            "leads: the page's links, then the academic finders",
            need(K::Leads),
            false,
            vec![
                w(&["page_links"], &[], false),
                w(&["openalex", "crossref"], &[], false),
            ],
        ),
    ];

    let setup = Setup::fresh();
    for (case, need, resolve_owner, expected) in cases {
        let plan = setup.plan(&need);
        assert_eq!(plan.resolve_owner, resolve_owner, "{case}");
        assert_eq!(summarise(&plan), expected, "{case}");
    }
}

#[test]
fn a_dated_read_routes_to_the_archive() {
    let mut dated = need(K::OfficialDocs);
    dated.freshness = Freshness::AsOf(NaiveDate::from_ymd_opt(2023, 5, 1).unwrap());
    assert_eq!(routed_kind(&dated), K::Archive);
    assert_eq!(
        summarise(&Setup::fresh().plan(&dated)),
        vec![
            w(&["wayback"], &[], false),
            w(&["common_crawl_index"], &[], false)
        ]
    );
}

#[test]
fn a_scope_without_a_subject_still_goes_owner_first() {
    let mut scoped = need(K::OfficialDocs);
    scoped.scope = Origin::parse("docs.python.org/3");
    let plan = Setup::fresh().plan(&scoped);
    assert!(
        !plan.resolve_owner,
        "nothing to resolve: the scope is the owner"
    );
    assert_eq!(
        summarise(&plan)[0],
        w(
            &[
                "site_discovery@owner",
                "duckduckgo@owner",
                "bing_html@owner"
            ],
            &[],
            false
        )
    );
}

#[test]
fn keys_contacts_instances_and_switches_gate_finders() {
    let mut setup = Setup::fresh();
    setup.keys.insert(KeyService::Brave);
    setup.keys.insert(KeyService::Exa);
    setup.settings.searxng_url = Some("https://searx.example.org".into());
    setup.settings.contact = Some("me@example.org".into());
    setup.settings.finders.insert(
        "tavily".into(),
        crate::application::contracts::settings::FinderSettingDto { enabled: false },
    );

    assert_eq!(
        summarise(&setup.plan(&need(K::Web))),
        vec![
            w(&["duckduckgo", "bing_html"], &[], false),
            w(&["brave", "exa", "searxng"], &["tavily:disabled"], false),
        ]
    );
    assert_eq!(
        summarise(&setup.plan(&need(K::Paper)))[2],
        w(&["exa", "unpaywall", "duckduckgo", "bing_html"], &[], false)
    );
}

#[test]
fn a_searxng_url_that_is_not_http_keeps_it_off() {
    let mut setup = Setup::fresh();
    setup.settings.searxng_url = Some("file:///etc/passwd".into());
    let plan = setup.plan(&need(K::Web));
    assert!(summarise(&plan)[1]
        .1
        .contains(&"searxng:no_instance".to_string()));
}

#[test]
fn turning_the_archive_fallback_off_turns_the_archive_finders_off() {
    let mut setup = Setup::fresh();
    setup.settings.archive_fallback = false;
    assert_eq!(
        summarise(&setup.plan(&need(K::Archive))),
        vec![
            w(&[], &["wayback:disabled"], false),
            w(&[], &["common_crawl_index:disabled"], false),
        ]
    );
}

#[test]
fn an_open_breaker_is_skipped_once_and_the_wave_still_runs() {
    let mut setup = Setup::fresh();
    let until = Utc
        .with_ymd_and_hms(2026, 10, 18, 14, 2, 0)
        .single()
        .unwrap();
    setup.refused.insert(
        FinderId::from_static("duckduckgo"),
        SkipReason::CircuitOpen {
            until,
            last: Box::new(FailureCause::Challenge),
        },
    );
    let mut reworded = need(K::Web);
    reworded.alternatives = vec!["other words".into()];
    assert_eq!(
        summarise(&setup.plan(&reworded)),
        vec![
            w(&["bing_html"], &["duckduckgo:circuit_open"], false),
            w(&[], &OPT_IN_SKIPS, false),
            w(&["bing_html"], &[], true),
        ]
    );
}

#[test]
fn owner_scoped_engine_calls_need_a_native_site_filter() {
    let mut registry = registry();
    for descriptor in &mut registry {
        if descriptor.id.as_str() == "bing_html" {
            descriptor.supports_scope = false;
        }
    }
    let setup = Setup::fresh();
    let gate = FinderGate {
        settings: &setup.settings,
        keys: &setup.keys,
        refused: &setup.refused,
    };
    let docs = with_subject(need(K::OfficialDocs), ".NET", Vec::new());
    assert_eq!(
        summarise(&plan(&docs, &registry, &gate))[0],
        w(&["site_discovery@owner", "duckduckgo@owner"], &[], false)
    );
}
