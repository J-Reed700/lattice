#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! The provenance gate over fake judgements: levels are given, so these
//! tests pin what the gate builds from a finding (supports, clusters,
//! passage kinds, citations), how it maps the decision onto the claim
//! ledger, and that verification makes no model call beyond the check.
use super::*;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::domain::provenance::{
    ClaimKind, ClaimSubject, NextStep, StaticBars, SubjectKey, SubjectKind, TemporalValidity,
    POLICY,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn bars() -> EffectiveBars {
    EffectiveBars::unmoved(&StaticBars::table(), &POLICY)
}

fn kind(name: &str) -> ClaimKind {
    ClaimKind::from_column(name).expect("a claim kind")
}

fn level(name: &str) -> TrustLevel {
    TrustLevel::from_column(name).expect("a trust level")
}

fn profile(kind: ClaimKind, subject: Option<&str>) -> ClaimProfile {
    ClaimProfile {
        kind,
        subject: subject.map(|name| ClaimSubject {
            name: name.into(),
            kind: SubjectKind::Product,
            aliases: Vec::new(),
            classes: Vec::new(),
            key: SubjectKey::of(name),
        }),
        temporal: TemporalScope::Timeless,
        measurement_origin: None,
        attributed_to: None,
        reasons: Vec::new(),
        policy: "test".into(),
        model: "test".into(),
    }
}

fn facts(level: TrustLevel, origin: &str, text: &str, links: Vec<OutboundLink>) -> VersionFacts {
    VersionFacts {
        origin: OriginKey(origin.into()),
        trust: Trust {
            level,
            reasons: Vec::new(),
            override_: None,
        },
        dates: SourceDates::default(),
        added_by: AddedBy::Research,
        saved: Arc::new(SavedText {
            text: text.into(),
            links,
        }),
        owner_hints: Vec::new(),
    }
}

fn passage(id: &str, text: &str) -> EvidencePassage {
    EvidencePassage {
        source_id: id.into(),
        text: text.into(),
        start_byte: 0,
        end_byte: text.len(),
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
    }
}

fn finding(statement: &str, quote: &str, evidence: Vec<EvidencePassage>) -> Finding {
    Finding {
        unit: 0,
        quote: statement.into(),
        statement: statement.into(),
        verdict: ClaimVerdict::Supported,
        reason: "The passage states it.".into(),
        evidence,
        supporting_quote: Some(quote.into()),
        resolution: None,
        two_pass: Default::default(),
        trust: None,
    }
}

fn weighed(
    profile: &ClaimProfile,
    finding: &Finding,
    facts: &HashMap<String, VersionFacts>,
) -> FindingTrust {
    assess(
        profile,
        finding,
        &Evidence {
            facts,
            relations: &[],
            bars: &bars(),
            policy: &POLICY,
        },
    )
}

/// The ledger's gate on a weighed finding.
fn judge(finding: &Finding) -> Sufficiency {
    let quote = finding.supporting_quote.clone().unwrap_or_default();
    let source = supporting_passages(finding)
        .first()
        .map(|passage| passage.source_id.clone())
        .unwrap_or_default();
    ProvenanceGate.judge(&Supported {
        claim: &finding.statement,
        source_version_id: &source,
        supporting_quote: &quote,
        passages: &finding.evidence,
        trust: finding.trust.as_deref(),
    })
}

const STRUCTURED: &str =
    "Structured Outputs ensures the model always follows the supplied JSON Schema.";

#[test]
fn a_primary_source_meets_a_specification_bar() {
    let text = "Structured Outputs ensures the model always follows the supplied JSON Schema.";
    let mut claim = finding(STRUCTURED, text, vec![passage("docs", text)]);
    let facts = HashMap::from([(
        "docs".to_owned(),
        facts(level("primary"), "web:openai.com", text, Vec::new()),
    )]);
    let trust = weighed(
        &profile(ClaimKind::Specification, Some("OpenAI API")),
        &claim,
        &facts,
    );
    assert!(matches!(trust.decision, Decision::Sufficient { .. }));
    assert!(!trust.needs_scrutiny());
    assert_eq!(trust.supports.len(), 1);
    assert_eq!(trust.supports[0].passage, PassageKind::Direct);
    claim.trust = Some(Box::new(trust));
    assert!(matches!(judge(&claim), Sufficiency::Sufficient));
}

#[test]
fn an_established_source_alone_fails_a_specification_bar_and_names_its_next_steps() {
    let link = "https://platform.openai.com/docs/guides/structured-outputs";
    let text = format!("Structured Outputs ensures the model always follows the supplied JSON Schema (see [the guide]({link})).");
    let links = lattice_core::domain::provenance::markdown_links(&text);
    assert_eq!(links.len(), 1, "the fixture text carries one link");
    let mut blog = facts(level("established"), "web:blog.example", &text, links);
    blog.owner_hints = vec!["platform.openai.com".into()];
    let mut claim = finding(
        STRUCTURED,
        "Structured Outputs ensures the model always follows the supplied JSON Schema",
        vec![passage("blog", &text)],
    );
    let facts = HashMap::from([("blog".to_owned(), blog)]);
    let trust = weighed(
        &profile(ClaimKind::Specification, Some("OpenAI API")),
        &claim,
        &facts,
    );
    let Decision::Insufficient { bar, next, .. } = &trust.decision else {
        panic!("an established blog alone is not enough for a specification");
    };
    assert_eq!(*bar, lattice_core::domain::provenance::Bar::Primary);
    assert_eq!(trust.supports[0].cited_urls, vec![link.to_owned()]);
    assert!(next.contains(&NextStep::FollowCitation {
        url: link.into(),
        from: "blog".into()
    }));
    assert!(next.iter().any(|step| matches!(
        step,
        NextStep::SearchOwner { origin_hints } if origin_hints.contains(&"platform.openai.com".to_owned())
    )));
    // The blog relays the owner's documentation: a report until followed.
    assert!(matches!(
        &trust.supports[0].passage,
        PassageKind::Testimonial { attributed_to, borne_out: None } if attributed_to == "platform.openai.com"
    ));
    assert!(trust.needs_scrutiny());
    claim.trust = Some(Box::new(trust));
    let Sufficiency::Insufficient { reason, .. } = judge(&claim) else {
        panic!("the ledger records an insufficient support as unsupported");
    };
    assert!(reason.starts_with("Its support "), "{reason}");
    assert!(reason.contains("OpenAI API"), "{reason}");
}

#[test]
fn a_passage_linking_to_the_subject_owner_reports_the_owner() {
    let text = "The USDA says poultry is safe at [74 °C](https://www.fsis.usda.gov/food-safety).";
    let links = lattice_core::domain::provenance::markdown_links(text);
    let claim = finding(
        "Poultry is safe at an internal temperature of 74 °C.",
        text,
        vec![passage("cooking", text)],
    );
    let mut cooking = facts(level("authoritative"), "web:seriouseats.com", text, links);
    cooking.owner_hints = vec!["fsis.usda.gov".into()];
    let facts = HashMap::from([("cooking".to_owned(), cooking)]);
    let trust = weighed(
        &profile(ClaimKind::Measurement, Some("USDA FSIS")),
        &claim,
        &facts,
    );
    assert!(matches!(
        &trust.supports[0].passage,
        PassageKind::Testimonial { attributed_to, borne_out: None } if attributed_to == "www.fsis.usda.gov"
    ));
    assert!(matches!(trust.decision, Decision::Insufficient { .. }));
}

#[test]
fn links_to_its_own_site_or_to_a_non_owner_leave_a_passage_direct() {
    let text = "Use [strict mode](https://openai.com/docs/strict) with a [JSON Schema](https://json-schema.org/) to follow the schema.";
    let links = lattice_core::domain::provenance::markdown_links(text);
    let claim = finding(
        "Strict mode follows the schema.",
        text,
        vec![passage("docs", text)],
    );
    let mut docs = facts(level("primary"), "web:openai.com", text, links);
    docs.owner_hints = vec!["openai.com".into()];
    let facts = HashMap::from([("docs".to_owned(), docs)]);
    let trust = weighed(
        &profile(ClaimKind::Specification, Some("OpenAI API")),
        &claim,
        &facts,
    );
    assert_eq!(trust.supports[0].passage, PassageKind::Direct);
    assert!(matches!(trust.decision, Decision::Sufficient { .. }));
}

#[test]
fn a_supports_own_pages_and_its_publishers_are_never_citations_to_follow() {
    let guide = "https://platform.openai.com/docs/guides/structured-outputs";
    let text = format!("Structured Outputs ensures the model always follows the supplied JSON Schema ([below](https://blog.example.org/structured#strict), [related post](https://www.example.org/rlhf)), as [the guide]({guide}) says.");
    let links = lattice_core::domain::provenance::markdown_links(&text);
    let blog = facts(level("established"), "web:example.org", &text, links);
    let claim = finding(
        STRUCTURED,
        "Structured Outputs ensures the model always follows the supplied JSON Schema",
        vec![passage("blog", &text)],
    );
    let facts = HashMap::from([("blog".to_owned(), blog)]);
    let trust = weighed(
        &profile(ClaimKind::Specification, Some("OpenAI API")),
        &claim,
        &facts,
    );
    assert_eq!(trust.supports[0].cited_urls, vec![guide.to_owned()]);
}

#[test]
fn copies_from_one_origin_are_one_support_and_independent_origins_corroborate() {
    let text = "The Peace of Westphalia was signed in 1648.";
    let historical = profile(ClaimKind::Historical, Some("Peace of Westphalia"));
    let claim = finding(text, text, vec![passage("a", text), passage("b", text)]);
    let same = HashMap::from([
        (
            "a".to_owned(),
            facts(
                level("established"),
                "web:history.example",
                text,
                Vec::new(),
            ),
        ),
        (
            "b".to_owned(),
            facts(
                level("established"),
                "web:history.example",
                text,
                Vec::new(),
            ),
        ),
    ]);
    let trust = weighed(&historical, &claim, &same);
    assert_eq!(
        trust.supports[0].origin_cluster,
        trust.supports[1].origin_cluster
    );
    assert!(matches!(trust.decision, Decision::Insufficient { .. }));

    let other = "In 1648 the Peace of Westphalia was signed, ending the Thirty Years' War.";
    let claim = finding(
        text,
        "Peace of Westphalia was signed",
        vec![passage("a", text), passage("c", other)],
    );
    let independent = HashMap::from([
        (
            "a".to_owned(),
            facts(
                level("established"),
                "web:history.example",
                text,
                Vec::new(),
            ),
        ),
        (
            "c".to_owned(),
            facts(
                level("established"),
                "web:another.example",
                other,
                Vec::new(),
            ),
        ),
    ]);
    let trust = weighed(&historical, &claim, &independent);
    assert_ne!(
        trust.supports[0].origin_cluster,
        trust.supports[1].origin_cluster
    );
    assert!(matches!(trust.decision, Decision::Sufficient { .. }));
}

#[test]
fn only_passages_holding_the_quotation_support_and_each_version_once() {
    let text = "Monotremes lay eggs.";
    let claim = finding(
        text,
        text,
        vec![
            passage("a", text),
            passage("a", "Monotremes lay eggs. Platypuses are monotremes."),
            passage("b", "Most mammals give birth to live young."),
            EvidencePassage {
                retrieval_kind: EXECUTION.into(),
                ..passage("run", text)
            },
        ],
    );
    let ids: Vec<_> = supporting_passages(&claim)
        .into_iter()
        .map(|passage| passage.source_id.as_str())
        .collect();
    assert_eq!(ids, vec!["a"]);
}

#[test]
fn material_the_user_added_is_sufficient_and_says_so() {
    let text = "Our team's style guide requires tabs.";
    let mut pasted = facts(level("unknown"), "pasted:v1", text, Vec::new());
    pasted.added_by = AddedBy::User;
    let claim = finding(text, text, vec![passage("v1", text)]);
    let facts = HashMap::from([("v1".to_owned(), pasted)]);
    let trust = weighed(&profile(ClaimKind::Practice, None), &claim, &facts);
    let Decision::Sufficient { reasons, .. } = &trust.decision else {
        panic!("the user's own evidence is accepted");
    };
    assert!(reasons
        .iter()
        .any(|reason| reason.code == lattice_core::domain::provenance::ReasonCode::AddedByYou));
}

#[test]
fn a_quotation_outside_the_named_passage_is_never_sufficient() {
    let text = "Structured Outputs ensures the model always follows the supplied JSON Schema.";
    let mut claim = finding(
        STRUCTURED,
        "words the passage does not hold",
        vec![passage("docs", text)],
    );
    let facts = HashMap::from([(
        "docs".to_owned(),
        facts(level("primary"), "web:openai.com", text, Vec::new()),
    )]);
    claim.trust = Some(Box::new(weighed(
        &profile(ClaimKind::Specification, Some("OpenAI API")),
        &claim,
        &facts,
    )));
    let quote = claim.supporting_quote.clone().unwrap();
    let verdict = ProvenanceGate.judge(&Supported {
        claim: &claim.statement,
        source_version_id: "docs",
        supporting_quote: &quote,
        passages: &claim.evidence,
        trust: claim.trust.as_deref(),
    });
    assert!(matches!(verdict, Sufficiency::Insufficient { .. }));
}

#[test]
fn an_unweighed_finding_keeps_the_quotation_rule_alone() {
    let text = "Monotremes lay eggs.";
    let claim = finding(text, text, vec![passage("a", text)]);
    assert!(claim.trust.is_none());
    assert!(matches!(judge(&claim), Sufficiency::Sufficient));
}

#[test]
fn a_contrary_that_outranks_the_support_contradicts_the_claim() {
    let text = "The current stable Python is 3.13.";
    let mut current = profile(ClaimKind::VersionBound, Some("Python"));
    current.temporal = TemporalScope::Current;
    let mut claim = finding(text, text, vec![passage("blog", text)]);
    // Unix seconds: published mid-2025, saved and contradicted in 2026.
    let mut blog = facts(level("established"), "web:blog.example", text, Vec::new());
    blog.dates.published = Some(1_750_000_000);
    blog.dates.acquired_at = 1_760_000_000;
    let mut trust = weighed(
        &current,
        &claim,
        &HashMap::from([("blog".to_owned(), blog)]),
    );
    trust.contrary.push(Contrary {
        source_version_id: "python-org".into(),
        origin_cluster: ClusterId("python-org".into()),
        trust: Trust {
            level: TrustLevel::Primary,
            reasons: Vec::new(),
            override_: None,
        },
        dates: SourceDates {
            published: Some(1_790_000_000),
            acquired_at: 1_790_000_000,
            ..SourceDates::default()
        },
        temporal: TemporalValidity::Valid,
        quote: "The current stable release is Python 3.14.".into(),
    });
    assert!(trust.needs_scrutiny());
    assert!(matches!(
        trust.assessment("v1", "").overturn(),
        Overturn::Contradicted { .. }
    ));
    claim.trust = Some(Box::new(trust));
    assert!(matches!(judge(&claim), Sufficiency::Contradicted { .. }));
}

#[test]
fn a_contradiction_the_other_pass_found_is_weighed_as_a_contrary() {
    let text = "The current stable Python is 3.13.";
    let newer = "The current stable release is Python 3.14.";
    let mut current = profile(ClaimKind::VersionBound, Some("Python"));
    current.temporal = TemporalScope::Current;
    // Pass A supported the claim; pass B found a passage saying otherwise.
    let mut claim = finding(text, text, vec![passage("blog", text)]);
    claim.two_pass.contrary.push(ContraryPassage {
        passage: passage("python-org", newer),
        quote: newer.into(),
    });
    let mut blog = facts(level("established"), "web:blog.example", text, Vec::new());
    blog.dates.published = Some(1_750_000_000);
    blog.dates.acquired_at = 1_760_000_000;
    let mut org = facts(TrustLevel::Primary, "web:python.org", newer, Vec::new());
    org.dates.published = Some(1_790_000_000);
    org.dates.acquired_at = 1_790_000_000;
    let trust = weighed(
        &current,
        &claim,
        &HashMap::from([("blog".to_owned(), blog), ("python-org".to_owned(), org)]),
    );
    // The gate holds it beside the support, levelled for the claim's
    // subject, and the overturn rule weighs it.
    assert_eq!(trust.contrary.len(), 1);
    assert_eq!(trust.contrary[0].source_version_id, "python-org");
    assert_eq!(trust.contrary[0].trust.level, TrustLevel::Primary);
    assert_eq!(trust.contrary[0].quote, newer);
    assert!(trust.needs_scrutiny(), "research looks further");
    // A contrary version with no facts is not counted, as a support is not.
    let unweighed = weighed(
        &current,
        &claim,
        &HashMap::from([(
            "blog".to_owned(),
            facts(level("established"), "web:blog.example", text, Vec::new()),
        )]),
    );
    assert!(unweighed.contrary.is_empty());
}

#[test]
fn owner_hints_match_the_host_or_a_subdomain_only() {
    let hints = vec!["openai.com".to_owned()];
    assert!(owner_hinted("https://openai.com/docs", &hints));
    assert!(owner_hinted("https://platform.openai.com/docs", &hints));
    assert!(!owner_hinted(
        "https://openai.com.evil.example/docs",
        &hints
    ));
    assert!(!owner_hinted("https://notopenai.com/docs", &hints));
    assert!(!owner_hinted("not a url", &hints));
}

/// `sufficiency_cases.json` (the former evidence-challenge cases): the
/// checker supports each claim its passage states; whether that support is
/// enough is the gate's decision by the source's level and the claim's kind.
#[test]
fn sufficiency_cases_decide_by_level_and_kind() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("sufficiency_cases.json")).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let passages = case["passages"].as_array().unwrap();
        let supports = case["supports"].as_array().unwrap();
        let mut evidence = Vec::new();
        let mut known = HashMap::new();
        for (id, (text, support)) in passages.iter().zip(supports).enumerate() {
            let id = format!("v{id}");
            let text = text.as_str().unwrap();
            evidence.push(passage(&id, text));
            known.insert(
                id,
                facts(
                    level(support["level"].as_str().unwrap()),
                    support["origin"].as_str().unwrap(),
                    text,
                    Vec::new(),
                ),
            );
        }
        let claim = finding(
            case["claim"].as_str().unwrap(),
            case["quote"].as_str().unwrap(),
            evidence,
        );
        let trust = weighed(
            &profile(
                kind(case["kind"].as_str().unwrap()),
                Some(case["domain"].as_str().unwrap()),
            ),
            &claim,
            &known,
        );
        match (&trust.decision, case["decision"].as_str().unwrap()) {
            (Decision::Sufficient { .. }, "sufficient") => {}
            (Decision::Insufficient { bar, .. }, "insufficient") => assert_eq!(
                serde_json::to_value(bar).unwrap(),
                case["bar"],
                "case {index}"
            ),
            (decision, expected) => panic!("case {index}: expected {expected}, got {decision:?}"),
        }
    }
}

