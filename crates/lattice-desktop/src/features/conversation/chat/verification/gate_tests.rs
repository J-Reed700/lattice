//! The judge gate against the labelled claim pairs in `evals/grounding/`.
//!
//! On the 2026-10-12 claim-checker bake-off, 23 of 101 pairs never reached
//! the judge, and 17 of those were false claims certified on word overlap
//! alone. Each test below takes the dataset's own claim and passage for one
//! way those 17 went wrong and checks that the claim now goes to the judge.
//! The last ones check that a copy of the passage still does not, unless the
//! sentence it copies says something the copy left out.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use super::lexical::{lexical_pass, needs_judge, LexicalClaim};
use super::test_support::source;

/// One labelled pair from `claim-pairs-v1.jsonl`.
pub(super) struct Pair {
    pub(super) claim: String,
    pub(super) passage: String,
    pub(super) label: String,
}

fn dataset() -> &'static HashMap<String, Pair> {
    static PAIRS: OnceLock<HashMap<String, Pair>> = OnceLock::new();
    PAIRS.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/grounding/claim-pairs-v1.jsonl");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let row: serde_json::Value = serde_json::from_str(line).expect("a JSON pair");
                let field = |name: &str| row[name].as_str().expect(name).to_string();
                (
                    field("id"),
                    Pair {
                        claim: field("claim"),
                        passage: field("passage"),
                        label: field("label"),
                    },
                )
            })
            .collect()
    })
}

pub(super) fn pair(id: &str) -> &'static Pair {
    dataset()
        .get(id)
        .unwrap_or_else(|| panic!("{id} is not in the dataset"))
}

/// The claim as the eval harness sends it: one answer paragraph with a `[1]`
/// marker before its closing stop, and the passage as source 1.
pub(super) fn cited_answer(claim: &str) -> String {
    let claim = claim.trim();
    for stop in ['.', '!'] {
        if let Some(body) = claim.strip_suffix(stop) {
            return format!("{body} [1]{stop}");
        }
    }
    format!("{claim} [1]")
}

fn assessed(claim: &str, passage: &str) -> Vec<LexicalClaim> {
    let claims = lexical_pass(&cited_answer(claim), &[source(passage)]);
    assert!(!claims.is_empty(), "no claim sentence in {claim:?}");
    claims
}

fn reaches_judge(id: &str) -> bool {
    let pair = pair(id);
    assessed(&pair.claim, &pair.passage).iter().any(needs_judge)
}

fn assert_judged(ids: &[&str]) {
    for id in ids {
        assert!(reaches_judge(id), "{id} was settled without the judge");
    }
}

#[test]
fn a_dropped_negation_reaches_the_judge() {
    // "a manager cannot grant one" read back as "A manager can grant an
    // exception"; "it is not let back early" as "can be let back early".
    assert_judged(&["cp-025", "cp-088"]);
}

#[test]
fn a_swapped_entity_built_from_the_passage_words_reaches_the_judge() {
    // The Halloway bench owns the storage, not the pager; the supplier, not
    // the customer, bears the cost of a repeat visit.
    assert_judged(&["cp-004", "cp-055"]);
}

#[test]
fn a_reversal_reaches_the_judge() {
    // Readings reordered in place read back as dropped; the restart that
    // "just happened to come next" read back as the fix.
    assert_judged(&["cp-018", "cp-072"]);
}

#[test]
fn an_extra_unsupported_clause_reaches_the_judge() {
    // One clause copied from the passage, one the passage never states.
    assert_judged(&["cp-012", "cp-016", "cp-045", "cp-052", "cp-067", "cp-078"]);
}

#[test]
fn a_widened_scope_reaches_the_judge() {
    // Production rings widened to staging; exports above a threshold
    // widened to every export.
    assert_judged(&["cp-037", "cp-047"]);
}

#[test]
fn world_knowledge_the_passage_never_states_reaches_the_judge() {
    assert_judged(&["cp-038", "cp-098"]);
}

#[test]
fn a_swapped_weekday_reaches_the_judge() {
    // The passage assigns the Thursday window; the claim says Tuesday.
    assert_judged(&["cp-020"]);
}

#[test]
fn no_false_pair_in_the_dataset_is_settled_without_the_judge() {
    let mut settled: Vec<&str> = dataset()
        .iter()
        .filter(|(_, pair)| pair.label != "supported")
        .filter(|(id, _)| !reaches_judge(id))
        .map(|(id, _)| id.as_str())
        .collect();
    settled.sort_unstable();
    assert!(settled.is_empty(), "settled on wording alone: {settled:?}");
}

#[test]
fn a_near_verbatim_claim_still_skips_the_judge() {
    // A first-checks bullet, word for word.
    assert!(!reaches_judge("cp-019"));

    // Two copied sentences. The first is its source sentence whole. The
    // second stops before "...and a device without an escrowed key is
    // refused on the network after 45 calendar days", so what it cut
    // carries a negation and a number, and the judge reads it.
    let pair = pair("cp-029");
    let claims = assessed(&pair.claim, &pair.passage);
    assert_eq!(claims.len(), 2);
    assert!(claims.iter().all(|claim| claim.near_verbatim));
    assert!(!needs_judge(&claims[0]));
    assert!(needs_judge(&claims[1]));
}

#[test]
fn a_copy_that_cuts_a_negation_number_date_or_name_reaches_the_judge() {
    // Each claim is a dataset passage's own words with part of the sentence
    // cut, so it passes the near-verbatim test. What it cut is the point.
    for (id, claim) in [
        // "Do not widen the pool on Garnetline ..."
        (
            "cp-013",
            "Widen the pool on Garnetline to work around a stale lease takeover.",
        ),
        // "... is disabled automatically 32 weeks after its last successful sign-in ..."
        ("cp-045", "A contractor account is disabled automatically."),
        // "The July capacity review moved Garnetline ..."
        (
            "cp-016",
            "Capacity review moved Garnetline onto the wider shard map for Drummel Flats.",
        ),
        // "The Thursday rollout window for probe-registry ..."
        (
            "cp-019",
            "Rollout window for probe-registry belongs to the Tarrow data group unless Dagny Oreskel has swapped it.",
        ),
        // "Lev Ostrander flagged that the handover notes ..."
        (
            "cp-067",
            "The handover notes from Wendover Basin are still arriving as photographs.",
        ),
    ] {
        let claims = assessed(claim, &pair(id).passage);
        let first = claims.first().expect("one claim");
        assert!(first.near_verbatim, "{id}: {claim:?} is not a copy");
        assert!(needs_judge(first), "{id}: {claim:?} skipped the judge");
    }
}
