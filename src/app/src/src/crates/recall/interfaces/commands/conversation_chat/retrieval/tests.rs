use super::*;
use std::collections::{HashMap, HashSet};

#[test]
fn clarify_response_without_recent_document_is_generic() {
    let settings = RouterSettingsDto::default();

    let response = build_router_clarify_response(&settings, &None, "question", None);

    assert!(response.contains("I don't see a recent linked document"));
    assert!(!response.contains("previous document"));
}

#[test]
fn retrieval_plan_does_not_short_circuit_clarify_without_recent_document() {
    let settings = RouterSettingsDto::default();
    let decision = RouterDecisionOutcome {
        action: RouterAction::Clarify,
        recent_doc_meta: None,
        clarify_message: None,
    };
    let flags = SearchFlags {
        force_kb_search: false,
        force_web_search: false,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
    };

    let plan = RetrievalPlan::from_router(&settings, &decision, "test", flags);

    assert_eq!(plan.route_action, RouterAction::NewSearch);
    assert!(plan.short_circuit_response.is_none());
    assert!(plan.should_search_kb);
}

#[test]
fn external_lookup_is_skipped_when_kb_is_strong_and_used_as_primary() {
    let should_run = should_execute_external_lookup(
        true,  // external_as_fallback
        true,  // kb_attempted
        true,  // kb_has_results
        false, // kb_low_confidence
    );
    assert!(!should_run);
}

#[test]
fn external_lookup_runs_when_kb_is_weak_or_missing() {
    assert!(should_execute_external_lookup(true, true, false, true));
    assert!(should_execute_external_lookup(true, true, true, true));
    assert!(should_execute_external_lookup(true, false, false, true));
}

#[test]
fn followup_with_forced_web_prefers_kb_first_without_external_intent() {
    let flags = SearchFlags {
        force_kb_search: true,
        force_web_search: true,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
    };
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::raw_only(
        "Right, I am talking about a scenario with my company",
        QueryType::Followup,
    );

    let fallback = should_use_external_as_fallback(flags, &interpretation);
    assert!(fallback);
}

#[test]
fn question_with_forced_web_keeps_web_enabled() {
    let flags = SearchFlags {
        force_kb_search: true,
        force_web_search: true,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
    };
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::raw_only(
        "latest updates?",
        QueryType::Question,
    );

    let fallback = should_use_external_as_fallback(flags, &interpretation);
    assert!(!fallback);
}

#[test]
fn kb_plan_runs_parallel_when_hyde_differs_without_forced_kb() {
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::for_question(
        "What's new with NASA?",
        "NASA announced new mission timelines and propulsion tests.",
    );
    let flags = SearchFlags {
        force_kb_search: false,
        force_web_search: false,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
    };

    let plan =
        KbSearchPlan::from_interpretation("What's new with NASA?", &interpretation, flags, 0.4);

    assert!(plan.run_parallel_keyword_branch);
    assert!(matches!(plan.mode, SearchModeDto::Vector));
    assert_eq!(plan.threshold, Some(0.05));
}

#[test]
fn kb_plan_prefers_hybrid_for_forced_kb_without_distinct_hyde() {
    let interpretation =
        crate::domain::qa::hyde::HyDEInterpretation::raw_only("policy update", QueryType::Question);
    let flags = SearchFlags {
        force_kb_search: true,
        force_web_search: false,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
    };

    let plan = KbSearchPlan::from_interpretation("policy update", &interpretation, flags, 0.4);

    assert!(!plan.run_parallel_keyword_branch);
    assert_eq!(plan.threshold, Some(0.4));
    match plan.mode {
        SearchModeDto::Hybrid {
            vector_weight,
            bm25_weight,
        } => {
            assert!((vector_weight - 0.7).abs() < f32::EPSILON);
            assert!((bm25_weight - 0.3).abs() < f32::EPSILON);
        }
        mode => panic!("expected hybrid mode, got {:?}", mode),
    }
}

fn make_result(id: &str, doc_id: &str, title: &str, content: &str, score: f32) -> SearchResultDto {
    SearchResultDto {
        id: id.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        score,
        path: None,
        document_id: Some(doc_id.to_string()),
        position: None,
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: HashMap::new(),
    }
}

#[test]
fn keyword_plan_retains_informative_terms_and_structural_guards() {
    let plan = build_keyword_query_plan("what is the correlation in studies", None);
    assert!(plan.terms.iter().all(|term| term.len() >= 3));
    assert!(plan
        .terms
        .iter()
        .all(|term| term.chars().any(|c| c.is_ascii_alphabetic())));
    assert!(plan.terms.iter().any(|term| term.contains("correlation")));
    assert!(plan
        .terms
        .iter()
        .any(|term| term.contains("study") || term.contains("studi")));
}

