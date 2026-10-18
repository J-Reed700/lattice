//! Descriptors express every decided provider: keyless finders on, the keyed
//! engines (Brave, Tavily, Exa) off until a key, SearXNG off until a URL,
//! polite pools off until a contact; disclosures render from the data.

#![cfg(test)]

use std::time::Duration;

use super::*;
use crate::domain::sourcing::need::Origin;

fn descriptor(name: &'static str, auth: Auth, privacy: Privacy) -> FinderDescriptor {
    FinderDescriptor {
        id: FinderId::from_static(name),
        name,
        finds: &[Role {
            kind: NeedKind::Web,
            wave: WaveSlot::Second,
        }],
        reaches: Reach::OpenWeb,
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

#[test]
fn activation_and_disclosure_follow_auth_and_privacy() {
    let searxng = Auth::UserInstance {
        setting: InstanceSetting::SearxngUrl,
    };
    let cases: Vec<(&str, Auth, Privacy, Activation, &str)> = vec![
        (
            "duckduckgo",
            Auth::None,
            Privacy::QueryLogged { by: "duckduckgo.com" },
            Activation::Always,
            "Your query is sent to duckduckgo.com, which may log it",
        ),
        (
            "brave",
            Auth::RequiredKey {
                service: KeyService::Brave,
                plan: KeyPlan::Credits,
            },
            Privacy::IdentifiedByKey {
                by: "api.search.brave.com",
            },
            Activation::WhenKeySet(KeyService::Brave),
            "Your query is sent to api.search.brave.com under your key",
        ),
        (
            "tavily",
            Auth::RequiredKey {
                service: KeyService::Tavily,
                plan: KeyPlan::Credits,
            },
            Privacy::IdentifiedByKey { by: "api.tavily.com" },
            Activation::WhenKeySet(KeyService::Tavily),
            "Your query is sent to api.tavily.com under your key",
        ),
        (
            "exa",
            Auth::RequiredKey {
                service: KeyService::Exa,
                plan: KeyPlan::Credits,
            },
            Privacy::IdentifiedByKey { by: "api.exa.ai" },
            Activation::WhenKeySet(KeyService::Exa),
            "Your query is sent to api.exa.ai under your key",
        ),
        (
            "searxng",
            searxng,
            Privacy::SentToUserInstance,
            Activation::WhenInstanceSet(InstanceSetting::SearxngUrl),
            "Your query is sent to the instance you set; its operator may log it",
        ),
        (
            "github",
            Auth::OptionalKey {
                service: KeyService::GitHub,
            },
            Privacy::QueryLogged { by: "api.github.com" },
            Activation::Always,
            "Your query is sent to api.github.com, which may log it; under your key when you add one",
        ),
        (
            "unpaywall",
            Auth::Contact,
            Privacy::IdentifiedByContact { by: "api.unpaywall.org" },
            Activation::WhenContactSet,
            "Your query and your contact are sent to api.unpaywall.org",
        ),
    ];
    for (name, auth, privacy, activation, disclosure) in cases {
        let finder = descriptor(name, auth, privacy);
        assert_eq!(finder.activation(), activation, "{name}");
        assert_eq!(finder.disclosure(), disclosure, "{name}");
    }
}

#[test]
fn keyring_ids_are_distinct_and_stable() {
    let ids: Vec<&str> = KeyService::ALL.iter().map(|s| s.keyring_id()).collect();
    assert_eq!(
        ids,
        vec![
            "brave",
            "tavily",
            "exa",
            "github",
            "semantic_scholar",
            "api_data_gov",
            "stack_exchange"
        ]
    );
}

#[test]
fn slot_for_reads_the_roles() {
    let finder = descriptor("exa", Auth::None, Privacy::AnonymousQuery);
    assert_eq!(finder.slot_for(NeedKind::Web), Some(WaveSlot::Second));
    assert_eq!(finder.slot_for(NeedKind::Paper), None);
}

#[test]
fn origins_cover_their_host_subdomains_and_path_prefix_only() {
    let url = |raw: &str| url::Url::parse(raw).unwrap();
    let dotnet = Origin::parse("https://GitHub.com/dotnet/").unwrap();
    assert_eq!(dotnet.to_string(), "github.com/dotnet");
    let cases = [
        ("the prefix itself", "https://github.com/dotnet", true),
        (
            "under the prefix",
            "https://github.com/dotnet/runtime",
            true,
        ),
        (
            "a sibling path",
            "https://github.com/dotnetfoundation/x",
            false,
        ),
        ("another host", "https://gitlab.com/dotnet/runtime", false),
    ];
    for (case, raw, expected) in cases {
        assert_eq!(dotnet.covers(&url(raw)), expected, "{case}");
    }

    let microsoft = Origin::parse("learn.microsoft.com").unwrap();
    assert!(microsoft.covers(&url("https://learn.microsoft.com/en-us/dotnet")));
    assert!(microsoft.covers(&url("https://eu.learn.microsoft.com/x")));
    assert!(
        !microsoft.covers(&url("https://learn-microsoft.com/x")),
        "a look-alike mirror"
    );
    assert!(!microsoft.covers(&url("https://notlearn.microsoft.com/x")));
    assert_eq!(Origin::parse(""), None);
    assert_eq!(Origin::parse("two words"), None);
}
