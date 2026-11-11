use super::*;
use std::collections::HashSet;

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn the_catalogue_has_unique_ids_and_names_and_every_family() {
    assert!(
        PROBLEM_PATTERNS.len() >= 18,
        "the design asks for about twenty patterns"
    );
    let unique_ids: HashSet<_> = PROBLEM_PATTERNS.iter().map(|pattern| pattern.id).collect();
    assert_eq!(unique_ids.len(), PROBLEM_PATTERNS.len());
    let unique_names: HashSet<_> = PROBLEM_PATTERNS
        .iter()
        .map(|pattern| pattern.name.to_lowercase())
        .collect();
    assert_eq!(unique_names.len(), PROBLEM_PATTERNS.len());
    for family in PatternFamily::ALL {
        assert!(
            PROBLEM_PATTERNS
                .iter()
                .any(|pattern| pattern.family == family),
            "no pattern in the {} family",
            family.as_str()
        );
    }
    for pattern in PROBLEM_PATTERNS {
        assert!(!pattern.cue.trim().is_empty(), "{} has no cue", pattern.id);
    }
}

#[test]
fn a_pattern_is_found_by_its_id() {
    let found = problem_pattern("hash_map_lookup").map(|pattern| pattern.name);
    assert_eq!(found, Some("Hash map lookup"));
    assert!(problem_pattern("not_a_pattern").is_none());
}

#[test]
fn a_name_is_found_as_a_whole_phrase_ignoring_case() {
    let found = named_in(
        "Use a HASH MAP lookup to answer each query.",
        &ids(&["hash_map_lookup"]),
    );
    assert_eq!(found.map(|pattern| pattern.id), Some("hash_map_lookup"));
    let punctuated = named_in("Try binary-search, then stop.", &ids(&["binary_search"]));
    assert_eq!(punctuated.map(|pattern| pattern.id), Some("binary_search"));
}

#[test]
fn an_alias_is_found_like_a_name() {
    let alias = PROBLEM_PATTERNS
        .iter()
        .find_map(|pattern| pattern.aliases.first().map(|alias| (pattern.id, *alias)));
    let Some((id, alias)) = alias else {
        panic!("no pattern has an alias");
    };
    let text = format!("Think about {} here.", alias.to_uppercase());
    assert_eq!(
        named_in(&text, &ids(&[id])).map(|pattern| pattern.id),
        Some(id)
    );
}

#[test]
fn a_name_inside_a_longer_word_is_not_found() {
    assert!(named_in("She retried the entries twice.", &ids(&["trie"])).is_none());
    assert!(named_in(
        "The greedy choices were not binary searching anything.",
        &ids(&["greedy_choice", "binary_search"])
    )
    .is_none());
}

#[test]
fn only_the_listed_patterns_are_looked_for() {
    let text = "Use a hash map lookup.";
    assert!(named_in(text, &ids(&["binary_search"])).is_none());
    assert!(named_in(text, &[]).is_none());
    assert!(named_in(text, &ids(&["no_such_pattern"])).is_none());
}

#[test]
fn the_first_listed_pattern_that_appears_wins() {
    let text = "Binary search it, or rather build a hash map lookup.";
    let found = named_in(text, &ids(&["hash_map_lookup", "binary_search"]));
    assert_eq!(found.map(|pattern| pattern.id), Some("hash_map_lookup"));
}

#[test]
fn a_statement_that_does_not_name_a_pattern_is_clean() {
    let text = "Given a list of numbers and a target, return the positions of two numbers that add up to it.";
    let all: Vec<String> = PROBLEM_PATTERNS
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect();
    assert!(named_in(text, &all).is_none());
}