#[test]
fn keyword_plan_prioritizes_hyde_lexical_signal_when_present() {
    let query = "what can i do about my employee contract at bigtime";
    let hyde = Some(
            "Search BigTime employee contract terms focusing on noncompete non-disclosure and IP assignment restrictions.",
        );

    let with_hyde = build_keyword_query_plan(query, hyde);
    let without_hyde = build_keyword_query_plan(query, None);

    let hyde_alignment_with = with_hyde
        .terms
        .iter()
        .filter(|term| {
            term.contains("noncompete")
                || term.contains("disclosure")
                || term.contains("assign")
                || term.contains("restrict")
        })
        .count();
    let hyde_alignment_without = without_hyde
        .terms
        .iter()
        .filter(|term| {
            term.contains("noncompete")
                || term.contains("disclosure")
                || term.contains("assign")
                || term.contains("restrict")
        })
        .count();

    assert!(hyde_alignment_with >= hyde_alignment_without);
}

#[test]
fn keyword_plan_with_hyde_suppresses_short_singletons() {
    let query = "what can i do and what can we do from this and that contract at bigtime";
    let hyde = Some(
            "Review employment agreement clauses including confidentiality, assignment, and restrictive covenants.",
        );

    let plan = build_keyword_query_plan(query, hyde);
    let has_short_singleton = plan.terms.iter().any(|term| {
        let tokens = tokenize_keyword_terms(term);
        tokens.len() == 1 && tokens[0].len() < 5
    });

    assert!(!has_short_singleton);
}

#[test]
fn overlap_filter_requires_two_hits_for_two_term_query() {
    let results = vec![
        make_result(
            "r1",
            "d1",
            "single match",
            "this text mentions studies only",
            0.9,
        ),
        make_result(
            "r2",
            "d2",
            "double match",
            "this discusses correlations between studies in detail",
            0.8,
        ),
    ];

    let filtered =
        filter_results_by_query_overlap(results, "any correlations between studies?", None, None);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "r2");
}

#[test]
fn overlap_term_selection_downweights_interrogative_lead_token() {
    let query = "What is the capital of France?";
    let terms = extract_overlap_query_terms(query);
    let results = vec![
        make_result(
            "r1",
            "d1",
            "Capital of France",
            "The capital city of France is discussed in geography references.",
            1.0,
        ),
        make_result(
            "r2",
            "d2",
            "French administrative center",
            "France has a capital and major administrative center.",
            0.8,
        ),
        make_result(
            "r3",
            "d3",
            "Interview transcript",
            "What people ask in interviews varies by topic.",
            0.45,
        ),
    ];

    let selected = select_informative_overlap_terms(
        terms,
        query,
        Some("capital city france administrative center"),
        &results,
    );
    assert!(selected.iter().any(|term| term == "capital"));
    assert!(selected.iter().any(|term| term == "france"));
    assert!(!selected.iter().any(|term| term == "what"));
}

#[test]
fn informative_overlap_terms_drop_ubiquitous_low_signal_term() {
    let terms = vec![
        "there".to_string(),
        "blueberries".to_string(),
        "anthocyanin".to_string(),
    ];
    let results = vec![
        make_result(
            "r1",
            "d1",
            "there blueberries anthocyanin",
            "there blueberries anthocyanin profile",
            1.0,
        ),
        make_result(
            "r2",
            "d2",
            "there blueberries",
            "there blueberries yield",
            0.85,
        ),
        make_result("r3", "d3", "there studies", "there studies stress", 0.6),
        make_result("r4", "d4", "there report", "there report methods", 0.55),
    ];

    let selected =
        select_informative_overlap_terms(terms, "there blueberries anthocyanin", None, &results);
    assert!(!selected.iter().any(|term| term == "there"));
    assert!(selected
        .iter()
        .any(|term| term == "blueberries" || term == "anthocyanin"));
}

#[test]
fn overlap_filter_followup_requires_previous_turn_anchor_overlap() {
    let results = vec![
        make_result(
            "r1",
            "d1",
            "blueberry health profile",
            "blueberries provide antioxidant health benefits in multiple studies",
            0.91,
        ),
        make_result(
            "r2",
            "d2",
            "arugula glucosinolate effects",
            "arugula glucosinolate compounds have health benefits through isothiocyanates",
            0.76,
        ),
    ];
    let anchors: HashSet<String> = ["arugula".to_string(), "glucosinolate".to_string()]
        .into_iter()
        .collect();
    let filtered = filter_results_by_query_overlap(
        results,
        "What health benefits do those compounds provide?",
        Some(
            "What health benefits are linked to glucosinolate-derived isothiocyanates in arugula?",
        ),
        Some(&anchors),
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "r2");
}

