//! The catalogue is well-formed data: unique ids that match their families,
//! one-sentence definitions, known languages, and enough patterns.

use std::collections::HashSet;

use super::*;

/// What the design's section 3 lists as examples of each family.
const DESIGN_EXAMPLES: &[&str] = &[
    "correctness.off_by_one",
    "correctness.boundary_and_empty_input",
    "correctness.null_dereference",
    "correctness.wrong_operator",
    "correctness.mutated_argument",
    "correctness.stale_state",
    "errors.swallowed_exception",
    "errors.error_as_success",
    "errors.lost_context",
    "errors.retry_without_backoff",
    "errors.retry_not_idempotent",
    "concurrency.check_then_act",
    "concurrency.unsynchronised_shared_state",
    "concurrency.lock_ordering",
    "concurrency.missing_await",
    "concurrency.blocking_in_async",
    "resources.leaked_handle",
    "resources.unbounded_growth",
    "resources.missing_timeout",
    "security.sql_injection",
    "security.command_injection",
    "security.path_traversal",
    "security.missing_authorisation",
    "security.hardcoded_secret",
    "security.unsafe_deserialisation",
    "security.ssrf",
    "performance.n_plus_one_queries",
    "performance.quadratic_loop",
    "performance.repeated_work_in_loop",
    "performance.needless_allocation",
    "design.long_function",
    "design.feature_envy",
    "design.primitive_obsession",
    "design.leaky_abstraction",
    "design.temporal_coupling",
    "readability.misleading_name",
    "readability.magic_number",
    "readability.dead_code",
    "readability.duplicated_logic",
    "testing.untested_branch",
    "testing.assertion_free_test",
    "testing.coupled_to_implementation",
];

/// A sentence that starts with a capital, ends with a full stop and has no
/// other sentence end inside it.
fn is_one_sentence(text: &str) -> bool {
    text.ends_with('.')
        && !text.contains('\n')
        && !text.contains(". ")
        && !text.contains("? ")
        && !text.contains("! ")
        && text.chars().next().is_some_and(char::is_uppercase)
}

#[test]
fn the_check_for_one_sentence_tells_one_from_two() {
    assert!(is_one_sentence("A loop is one too long, so it fails."));
    assert!(!is_one_sentence("It fails. It is wrong."));
    assert!(!is_one_sentence("It fails"));
    assert!(!is_one_sentence("it fails."));
    assert!(!is_one_sentence("Why does it fail? It does."));
}

#[test]
fn ids_are_unique() {
    let ids: HashSet<&str> = CATALOGUE.iter().map(|pattern| pattern.id).collect();
    assert_eq!(ids.len(), CATALOGUE.len());
}

#[test]
fn names_are_unique_and_not_empty() {
    let names: HashSet<&str> = CATALOGUE.iter().map(|pattern| pattern.name).collect();
    assert_eq!(names.len(), CATALOGUE.len());
    assert!(CATALOGUE
        .iter()
        .all(|pattern| !pattern.name.trim().is_empty()));
}

#[test]
fn an_id_is_its_familys_slug_a_dot_and_a_lowercase_name() {
    for pattern in CATALOGUE {
        let name = pattern
            .id
            .strip_prefix(pattern.family.slug())
            .and_then(|rest| rest.strip_prefix('.'))
            .unwrap_or_else(|| panic!("{} is not in {:?}", pattern.id, pattern.family));
        assert!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{}",
            pattern.id
        );
    }
}

#[test]
fn a_familys_slug_is_its_name_in_the_wire_form() {
    for family in Family::ALL {
        assert_eq!(
            serde_json::to_string(&family).unwrap(),
            format!("\"{}\"", family.slug())
        );
    }
}

#[test]
fn every_definition_is_one_sentence() {
    for pattern in CATALOGUE {
        assert!(is_one_sentence(pattern.definition), "{}", pattern.id);
    }
}

#[test]
fn every_source_is_given() {
    for pattern in CATALOGUE {
        assert!(!pattern.source.trim().is_empty(), "{}", pattern.id);
    }
}