/// A checker that supports every claim by its first passage, counting its
/// calls.
struct CountingChecker {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl LLMPort for CountingChecker {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CompletionResponse {
            text: "Reason: The passage states it.\nSource passage: passage-0\nVerdict: supported"
                .into(),
            finish_reason: "stop".into(),
            ..Default::default()
        })
    }
    fn model_name(&self) -> &str {
        "counting-checker"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The function-calling lesson (`live_fixtures/function_calling_claims.json`):
/// each claim is judged with one model call and no challenge call, and its
/// support is then weighed by level and kind. The claims backed by the
/// owner's documentation, and the practices backed by an identified author,
/// pass; a specification only a third party states waits for the owner's.
#[tokio::test]
async fn the_function_calling_lesson_passes_its_primary_backed_claims_without_a_challenge_call(
) -> Result<()> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../live_fixtures/function_calling_claims.json"
    ))?;
    for case in fixture["claims"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let text = case["passage"].as_str().unwrap();
        let statement = case["claim"].as_str().unwrap();
        let checker = CountingChecker {
            calls: AtomicUsize::new(0),
        };
        let judged = judge_for_publication(&checker, statement, &[text.to_owned()], &[0])
            .await
            .judgment;
        assert_eq!(
            checker.calls.load(Ordering::SeqCst),
            1,
            "{id}: one check, no challenge"
        );
        let ClaimJudgment::Judged(outcome) = judged else {
            panic!("{id}: the checker answered");
        };
        assert_eq!(
            outcome.verdict,
            ClaimVerdict::Supported,
            "{id}: the checker's verdict stands"
        );
        let quote = outcome.quote.expect("the cited passage's text");

        let source = &fixture["sources"][case["source"].as_str().unwrap()];
        let mut version = facts(
            level(source["level"].as_str().unwrap()),
            source["origin"].as_str().unwrap(),
            text,
            Vec::new(),
        );
        version.owner_hints = case["owner_hints"]
            .as_array()
            .map(|hints| {
                hints
                    .iter()
                    .map(|hint| hint.as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut claim = finding(statement, &quote, vec![passage(id, text)]);
        let trust = weighed(
            &profile(
                kind(case["kind"].as_str().unwrap()),
                case["subject"].as_str(),
            ),
            &claim,
            &HashMap::from([(id.to_owned(), version)]),
        );
        claim.trust = Some(Box::new(trust));
        match case["decision"].as_str().unwrap() {
            "sufficient" => assert!(
                matches!(judge(&claim), Sufficiency::Sufficient),
                "{id}: {}",
                claim.trust.as_ref().unwrap().explain()
            ),
            _ => {
                let trust = claim.trust.as_ref().unwrap();
                assert!(trust.needs_scrutiny(), "{id}");
                assert!(
                    matches!(judge(&claim), Sufficiency::Insufficient { .. }),
                    "{id}"
                );
                let Decision::Insufficient { bar, next, .. } = &trust.decision else {
                    unreachable!()
                };
                assert_eq!(serde_json::to_value(bar)?, case["bar"], "{id}");
                assert!(
                    next.iter()
                        .any(|step| matches!(step, NextStep::SearchOwner { .. })),
                    "{id}"
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn outside_a_trust_scope_an_outline_assertion_is_not_weighed() -> Result<()> {
    let checker = CountingChecker {
        calls: AtomicUsize::new(0),
    };
    let page = vec![passage(
        "v1",
        "The API returns JSON when strict mode is on.",
    )];
    let weighed = weigh_claim(
        &checker,
        0,
        "The API returns JSON in strict mode.",
        &page,
        SupportedBy {
            source_version_id: "v1".into(),
            supporting_quote: "returns JSON".into(),
        },
    )
    .await?;
    assert!(weighed.is_none(), "no course to weigh it in");
    assert_eq!(checker.calls.load(Ordering::SeqCst), 0, "no model call");
    Ok(())
}