#[test]
fn overlap_filter_short_followup_rescues_context_anchored_candidates() {
    let results = vec![
        make_result(
            "r1",
            "d1",
            "employment agreement restrictions",
            "company contract noncompete confidentiality obligations",
            0.91,
        ),
        make_result(
            "r2",
            "d2",
            "unrelated benefits guide",
            "medical dental vision enrollment details",
            0.77,
        ),
    ];
    let anchors: HashSet<String> = ["contract".to_string(), "noncompete".to_string()]
        .into_iter()
        .collect();

    let filtered = filter_results_by_query_overlap(
        results,
        "Right, scenario with my company",
        Some("Review contract and noncompete restrictions for follow-up scenario"),
        Some(&anchors),
    );

    assert!(!filtered.is_empty());
    assert!(filtered.iter().any(|result| result.id == "r1"));
}

#[test]
fn overlap_filter_question_does_not_require_followup_anchors() {
    let results = vec![
        make_result(
            "r1",
            "d1",
            "nitric oxide signaling pathways",
            "nitric oxide pathways from vascular signaling studies",
            0.88,
        ),
        make_result(
            "r2",
            "d2",
            "completely unrelated",
            "blueberry anthocyanin profile and health outcomes",
            0.77,
        ),
    ];

    let filtered = filter_results_by_query_overlap(
            results,
            "is the bitterness in arugula from nitric oxide?",
            Some(
                "Investigate glucosinolate metabolites in arugula and how nitric oxide influences flavor pathways.",
            ),
            None,
        );

    assert!(filtered.iter().any(|result| result.id == "r1"));
    assert!(!filtered.iter().any(|result| result.id == "r2"));
}

#[test]
fn overlap_filter_requires_hyde_anchor_for_broad_question_queries() {
    let results = vec![
        make_result(
            "r1",
            "d1",
            "general policy",
            "bigtime contract clauses and employment policy text",
            0.9,
        ),
        make_result(
            "r2",
            "d2",
            "confidentiality section",
            "bigtime contract clauses confidentiality assignment restrictions",
            0.85,
        ),
    ];

    let filtered = filter_results_by_query_overlap(
        results,
        "bigtime employee contract clauses legally startup ideas compete",
        Some("Review confidentiality and assignment restrictions in employment agreement clauses."),
        None,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "r2");
}

#[test]
fn web_query_prefers_raw_user_text_for_followups_when_specific() {
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::hybrid(
        "What health benefits do those compounds provide?",
        "What health benefits are linked to glucosinolate-derived isothiocyanates in arugula?",
        QueryType::Followup,
    );

    let raw = "What health benefits do those compounds provide?";
    let query = select_web_search_query(raw, &interpretation, None);

    assert_eq!(query, raw);
}

#[test]
fn web_query_prefers_raw_user_text_for_questions() {
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::for_question(
        "is the bitterness in arugula from nitric oxide?",
        "Investigate nitric oxide and glucosinolate pathways in arugula.",
    );

    let raw = "is the bitterness in arugula from nitric oxide?";
    let query = select_web_search_query(raw, &interpretation, None);
    assert_eq!(query, raw);
}

#[test]
fn web_query_prefers_raw_user_text_for_non_followup_questions() {
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::for_question(
        "What are the health benefits though?",
        "What health benefits are associated with glucosinolates and isothiocyanates in arugula?",
    );

    let raw = "What are the health benefits though?";
    let query = select_web_search_query(raw, &interpretation, None);
    assert_eq!(query, raw);
}

#[test]
fn web_query_falls_back_to_terms_for_generic_search_command() {
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::hybrid(
        "search web",
        "Search for the SAVE America Act bill text and Senate status updates.",
        QueryType::Command,
    );
    let anchors: HashSet<String> = ["save".to_string(), "america".to_string()]
        .into_iter()
        .collect();

    let query = select_web_search_query("search web", &interpretation, Some(&anchors));

    assert!(query.contains("save"));
    assert!(query.contains("america"));
    assert_ne!(query, "search web");
}

#[test]
fn hard_space_scope_filter_is_strict() {
    let response = SearchResponseDto {
        total: 2,
        query_time_ms: 5,
        results: vec![
            make_result("r1", "doc-a", "A", "alpha", 0.9),
            make_result("r2", "doc-b", "B", "beta", 0.8),
        ],
    };
    let scoped_ids: HashSet<String> = ["doc-b".to_string()].into_iter().collect();

    let filtered = apply_hard_space_scope_filter(response, "space_general", &scoped_ids);

    assert_eq!(filtered.total, 1);
    assert_eq!(filtered.results.len(), 1);
    assert_eq!(filtered.results[0].document_id.as_deref(), Some("doc-b"));
}