#[test]
fn every_security_pattern_cites_a_cwe_id() {
    let security: Vec<&Pattern> = CATALOGUE
        .iter()
        .filter(|pattern| pattern.family == Family::Security)
        .collect();
    assert!(!security.is_empty());
    for pattern in security {
        let digits = pattern.source.strip_prefix("CWE-").unwrap_or_default();
        assert!(
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
            "{}: {}",
            pattern.id,
            pattern.source
        );
    }
}

#[test]
fn every_only_names_known_languages() {
    for pattern in CATALOGUE {
        if let Languages::Only(languages) = pattern.languages {
            assert!(!languages.is_empty(), "{}", pattern.id);
            let distinct: HashSet<&&str> = languages.iter().collect();
            assert_eq!(distinct.len(), languages.len(), "{}", pattern.id);
            for language in languages {
                assert!(
                    LANGUAGES.contains(language),
                    "{} names {language}",
                    pattern.id
                );
            }
        }
    }
}

#[test]
fn the_language_ids_are_lowercase_and_unique() {
    let distinct: HashSet<&&str> = LANGUAGES.iter().collect();
    assert_eq!(distinct.len(), LANGUAGES.len());
    assert!(LANGUAGES
        .iter()
        .all(|language| *language == language.to_ascii_lowercase()));
}

#[test]
fn the_catalogue_lists_its_families_in_order() {
    let order: Vec<Family> = CATALOGUE.iter().map(|pattern| pattern.family).collect();
    assert!(order.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(Family::ALL.to_vec(), {
        let mut seen = order.clone();
        seen.dedup();
        seen
    });
}

#[test]
fn the_designs_examples_are_all_in_the_catalogue() {
    for id in DESIGN_EXAMPLES {
        assert!(pattern(id).is_some(), "{id}");
    }
}

/// The language ids a pattern applies to.
fn applying(id: &str) -> HashSet<&'static str> {
    let found = pattern(id).unwrap();
    LANGUAGES
        .iter()
        .copied()
        .filter(|language| found.applies_to(language))
        .collect()
}

