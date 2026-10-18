//! The eval set in `evals/provenance/` as tests: every labelled source's
//! origin and level, the copy thresholds pinned to the labelled pairs, every
//! sufficiency, contradiction, temporal and calibration case, citation-chain
//! ranking and injection detection. The README there describes the files.

#![cfg(test)]

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::Value;

use super::*;

fn eval_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../evals/provenance")
}

fn lines(name: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(eval_dir().join(name)).unwrap();
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn date(text: &str) -> i64 {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

/// When every fixture was read: the eval set's acquisition date.
fn acquired() -> i64 {
    date("2026-10-18")
}

fn enum_of<T: for<'de> Deserialize<'de>>(value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap()
}

// Sources ----------------------------------------------------------------------------------

#[derive(Deserialize)]
struct SourceRow {
    id: String,
    language: String,
    subject: SubjectRow,
    added_by: AddedBy,
    signals: ProvenanceSignals,
    expected: Expected,
    #[serde(default)]
    mirror_of: Option<MirrorRow>,
    rationale: String,
}

#[derive(Deserialize)]
struct SubjectRow {
    name: String,
    kind: SubjectKind,
    aliases: Vec<String>,
    classes: Vec<String>,
}

#[derive(Deserialize)]
struct Expected {
    standing: Standing,
    publisher_kind: PublisherKind,
    editorial: EditorialProcess,
    nature: ContentNature,
    flags: Vec<RiskFlag>,
    relation: SubjectRelation,
    level: TrustLevel,
}

#[derive(Deserialize)]
struct MirrorRow {
    id: String,
    relation: RelationKind,
    containment: f64,
}

#[test]
fn every_labelled_source_has_the_origin_the_suffix_list_gives() {
    let mut failures = Vec::new();
    for row in lines("sources-v1.jsonl") {
        let row: SourceRow = serde_json::from_value(row).unwrap();
        if let ProvenanceSignals::Web(page) = &row.signals {
            let computed = Origin::of_url(&page.resolved_url).unwrap();
            if computed != page.origin {
                failures.push(format!("{}: {computed:?} != {:?}", row.id, page.origin));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_labelled_source_gets_its_labelled_level_from_its_labelled_judgements() {
    let rows: Vec<SourceRow> = lines("sources-v1.jsonl")
        .into_iter()
        .map(|row| serde_json::from_value(row).unwrap())
        .collect();
    assert!(rows.len() >= 100, "about 120 sources");
    let languages: std::collections::BTreeSet<&str> =
        rows.iter().map(|r| r.language.as_str()).collect();
    assert!(languages.len() >= 6, "{languages:?}");
    let mut levels: HashMap<String, TrustLevel> = HashMap::new();
    let mut failures = Vec::new();
    for row in &rows {
        assert!(!row.rationale.trim().is_empty(), "{}", row.id);
        let origin = match &row.signals {
            ProvenanceSignals::Web(page) => page.origin.clone(),
            ProvenanceSignals::Document(document) => Origin::of_file(&document.file_name),
            ProvenanceSignals::Pasted(_) | ProvenanceSignals::NotExtracted { .. } => {
                Origin::pasted(&row.id)
            }
        };
        let subject = ClaimSubject::new(
            row.subject.name.clone(),
            row.subject.kind,
            row.subject.aliases.clone(),
            row.subject.classes.clone(),
        );
        let standing = StandingAssessment {
            standing: row.expected.standing,
            publisher: None,
            publisher_kind: row.expected.publisher_kind,
            editorial: row.expected.editorial,
            nature: row.expected.nature,
            flags: row.expected.flags.clone(),
            reasons: vec![Reason::new(ReasonCode::PublisherMetadata, "labelled")],
            policy: "label".into(),
            model: "label".into(),
            signals_sha256: row.signals.signals_sha256(),
            track_record: TrackRecord::default(),
        };
        let relation = AuthorityAssessment {
            origin_key: origin.key.clone(),
            subject_key: subject.key.clone(),
            relation: row.expected.relation,
            basis: vec![Reason::new(ReasonCode::PublisherIsOwner, "labelled")],
            owner_origin_hints: vec![],
            policy: "label".into(),
            model: "label".into(),
        };
        let inherited: Vec<InheritedTrust> = row
            .mirror_of
            .iter()
            .map(|mirror| InheritedTrust {
                of: mirror.id.clone(),
                of_origin: OriginKey(mirror.id.clone()),
                relation: mirror.relation,
                level: levels[&mirror.id],
                containment: mirror.containment,
            })
            .collect();
        let trust = trust_level(&LevelInputs {
            program_id: "eval",
            source_id: &row.id,
            origin: &origin,
            signals: &row.signals,
            standing: &standing,
            subject: Some(&subject),
            relation: Some(&relation),
            inherited: &inherited,
            overrides: &Overrides::default(),
            added_by: row.added_by,
            policy: &POLICY,
        });
        if trust.level != row.expected.level {
            failures.push(format!(
                "{}: {} != labelled {} ({:?})",
                row.id,
                trust.level.as_str(),
                row.expected.level.as_str(),
                trust.reasons
            ));
        }
        levels.insert(row.id.clone(), trust.level);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// Independence ----------------------------------------------------------------------------

/// The best threshold on a 0.01 grid: the centre of the widest run of grid
/// points with the highest F1 (predicting a positive when score ≥ t).
/// Returns (threshold in hundredths, F1, run start, run end).
fn sweep(scores: &[(f64, bool)]) -> (u32, f64, u32, u32) {
    let f1_at = |t: u32| -> (u64, u64) {
        let threshold = f64::from(t) / 100.0;
        let (mut tp, mut fp, mut fn_) = (0u64, 0u64, 0u64);
        for (score, positive) in scores {
            match (*score >= threshold, *positive) {
                (true, true) => tp += 1,
                (true, false) => fp += 1,
                (false, true) => fn_ += 1,
                (false, false) => {}
            }
        }
        (2 * tp, 2 * tp + fp + fn_)
    };
    let grid: Vec<(u32, (u64, u64))> = (1..=100).map(|t| (t, f1_at(t))).collect();
    let better = |a: (u64, u64), b: (u64, u64)| a.0 * b.1 > b.0 * a.1;
    let best = grid
        .iter()
        .map(|(_, f)| *f)
        .fold((0, 1), |best, f| if better(f, best) { f } else { best });
    let equal = |f: (u64, u64)| f.0 * best.1 == best.0 * f.1;
    let (mut run, mut widest) = (None::<(u32, u32)>, (0u32, 0u32));
    for (t, f) in &grid {
        run = if equal(*f) {
            Some(run.map_or((*t, *t), |(start, _)| (start, *t)))
        } else {
            None
        };
        if let Some((start, end)) = run {
            if end - start > widest.1 - widest.0 || widest == (0, 0) {
                widest = (start, end);
            }
        }
    }
    let centre = (widest.0 + widest.1) / 2;
    (centre, best.0 as f64 / best.1 as f64, widest.0, widest.1)
}

/// Each pair's score with whether it is a positive, for one threshold.
type Scores = Vec<(f64, bool)>;

fn pair_scores() -> (Scores, Scores, usize) {
    let pairs = lines("independence-pairs-v1.jsonl");
    let mut resemblances = Vec::new();
    let mut containments = Vec::new();
    for pair in &pairs {
        let a = shingles(pair["a"].as_str().unwrap(), &POLICY);
        let b = shingles(pair["b"].as_str().unwrap(), &POLICY);
        let label = pair["label"].as_str().unwrap();
        resemblances.push((resemblance(&a, &b), label == "copy"));
        containments.push((
            containment(&a, &b).max(containment(&b, &a)),
            label != "independent",
        ));
    }
    (resemblances, containments, pairs.len())
}

#[test]
fn the_copy_thresholds_are_the_sweeps_f1_optimum() {
    let (resemblances, containments, pairs) = pair_scores();
    assert!(pairs >= 200, "about 200 labelled pairs");
    let (r, r_f1, r_low, r_high) = sweep(&resemblances);
    let (c, c_f1, c_low, c_high) = sweep(&containments);
    let hundredths = |value: f64| (value * 100.0).round() as u32;
    assert_eq!(
        (hundredths(POLICY.resemblance_copy), hundredths(POLICY.containment_copy)),
        (r, c),
        "resemblance F1 {r_f1:.3} on [{r_low}, {r_high}]; containment F1 {c_f1:.3} on [{c_low}, {c_high}]"
    );
}

/// Writes `results/independence-thresholds.json`: run it after changing the
/// pairs or the shingling, then commit the file (README, "How to run").
#[test]
#[ignore = "writes evals/provenance/results/independence-thresholds.json"]
fn record_independence_thresholds() {
    let (resemblances, containments, pairs) = pair_scores();
    let describe = |scores: &[(f64, bool)]| {
        let (t, f1, low, high) = sweep(scores);
        serde_json::json!({
            "threshold": f64::from(t) / 100.0,
            "f1": (f1 * 1000.0).round() / 1000.0,
            "optimal_from": f64::from(low) / 100.0,
            "optimal_to": f64::from(high) / 100.0,
            "positives": scores.iter().filter(|(_, p)| *p).count(),
            "negatives": scores.iter().filter(|(_, p)| !*p).count(),
        })
    };
    let report = serde_json::json!({
        "policy": POLICY.version,
        "pairs_file": "independence-pairs-v1.jsonl",
        "pairs": pairs,
        "shingling": {
            "words": POLICY.shingle_words,
            "chars": POLICY.shingle_chars,
            "word_break_ratio": POLICY.word_break_ratio,
        },
        "rule": "centre of the widest run of 0.01-grid thresholds with the highest F1; score >= threshold predicts positive",
        "resemblance_copy": describe(&resemblances),
        "containment_copy": describe(&containments),
    });
    std::fs::write(
        eval_dir().join("results/independence-thresholds.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}

// Sufficiency, temporal validity, contradiction, overturn, calibration --------------------

fn level_of(value: &Value) -> TrustLevel {
    enum_of(value)
}

fn dates_of(value: &Value) -> SourceDates {
    SourceDates {
        published: value.get("published").and_then(Value::as_str).map(date),
        modified: value.get("modified").and_then(Value::as_str).map(date),
        acquired_at: acquired(),
        ..SourceDates::default()
    }
}

fn profile_of(case: &Value) -> ClaimProfile {
    let profile = &case["profile"];
    let temporal = &profile["temporal"];
    let scope = match temporal["scope"].as_str().unwrap() {
        "timeless" => TemporalScope::Timeless,
        "current" => TemporalScope::Current,
        "as_of" => TemporalScope::AsOf {
            date: date(temporal["date"].as_str().unwrap()),
        },
        "since" => TemporalScope::Since {
            version: temporal
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
            date: temporal.get("date").and_then(Value::as_str).map(date),
        },
        other => panic!("scope {other}"),
    };
    ClaimProfile {
        kind: enum_of(&profile["kind"]),
        subject: profile
            .get("subject")
            .and_then(Value::as_str)
            .map(|name| ClaimSubject::new(name, SubjectKind::Other, vec![], vec![])),
        temporal: scope,
        measurement_origin: profile
            .get("measurement_origin")
            .and_then(Value::as_str)
            .map(str::to_owned),
        attributed_to: None,
        reasons: vec![],
        policy: "fixture".into(),
        model: "fixture".into(),
    }
}

fn trust_of(value: &Value) -> Trust {
    let mirror = value
        .get("mirror")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Trust {
        level: level_of(&value["level"]),
        reasons: if mirror {
            vec![Reason::new(
                ReasonCode::Mirror,
                "Mirror of the owner's text",
            )]
        } else {
            vec![]
        },
        override_: None,
    }
}

fn temporal_of(
    profile: &ClaimProfile,
    value: &Value,
    newest: Option<(&str, i64)>,
) -> TemporalValidity {
    let mentions = value
        .get("mentions_version")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    validity(&profile.temporal, &dates_of(value), mentions, newest)
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn validity_name(validity: &TemporalValidity) -> &'static str {
    match validity {
        TemporalValidity::Valid => "valid",
        TemporalValidity::Stale { .. } => "stale",
        TemporalValidity::CannotPlace { .. } => "cannot_place",
        TemporalValidity::Undated { .. } => "undated",
        TemporalValidity::Invalid { .. } => "invalid",
    }
}

fn bars_of(case: &Value) -> EffectiveBars {
    let mut outcomes = Vec::new();
    for verdict in case
        .get("verdicts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let level = level_of(&verdict["level"]);
        let kind: ClaimKind = enum_of(&verdict["kind"]);
        let held = verdict["held"].as_bool().unwrap();
        let sufficient = StaticBars::table()
            .lowest(kind)
            .is_some_and(|lowest| level >= lowest);
        for at in 0..verdict["count"].as_i64().unwrap() {
            outcomes.push(Outcome {
                level,
                kind,
                decision_was_sufficient: sufficient,
                right: held == sufficient,
                at,
            });
        }
    }
    effective_bars(
        &StaticBars::table(),
        &Calibration::from_outcomes(&outcomes),
        &POLICY,
    )
}

fn json_name<T: serde::Serialize>(value: &T, tag: &str) -> Value {
    let json = serde_json::to_value(value).unwrap();
    match json.get(tag) {
        Some(name) => name.clone(),
        None => json,
    }
}

#[test]
fn every_sufficiency_case_decides_as_labelled() {
    let cases = lines("sufficiency-cases-v1.jsonl");
    assert!(cases.len() >= 40);
    let mut failures = Vec::new();
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let profile = profile_of(case);
        let newest = case.get("newest_primary").map(|n| {
            (
                n["id"].as_str().unwrap(),
                date(n["published"].as_str().unwrap()),
            )
        });
        let supports: Vec<Support> = case["supports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let sid = s["id"].as_str().unwrap();
                Support {
                    source_version_id: sid.into(),
                    origin_cluster: ClusterId(
                        s.get("cluster")
                            .and_then(Value::as_str)
                            .unwrap_or(sid)
                            .into(),
                    ),
                    trust: trust_of(s),
                    dates: dates_of(s),
                    passage: match s.get("passage") {
                        Some(p) if p.get("testimonial").is_some() => PassageKind::Testimonial {
                            attributed_to: p["testimonial"].as_str().unwrap().into(),
                            borne_out: None,
                        },
                        Some(p) if p.get("quotation").is_some() => PassageKind::Quotation {
                            of: p["quotation"].as_str().map(str::to_owned),
                        },
                        _ => PassageKind::Direct,
                    },
                    temporal: temporal_of(&profile, s, newest),
                    added_by: s.get("added_by").map_or(AddedBy::Research, enum_of),
                    cited_urls: strings(s.get("cited_urls")),
                    owner_hints: strings(s.get("owner_hints")),
                }
            })
            .collect();
        let contrary: Vec<Contrary> = case
            .get("contrary")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|c| {
                let cid = c["id"].as_str().unwrap();
                Contrary {
                    source_version_id: cid.into(),
                    origin_cluster: ClusterId(
                        c.get("cluster")
                            .and_then(Value::as_str)
                            .unwrap_or(cid)
                            .into(),
                    ),
                    trust: trust_of(c),
                    dates: dates_of(c),
                    temporal: temporal_of(&profile, c, newest),
                    quote: "contrary".into(),
                }
            })
            .collect();
        let decision = decide(&profile, &supports, &contrary, &bars_of(case));
        let resolutions = resolve_all(&profile, &supports, &contrary);
        let overturned = overturn(&decision, &resolutions);
        let expected = &case["expected"];
        let mut check = |what: &str, got: Value, want: &Value| {
            if got != *want {
                failures.push(format!("{id} {what}: got {got}, want {want}"));
            }
        };
        let decision_json = serde_json::to_value(&decision).unwrap();
        check(
            "decision",
            decision_json["decision"].clone(),
            &expected["decision"],
        );
        check(
            "overturn",
            json_name(&overturned, "overturn"),
            &expected["overturn"],
        );
        if let Some(want) = expected.get("label") {
            let label = match &decision {
                Decision::Sufficient {
                    label: Some(Label::ReportedBy { attributing_to }),
                    ..
                } => {
                    serde_json::json!({ "reported_by": attributing_to })
                }
                Decision::Sufficient {
                    label: Some(label), ..
                } => json_name(label, "label"),
                _ => Value::Null,
            };
            check("label", label, want);
        }
        if let Some(want) = expected.get("met_by") {
            check(
                "met_by",
                decision_json.get("met_by").cloned().unwrap_or(Value::Null),
                want,
            );
        }
        if let Decision::Insufficient {
            bar,
            missing,
            next,
            have,
            ..
        } = &decision
        {
            if let Some(want) = expected.get("bar") {
                check("bar", serde_json::to_value(bar).unwrap(), want);
            }
            if let Some(want) = expected.get("missing") {
                let got = match missing {
                    Missing::NoIndependentCorroboration { have } => {
                        serde_json::json!({ "no_independent_corroboration": have })
                    }
                    Missing::OnlyStale { newer_primary } => {
                        serde_json::json!({ "only_stale": newer_primary })
                    }
                    other => json_name(other, "missing"),
                };
                check("missing", got, want);
            }
            if let Some(want) = expected.get("next") {
                let got: Vec<Value> = next.iter().map(|step| json_name(step, "step")).collect();
                check("next", Value::Array(got), want);
            }
            if let Some(want) = expected.get("counted").and_then(Value::as_object) {
                for (sid, level) in want {
                    let got = have
                        .iter()
                        .find(|h| &h.source_version_id == sid)
                        .map(|h| serde_json::to_value(h.counted).unwrap());
                    check(&format!("counted {sid}"), got.unwrap_or(Value::Null), level);
                }
            }
        } else if let Some(want) = expected.get("counted").and_then(Value::as_object) {
            let _ = want;
        }
        if let Some(want) = expected.get("temporal").and_then(Value::as_object) {
            for (sid, name) in want {
                let got = supports
                    .iter()
                    .find(|s| &s.source_version_id == sid)
                    .map(|s| validity_name(&s.temporal));
                check(
                    &format!("temporal {sid}"),
                    Value::from(got.unwrap_or("missing")),
                    name,
                );
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// Citation chains and injection -----------------------------------------------------------

#[test]
fn every_chain_case_ranks_its_candidates_as_labelled() {
    let mut failures = Vec::new();
    for case in lines("chain-cases-v1.jsonl") {
        let text = case["passage_markdown"].as_str().unwrap();
        let span_text = case["span_text"].as_str().unwrap();
        let start = text.find(span_text).unwrap();
        let owners = strings(case.get("owner_domains"));
        let authorities = strings(case.get("authoritative_domains"));
        let links = markdown_links(text);
        let candidates =
            citation_candidates(text, (start, start + span_text.len()), &links, |link| {
                let site = Origin::of_url(&link.url)
                    .ok()
                    .and_then(|o| o.site().map(str::to_owned))
                    .unwrap_or_default();
                if owners.contains(&site) {
                    LinkStanding::Owner
                } else if authorities.contains(&site) {
                    LinkStanding::Authoritative
                } else {
                    LinkStanding::Unknown
                }
            });
        let got: Vec<String> = candidates.into_iter().map(|c| c.url).collect();
        let want = strings(case.get("expected_candidates_in_order"));
        if got != want {
            failures.push(format!("{}: {got:?} != {want:?}", case["id"]));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_injection_and_self_description_case_is_detected_as_labelled() {
    let mut failures = Vec::new();
    for case in lines("injection-cases-v1.jsonl") {
        let id = case["id"].as_str().unwrap();
        if let Some(text) = case.get("text").and_then(Value::as_str) {
            let want = case["expected_injection"].as_bool().unwrap();
            if addresses_reader(text) != want {
                failures.push(format!("{id}: {text:?} should be {want}"));
            }
        } else {
            let fields = strings(case.get("self_description_fields"));
            let got = self_description(fields.iter().map(String::as_str));
            let want = strings(case.get("expected_tokens"));
            if got != want {
                failures.push(format!("{id}: {got:?} != {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