#[test]
fn hard_space_scope_filter_with_empty_scope_removes_all_results() {
    let response = SearchResponseDto {
        total: 2,
        query_time_ms: 5,
        results: vec![
            make_result("r1", "doc-a", "A", "alpha", 0.9),
            make_result("r2", "doc-b", "B", "beta", 0.8),
        ],
    };
    let scoped_ids: HashSet<String> = HashSet::new();

    let filtered = apply_hard_space_scope_filter(response, "space_general", &scoped_ids);

    assert_eq!(filtered.total, 0);
    assert!(filtered.results.is_empty());
}

#[test]
fn low_confidence_detection_flags_empty_or_weak_results() {
    let empty = SearchResponseDto {
        results: vec![],
        total: 0,
        query_time_ms: 0,
    };
    assert!(is_low_confidence_kb_response(&empty));

    let weak_single = SearchResponseDto {
        total: 1,
        query_time_ms: 5,
        results: vec![make_result("r1", "doc-1", "Title", "Snippet", 0.18)],
    };
    assert!(is_low_confidence_kb_response(&weak_single));
}

#[test]
fn low_confidence_detection_keeps_strong_results() {
    let strong = SearchResponseDto {
        total: 3,
        query_time_ms: 5,
        results: vec![
            make_result("r1", "doc-1", "Title", "Snippet", 0.61),
            make_result("r2", "doc-2", "Title", "Snippet", 0.39),
            make_result("r3", "doc-3", "Title", "Snippet", 0.21),
        ],
    };
    assert!(!is_low_confidence_kb_response(&strong));
}

#[test]
fn query_anchor_terms_favor_specific_entities() {
    let anchors = extract_query_anchor_terms("is the bitterness in arugula from nitric oxide?");
    assert!(anchors.contains("arugula"));
    assert!(anchors.contains("bitterness"));
    assert!(!anchors.contains("nitric"));
}

#[test]
fn turn_anchor_terms_capture_shared_topical_entities() {
    let user = "is the bitterness in arugula from nitric oxide?";
    let assistant = "Arugula bitterness comes from glucosinolates, not nitric oxide.";
    let anchors = extract_turn_anchor_terms(user, assistant);
    assert!(anchors.contains("arugula"));
    assert!(anchors.contains("bitterness"));
    assert!(!anchors.contains("nitric"));
}

#[test]
fn turn_anchor_terms_ignore_short_shared_tokens() {
    let user = "is NO involved in taste?";
    let assistant = "NO is a signaling molecule.";
    let anchors = extract_turn_anchor_terms(user, assistant);
    assert!(anchors.is_empty());
}

#[test]
fn turn_anchor_terms_drop_generic_shared_words() {
    let user = "give me information and related details about noncompete clauses in my contract";
    let assistant =
        "I can provide related information and details about noncompete clauses in your contract";
    let anchors = extract_turn_anchor_terms(user, assistant);
    assert!(anchors.contains("noncompete"));
    assert!(anchors.contains("clauses"));
    assert!(!anchors.is_empty());
}

#[test]
fn followup_anchor_coverage_filter_drops_ubiquitous_anchor() {
    let anchors: HashSet<String> = ["company".to_string(), "noncompete".to_string()]
        .into_iter()
        .collect();
    let results = vec![
        make_result(
            "r1",
            "d1",
            "Employment contract company",
            "company noncompete clause summary",
            0.9,
        ),
        make_result(
            "r2",
            "d2",
            "Benefits company guide",
            "company medical benefits overview",
            0.8,
        ),
        make_result(
            "r3",
            "d3",
            "Company policy handbook",
            "company values and handbook sections",
            0.7,
        ),
        make_result(
            "r4",
            "d4",
            "Restrictive covenants",
            "noncompete obligations and contract terms",
            0.6,
        ),
    ];

    let filtered = filter_followup_anchor_terms_by_candidate_coverage(anchors, &results);
    assert!(!filtered.contains("company"));
    assert!(filtered.contains("noncompete"));
}