fn set(languages: &[&'static str]) -> HashSet<&'static str> {
    languages.iter().copied().collect()
}

#[test]
fn the_patterns_added_after_review_apply_where_their_language_has_the_thing() {
    let all = set(&LANGUAGES);
    let programming = set(&[
        "python",
        "javascript",
        "typescript",
        "rust",
        "go",
        "java",
        "csharp",
        "cpp",
        "c",
        "kotlin",
        "swift",
        "ruby",
        "php",
    ]);
    let web = set(&[
        "javascript",
        "typescript",
        "python",
        "ruby",
        "php",
        "java",
        "csharp",
        "kotlin",
        "go",
    ]);
    let async_await = set(&[
        "python",
        "javascript",
        "typescript",
        "csharp",
        "rust",
        "swift",
        "kotlin",
    ]);
    let without_c: HashSet<&str> = programming.iter().copied().filter(|l| *l != "c").collect();
    let cases = [
        ("correctness.float_for_exact_values", all.clone()),
        ("correctness.modified_while_iterating", programming.clone()),
        (
            "correctness.loop_variable_capture",
            set(&["python", "javascript", "typescript", "csharp"]),
        ),
        (
            "correctness.reference_equality",
            set(&["java", "csharp", "python", "javascript", "typescript"]),
        ),
        ("correctness.naive_datetime", all.clone()),
        (
            "errors.overly_broad_catch",
            set(&[
                "python",
                "javascript",
                "typescript",
                "java",
                "csharp",
                "cpp",
                "kotlin",
                "swift",
                "ruby",
                "php",
            ]),
        ),
        ("errors.partial_update_without_rollback", all.clone()),
        (
            "concurrency.lock_held_across_slow_call",
            programming.clone(),
        ),
        (
            "concurrency.double_checked_locking",
            set(&["java", "cpp", "csharp", "kotlin"]),
        ),
        (
            "resources.lapsed_listener",
            set(&[
                "javascript",
                "typescript",
                "java",
                "csharp",
                "kotlin",
                "swift",
            ]),
        ),
        ("security.weak_randomness", programming.clone()),
        ("security.timing_unsafe_comparison", programming.clone()),
        ("security.mass_assignment", web.clone()),
        ("security.open_redirect", web.clone()),
        ("security.missing_csrf_protection", web.clone()),
        (
            "performance.string_building_in_loop",
            set(&["java", "csharp", "kotlin", "go", "python"]),
        ),
        ("performance.sequential_awaits", async_await.clone()),
        ("design.repeated_switches", programming.clone()),
        ("design.large_class", without_c),
        ("design.flag_argument", programming.clone()),
        ("design.data_clumps", programming.clone()),
        ("design.speculative_generality", programming.clone()),
        ("design.hardwired_dependency", programming.clone()),
        (
            "readability.loop_instead_of_pipeline",
            set(&[
                "python",
                "javascript",
                "typescript",
                "rust",
                "java",
                "csharp",
                "kotlin",
                "swift",
                "ruby",
                "php",
            ]),
        ),
        ("readability.complex_conditional", all),
    ];
    for (id, expected) in cases {
        assert_eq!(applying(id), expected, "{id}");
    }
}

#[test]
fn each_family_leads_with_what_comes_up_most_in_review_interviews() {
    let leaders: Vec<&str> = Family::ALL
        .iter()
        .filter_map(|&family| CATALOGUE.iter().find(|pattern| pattern.family == family))
        .map(|pattern| pattern.id)
        .collect();
    assert_eq!(
        leaders,
        [
            "correctness.null_dereference",
            "errors.swallowed_exception",
            "concurrency.check_then_act",
            "resources.leaked_handle",
            "security.sql_injection",
            "performance.n_plus_one_queries",
            "design.long_function",
            "readability.misleading_name",
            "testing.untested_branch",
        ]
    );
}

#[test]
fn missing_await_cites_where_it_is_named() {
    let source = pattern("concurrency.missing_await").unwrap().source;
    assert!(source.contains("asyncio") && source.contains("no-floating-promises"));
}

#[test]
fn every_language_has_a_pattern_in_every_family() {
    for language in LANGUAGES {
        for family in Family::ALL {
            assert!(
                patterns_for(language).any(|pattern| pattern.family == family),
                "{language} has no {family:?} pattern"
            );
        }
    }
}

#[test]
fn a_pattern_is_found_by_its_id() {
    let found = pattern("concurrency.check_then_act").unwrap();
    assert_eq!(found.name, "Check-then-act race");
    assert_eq!(found.family, Family::Concurrency);
    assert!(pattern("concurrency.nothing").is_none());
    assert!(pattern("Concurrency.check_then_act").is_none());
    assert!(pattern("").is_none());
}

#[test]
fn a_language_gets_the_patterns_that_apply_to_it_in_the_catalogues_order() {
    let python: Vec<&str> = patterns_for("python").map(|pattern| pattern.id).collect();
    assert!(python.contains(&"correctness.mutable_default_argument"));
    assert!(!python.contains(&"errors.unwrap_on_fallible"));
    assert!(python.contains(&"security.sql_injection"));
    let rust: Vec<&str> = patterns_for("rust").map(|pattern| pattern.id).collect();
    assert!(rust.contains(&"errors.unwrap_on_fallible"));
    assert!(!rust.contains(&"correctness.mutable_default_argument"));
    let positions: Vec<usize> = python
        .iter()
        .filter_map(|id| CATALOGUE.iter().position(|pattern| pattern.id == *id))
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn a_language_the_catalogue_does_not_know_gets_only_the_patterns_for_any() {
    let unknown: Vec<&Pattern> = patterns_for("cobol").collect();
    assert!(!unknown.is_empty());
    assert!(unknown
        .iter()
        .all(|pattern| pattern.languages == Languages::Any));
}

#[test]
fn only_applies_to_the_languages_it_lists_and_any_to_all() {
    assert!(Languages::Any.includes("python"));
    assert!(Languages::Any.includes("anything"));
    let some = Languages::Only(&["go", "rust"]);
    assert!(some.includes("go"));
    assert!(some.includes("rust"));
    assert!(!some.includes("python"));
    assert!(!some.includes("Go"));
}

#[test]
fn a_pattern_is_written_with_its_languages_and_family_in_snake_case() {
    let only = pattern("errors.unwrap_on_fallible").unwrap();
    let written = serde_json::to_value(only).unwrap();
    assert_eq!(written["family"], "errors");
    assert_eq!(written["languages"]["only"][0], "rust");
    let any = pattern("errors.swallowed_exception").unwrap();
    assert_eq!(serde_json::to_value(any).unwrap()["languages"], "any");
}
