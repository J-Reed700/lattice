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
        .filter(|language| found.applies_to(&Material::code(*language)))
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
            "fidelity.misquoted_figure",
            "scope.overgeneralisation",
            "reasoning.correlation_as_cause",
            "definition.wrong_definition",
            "consistency.self_contradiction",
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
        for family in Family::CODE {
            assert!(
                patterns_for(&Material::code(language)).any(|pattern| pattern.family == family),
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
    let python: Vec<&str> = patterns_for(&Material::code("python"))
        .map(|pattern| pattern.id)
        .collect();
    assert!(python.contains(&"correctness.mutable_default_argument"));
    assert!(!python.contains(&"errors.unwrap_on_fallible"));
    assert!(python.contains(&"security.sql_injection"));
    let rust: Vec<&str> = patterns_for(&Material::code("rust"))
        .map(|pattern| pattern.id)
        .collect();
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
    let unknown: Vec<&Pattern> = patterns_for(&Material::code("cobol")).collect();
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

fn passage_ids() -> Vec<&'static str> {
    patterns_for(&Material::Passage)
        .map(|pattern| pattern.id)
        .collect()
}

#[test]
fn a_passage_gets_only_the_passage_families() {
    let chosen: Vec<&Pattern> = patterns_for(&Material::Passage).collect();
    assert!(!chosen.is_empty());
    assert!(chosen
        .iter()
        .all(|pattern| Family::PASSAGE.contains(&pattern.family)));
    for family in Family::PASSAGE {
        assert!(chosen.iter().any(|pattern| pattern.family == family));
    }
}

#[test]
fn code_gets_none_of_the_passage_patterns() {
    for language in LANGUAGES.iter().copied().chain(["cobol"]) {
        assert!(
            patterns_for(&Material::code(language))
                .all(|pattern| Family::CODE.contains(&pattern.family)),
            "{language}"
        );
    }
}

#[test]
fn the_code_and_passage_families_are_all_of_them_and_do_not_share() {
    let joined: Vec<Family> = Family::CODE.into_iter().chain(Family::PASSAGE).collect();
    assert_eq!(joined, Family::ALL.to_vec());
}

#[test]
fn a_passage_has_the_designs_patterns() {
    let ids = passage_ids();
    for id in [
        "fidelity.misquoted_figure",
        "fidelity.wrong_unit_or_magnitude",
        "fidelity.swapped_entities",
        "fidelity.wrong_attribution",
        "fidelity.wrong_date_or_order",
        "scope.overgeneralisation",
        "scope.dropped_qualifier",
        "scope.shifted_population",
        "reasoning.correlation_as_cause",
        "reasoning.reversed_relation",
        "reasoning.unsupported_mechanism",
        "reasoning.non_sequitur",
        "definition.wrong_definition",
        "definition.conflated_terms",
        "consistency.self_contradiction",
    ] {
        assert!(ids.contains(&id), "{id}");
    }
    assert_eq!(ids.len(), 15);
}

#[test]
fn passage_patterns_apply_only_to_a_passage() {
    for id in passage_ids() {
        let found = pattern(id).unwrap();
        assert_eq!(found.languages, Languages::Passage, "{id}");
        assert!(found.applies_to(&Material::Passage), "{id}");
        for language in LANGUAGES {
            assert!(!found.applies_to(&Material::code(language)), "{id}");
        }
    }
    assert!(!Languages::Passage.includes("python"));
}

#[test]
fn code_patterns_do_not_apply_to_a_passage() {
    for found in CATALOGUE
        .iter()
        .filter(|pattern| Family::CODE.contains(&pattern.family))
    {
        assert!(!found.applies_to(&Material::Passage), "{}", found.id);
    }
}

#[test]
fn a_passage_pattern_is_written_with_its_languages_as_passage() {
    let found = pattern("scope.dropped_qualifier").unwrap();
    assert_eq!(serde_json::to_value(found).unwrap()["languages"], "passage");
}

/// The patterns python got before the catalogue had passage patterns, in order.
const PYTHON_BEFORE_PASSAGES: &[&str] = &[
    "correctness.null_dereference",
    "correctness.off_by_one",
    "correctness.boundary_and_empty_input",
    "correctness.wrong_operator",
    "correctness.mutated_argument",
    "correctness.mutable_default_argument",
    "correctness.reference_equality",
    "correctness.stale_state",
    "correctness.float_for_exact_values",
    "correctness.modified_while_iterating",
    "correctness.loop_variable_capture",
    "correctness.naive_datetime",
    "errors.swallowed_exception",
    "errors.error_as_success",
    "errors.overly_broad_catch",
    "errors.partial_update_without_rollback",
    "errors.lost_context",
    "errors.retry_without_backoff",
    "errors.retry_not_idempotent",
    "concurrency.check_then_act",
    "concurrency.unsynchronised_shared_state",
    "concurrency.lock_ordering",
    "concurrency.missing_await",
    "concurrency.blocking_in_async",
    "concurrency.lock_held_across_slow_call",
    "resources.leaked_handle",
    "resources.unbounded_growth",
    "resources.missing_timeout",
    "resources.unbounded_result_set",
    "security.sql_injection",
    "security.missing_authorisation",
    "security.hardcoded_secret",
    "security.xss",
    "security.command_injection",
    "security.path_traversal",
    "security.sensitive_data_in_logs",
    "security.weak_cryptography",
    "security.ssrf",
    "security.unsafe_deserialisation",
    "security.missing_csrf_protection",
    "security.weak_randomness",
    "security.mass_assignment",
    "security.open_redirect",
    "security.missing_certificate_validation",
    "security.timing_unsafe_comparison",
    "security.code_injection",
    "performance.n_plus_one_queries",
    "performance.quadratic_loop",
    "performance.repeated_work_in_loop",
    "performance.string_building_in_loop",
    "performance.sequential_awaits",
    "performance.needless_allocation",
    "performance.catastrophic_regex",
    "design.long_function",
    "design.large_class",
    "design.hardwired_dependency",
    "design.global_mutable_state",
    "design.long_parameter_list",
    "design.repeated_switches",
    "design.primitive_obsession",
    "design.feature_envy",
    "design.flag_argument",
    "design.leaky_abstraction",
    "design.data_clumps",
    "design.temporal_coupling",
    "design.message_chains",
    "design.speculative_generality",
    "readability.misleading_name",
    "readability.duplicated_logic",
    "readability.magic_number",
    "readability.deep_nesting",
    "readability.complex_conditional",
    "readability.dead_code",
    "readability.misleading_comment",
    "readability.loop_instead_of_pipeline",
    "testing.untested_branch",
    "testing.assertion_free_test",
    "testing.coupled_to_implementation",
    "testing.nondeterministic_test",
    "testing.shared_state_between_tests",
];

#[test]
fn python_gets_the_patterns_it_got_before_the_passage_ones() {
    let python: Vec<&str> = patterns_for(&Material::code("python"))
        .map(|pattern| pattern.id)
        .collect();
    assert_eq!(python, PYTHON_BEFORE_PASSAGES);
}

#[test]
fn the_language_most_fences_are_written_in_is_the_codes() {
    let texts = [
        "Intro.\n```python\nprint(1)\n```\n",
        "```rust\nfn main() {}\n```\n```python\nx = 1\n```\n",
    ];
    assert_eq!(language_of_code(&texts), Some("python"));
}

#[test]
fn a_fence_names_its_language_by_an_alias() {
    for (info, language) in [
        ("py", "python"),
        ("ts", "typescript"),
        ("rs", "rust"),
        ("c++", "cpp"),
        ("cs", "csharp"),
        ("golang", "go"),
        ("bash", "shell"),
        ("JS", "javascript"),
        (".python", "python"),
        ("postgresql", "sql"),
        ("kt", "kotlin"),
    ] {
        let text = format!("```{info}\ncode\n```\n");
        assert_eq!(language_of_code(&[text.as_str()]), Some(language), "{info}");
    }
}

#[test]
fn closing_fences_and_fences_with_no_language_count_for_nothing() {
    let text = "```\nplain\n```\n```rust\nlet x = 1;\n```\n";
    assert_eq!(language_of_code(&[text]), Some("rust"));
    assert_eq!(language_of_code(&["```\nplain\n```\n"]), None);
}

#[test]
fn a_closing_fence_is_not_read_as_an_opening_one() {
    let text = "```python\nx\n```\n```go\ny\n```\n```go\nz\n```\n";
    assert_eq!(language_of_code(&[text]), Some("go"));
    let one = "```python\nx\n```python\nnot a block\n";
    assert_eq!(language_of_code(&[one]), Some("python"));
}

#[test]
fn fences_in_no_language_of_the_catalogue_are_ignored() {
    let text = "```yaml\na: 1\n```\n```json\n{}\n```\n```powershell\nls\n```\n```text\nx\n```\n";
    assert_eq!(language_of_code(&[text]), None);
}

#[test]
fn a_tie_goes_to_the_language_listed_first() {
    let text = "```rust\na\n```\n```python\nb\n```\n";
    assert_eq!(language_of_code(&[text]), Some("python"));
    let text = "```shell\na\n```\n```go\nb\n```\n";
    assert_eq!(language_of_code(&[text]), Some("go"));
}

#[test]
fn tilde_fences_and_fences_indented_up_to_three_spaces_count() {
    assert_eq!(language_of_code(&["~~~python\nx\n~~~\n"]), Some("python"));
    assert_eq!(language_of_code(&["   ```rust\nx\n   ```\n"]), Some("rust"));
    assert_eq!(language_of_code(&["````go\nx\n````\n"]), Some("go"));
}

#[test]
fn a_fence_indented_four_spaces_does_not_count() {
    assert_eq!(language_of_code(&["    ```python\n    x\n    ```\n"]), None);
}

#[test]
fn an_info_string_with_attributes_names_its_first_word() {
    assert_eq!(
        language_of_code(&["```python {linenos}\nx\n```\n"]),
        Some("python")
    );
    assert_eq!(
        language_of_code(&["```rust title=x\nx\n```\n"]),
        Some("rust")
    );
    assert_eq!(
        language_of_code(&["```python{.numberLines}\nx\n```\n"]),
        Some("python")
    );
}

#[test]
fn no_texts_name_no_language() {
    assert_eq!(language_of_code(&[]), None);
    assert_eq!(language_of_code(&["No code here."]), None);
}