#[test]
fn document_support_filter_drops_weak_multi_hit_document() {
    let results = vec![
        make_result("r1", "strong", "strong-1", "high-signal chunk", 1.0),
        make_result("r2", "strong", "strong-2", "high-signal chunk", 0.9),
        make_result("r3", "weak", "weak-1", "low-signal chunk", 0.08),
        make_result("r4", "weak", "weak-2", "low-signal chunk", 0.07),
    ];

    let filtered = filter_results_by_document_support(results);
    assert!(filtered
        .iter()
        .all(|result| result.document_id.as_deref() == Some("strong")));
    assert_eq!(filtered.len(), 2);
}

#[test]
fn document_support_filter_keeps_strong_single_hit_document() {
    let results = vec![
        make_result("r1", "strong", "strong-1", "high-signal chunk", 1.0),
        make_result("r2", "strong", "strong-2", "high-signal chunk", 0.92),
        make_result(
            "r3",
            "single-strong",
            "single-strong-1",
            "targeted legal clause chunk",
            0.88,
        ),
        make_result(
            "r4",
            "single-weak",
            "single-weak-1",
            "low-signal chunk",
            0.10,
        ),
    ];

    let filtered = filter_results_by_document_support(results);
    assert!(filtered
        .iter()
        .any(|result| result.document_id.as_deref() == Some("single-strong")));
    assert!(!filtered
        .iter()
        .any(|result| result.document_id.as_deref() == Some("single-weak")));
}

#[test]
fn rm3_feedback_rejects_unanchored_single_doc_terms() {
    let base = build_keyword_query_plan("correlations between studies", None);
    let feedback = vec![
        make_result(
            "r1",
            "d1",
            "plant study",
            "this discusses correlations in studies",
            1.0,
        ),
        make_result(
            "r2",
            "d2",
            "unrelated",
            "election integrity ballot audit procedures",
            0.9,
        ),
    ];

    let expanded =
        expand_keyword_plan_with_rm3(base, &feedback, "correlations between studies", None);
    assert!(!expanded.terms.iter().any(|term| term.contains("election")));
    assert!(expanded
        .terms
        .iter()
        .any(|term| term.contains("correlation") || term.contains("study")));
}

#[test]
fn rm3_feedback_uses_distinct_documents_for_df() {
    let base = build_keyword_query_plan("correlations between studies", None);
    let feedback = vec![
        make_result(
            "r1",
            "doc-unrelated",
            "unrelated chunk 1",
            "election integrity ballot audit procedures",
            1.0,
        ),
        make_result(
            "r2",
            "doc-unrelated",
            "unrelated chunk 2",
            "election recount integrity ballot process",
            0.95,
        ),
        make_result(
            "r3",
            "doc-query",
            "query aligned",
            "this discusses correlations between studies",
            0.8,
        ),
    ];

    let expanded =
        expand_keyword_plan_with_rm3(base, &feedback, "correlations between studies", None);
    assert!(!expanded.terms.iter().any(|term| term.contains("election")));
    assert!(expanded
        .terms
        .iter()
        .any(|term| term.contains("correlation") || term.contains("study")));
}

#[test]
fn shortlist_gate_skips_when_confidence_is_low() {
    let shortlist_response = SearchResponseDto {
        results: vec![
            make_result("r1", "doc-a", "a", "alpha", 1.0),
            make_result("r2", "doc-b", "b", "beta", 0.9),
        ],
        total: 2,
        query_time_ms: 1,
    };
    let shortlist: std::collections::HashSet<String> = ["doc-a".to_string(), "doc-b".to_string()]
        .into_iter()
        .collect();
    assert!(!should_apply_document_shortlist(
        &shortlist_response,
        &shortlist
    ));
}

#[test]
fn shortlist_filter_fails_open_on_over_prune() {
    let response = SearchResponseDto {
        results: vec![
            make_result("r1", "doc-a", "a", "alpha", 1.0),
            make_result("r2", "doc-c", "c", "charlie", 0.95),
            make_result("r3", "doc-d", "d", "delta", 0.9),
            make_result("r4", "doc-e", "e", "echo", 0.85),
            make_result("r5", "doc-f", "f", "foxtrot", 0.8),
            make_result("r6", "doc-g", "g", "golf", 0.75),
            make_result("r7", "doc-h", "h", "hotel", 0.7),
            make_result("r8", "doc-i", "i", "india", 0.65),
            make_result("r9", "doc-j", "j", "juliet", 0.6),
            make_result("r10", "doc-k", "k", "kilo", 0.55),
        ],
        total: 10,
        query_time_ms: 1,
    };
    let shortlist: std::collections::HashSet<String> = ["doc-a".to_string(), "doc-b".to_string()]
        .into_iter()
        .collect();

    let filtered = filter_results_by_document_shortlist(response.clone(), &shortlist);
    assert_eq!(filtered.results.len(), response.results.len());
    assert_eq!(filtered.total, response.total);
}
