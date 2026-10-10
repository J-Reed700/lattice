use super::*;
use std::collections::HashSet;

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
        router_may_search: true,
    };
    let flags = SearchFlags {
        force_kb_search: false,
        force_web_search: false,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
        closed_book: false,
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
        closed_book: false,
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
        closed_book: false,
    };
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::raw_only(
        "latest updates?",
        QueryType::Question,
    );

    let fallback = should_use_external_as_fallback(flags, &interpretation);
    assert!(!fallback);
}

fn flags(kb: bool, web: bool, wiki: bool, followup: bool) -> SearchFlags {
    SearchFlags {
        force_kb_search: kb,
        force_web_search: web,
        force_wiki_search: wiki,
        deep_research_mode: false,
        force_followup_mode: followup,
        closed_book: false,
    }
}

#[test]
fn kb_runs_alongside_external_search_when_kb_cannot_cancel_it() {
    // Query turn mode forces KB and web together.
    let query_turn = flags(true, true, false, false);
    let kb_and_wiki = flags(true, false, true, false);
    // KB from the router, not forced: external search is never a fallback.
    let routed_kb_followup_web = flags(false, true, false, true);
    let routed_kb_wiki = flags(false, false, true, false);

    assert!(kb_can_run_alongside_external(query_turn));
    assert!(kb_can_run_alongside_external(kb_and_wiki));
    assert!(kb_can_run_alongside_external(routed_kb_followup_web));
    assert!(kb_can_run_alongside_external(routed_kb_wiki));
}

#[test]
fn kb_runs_first_when_external_search_is_its_fallback_or_absent() {
    // Forced follow-ups prefer KB first, so KB decides whether to search out.
    let followup_kb_web = flags(true, true, false, true);
    let followup_kb_web_wiki = flags(true, true, true, true);
    // Nothing external to run beside KB.
    let kb_only = flags(true, false, false, false);
    let nothing_forced = flags(false, false, false, false);

    assert!(!kb_can_run_alongside_external(followup_kb_web));
    assert!(!kb_can_run_alongside_external(followup_kb_web_wiki));
    assert!(!kb_can_run_alongside_external(kb_only));
    assert!(!kb_can_run_alongside_external(nothing_forced));
}

#[test]
fn concurrent_decision_matches_the_gate_kb_retrieval_will_produce() {
    // KB retrieval classifies a turn as a follow-up only in follow-up mode, so
    // whenever the pipeline runs KB concurrently, the post-KB gate stays open.
    for bits in 0u8..16 {
        let flags = flags(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
        let kb_query_type = if flags.force_followup_mode {
            QueryType::Followup
        } else {
            QueryType::Question
        };
        let kb_interpretation =
            crate::domain::qa::hyde::HyDEInterpretation::raw_only("q", kb_query_type);
        if kb_can_run_alongside_external(flags) {
            assert!(
                !should_use_external_as_fallback(flags, &kb_interpretation),
                "{flags:?}"
            );
        }
    }
}

fn empty_pipeline_outcome() -> RetrievalPipelineOutcome {
    RetrievalPipelineOutcome {
        short_circuit_response: None,
        interpretation: crate::domain::qa::hyde::HyDEInterpretation::raw_only(
            "q",
            QueryType::Question,
        ),
        search_response: empty_search_response(),
        followup_context: None,
        web_context: Vec::new(),
        web_search_error: None,
        kb_unavailable_reason: None,
        kb_attempted: false,
        web_queries: Vec::new(),
        sources: Vec::new(),
        evidence: Default::default(),
        sub_timings: RetrievalSubTimingMetrics::default(),
        searched_documents: 0,
        scope_is_linked: false,
        sufficiency: None,
        pages_read: Default::default(),
    }
}

fn external_result(url: &str, context: Option<&str>) -> pipeline::ExternalSearchResult {
    let citation = WebSearchResult {
        title: url.to_string(),
        url: url.to_string(),
        snippet: "snippet".to_string(),
        published_date: None,
        source: None,
    };
    pipeline::ExternalSearchResult {
        sources: build_web_source_citations(&[citation], &[], 200),
        context: context
            .map(|detail| {
                vec![WebContextItem {
                    url: url.to_string(),
                    title: url.to_string(),
                    detail: detail.to_string(),
                }]
            })
            .unwrap_or_default(),
        error: None,
        elapsed_ms: 7,
        pages: Default::default(),
        queries: Vec::new(),
    }
}

/// The turn in the log: retrieval read three pages, one of them blocked, and
/// the tool loop then started blank and asked for all three again. Whatever
/// the web phase learned has to reach the outcome the tool loop is handed.
#[test]
fn what_the_web_phase_read_reaches_the_outcome_the_tool_loop_starts_from() {
    use crate::features::conversation::chat::fetch_memory::{Delivery, Recall};

    let mut web = external_result("https://example.com/web", Some("web context"));
    web.pages
        .record_failure("https://blocked.test/guide", "HTTP 403 Forbidden");
    web.pages.record_page(
        "https://read.test/recap",
        "the whole recap",
        Delivery::Whole,
    );

    let mut outcome = empty_pipeline_outcome();
    pipeline::attach_web_results(&mut outcome, web);

    assert_eq!(
        outcome
            .pages_read
            .previous_failure("https://blocked.test/guide"),
        Some("HTTP 403 Forbidden")
    );
    assert_eq!(
        outcome.pages_read.recall("https://read.test/recap"),
        Some(Recall::AlreadyWhole { word_count: 3 })
    );
}

/// A deep-research turn lists these for the model, so it does not spend a
/// round repeating a search the first round already ran.
#[test]
fn the_searches_the_web_phase_ran_reach_the_outcome() {
    let mut web = external_result("https://example.com/web", Some("web context"));
    web.queries = vec![
        "China extreme weather 2026".to_string(),
        "China typhoon track October 2026".to_string(),
    ];
    let mut outcome = empty_pipeline_outcome();
    pipeline::attach_web_results(&mut outcome, web);
    assert_eq!(
        outcome.web_queries,
        [
            "China extreme weather 2026",
            "China typhoon track October 2026"
        ]
    );
}

#[test]
fn external_results_merge_wiki_before_web() {
    let mut outcome = empty_pipeline_outcome();
    let mut web = external_result("https://example.com/web", Some("web context"));
    web.error = Some("partial failure".to_string());

    pipeline::attach_wiki_results(
        &mut outcome,
        external_result("https://en.wikipedia.org/wiki/X", Some("wiki context")),
    );
    pipeline::attach_web_results(&mut outcome, web);

    let details: Vec<_> = outcome
        .web_context
        .iter()
        .map(|item| item.detail.as_str())
        .collect();
    assert_eq!(details, ["wiki context", "web context"]);
    let urls: Vec<_> = outcome
        .sources
        .iter()
        .map(|s| s.file_path.as_str())
        .collect();
    assert_eq!(urls.len(), 2, "{urls:?}");
    assert!(urls[0].contains("wikipedia"), "{urls:?}");
    assert_eq!(outcome.web_search_error.as_deref(), Some("partial failure"));
    assert_eq!(outcome.sub_timings.wiki_search_ms, 7);
    assert_eq!(outcome.sub_timings.web_search_ms, 7);
}

#[test]
fn empty_wiki_search_leaves_web_context_alone() {
    let mut outcome = empty_pipeline_outcome();

    pipeline::attach_wiki_results(&mut outcome, pipeline::ExternalSearchResult::default());
    assert!(outcome.web_context.is_empty());

    pipeline::attach_web_results(
        &mut outcome,
        external_result("https://example.com/web", Some("web context")),
    );
    assert_eq!(outcome.web_context.len(), 1);
    assert_eq!(outcome.web_context[0].detail, "web context");
    assert!(outcome.web_search_error.is_none());
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
fn web_query_condenses_a_rambling_message_instead_of_searching_it_verbatim() {
    let raw = "Please go into depth on what happens in the second season of silo on \
               apple tv. I watched it but need a deep recap. Season 1 and 2, really in \
               depth recap. I've watched both but I just need a very deep deep deep in \
               depth refresher";
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::for_question(
        raw,
        "Episode-by-episode recaps of Silo seasons 1 and 2 on Apple TV+, covering plot, \
         character arcs and the finale twists.",
    );

    let query = select_web_search_query(raw, &interpretation, None);

    // A search engine weighs every word it is handed, so the paragraph must not
    // reach it intact — the ranked terms carry the subject without the filler.
    assert_ne!(query, raw);
    assert!(
        query.len() < raw.len() / 2,
        "query was not condensed: {query}"
    );
    assert!(query.contains("silo"), "subject lost: {query}");
    assert!(!query.contains("really"), "filler survived: {query}");
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

fn skip_catalog() -> Vec<crate::application::contracts::search::CorpusDocument> {
    vec![crate::application::contracts::search::CorpusDocument {
        sections: Vec::new(),
        source_context: None,
        id: "handbook".into(),
        name: "Employment handbook".into(),
        opening: "Introduction".into(),
        chapter_number: None,
    }]
}

fn anchors(terms: &[&str]) -> HashSet<String> {
    terms.iter().map(|t| (*t).to_string()).collect()
}

#[test]
fn a_short_message_on_the_previous_turn_topic_skips_the_planner() {
    let previous = anchors(&["noncompete", "clauses", "severance"]);

    let shared = corpus_plan::planner_skip_anchors(
        "and the noncompete clauses after termination?",
        &previous,
    )
    .expect("two shared anchors in a short message is a continuation");

    assert_eq!(shared, ["clauses", "noncompete"]);
}

#[test]
fn one_shared_word_is_a_coincidence_not_a_continuation() {
    let previous = anchors(&["noncompete", "severance"]);

    assert!(
        corpus_plan::planner_skip_anchors("what about noncompete?", &previous).is_none(),
        "a single shared term must still reach the planner"
    );
    assert!(corpus_plan::planner_skip_anchors("what about holidays?", &previous).is_none());
    assert!(corpus_plan::planner_skip_anchors("what about holidays?", &HashSet::new()).is_none());
}

#[test]
fn a_long_message_reaches_the_planner_even_on_the_same_topic() {
    let previous = anchors(&["noncompete", "clauses"]);
    let long = "compare the noncompete clauses against the severance provisions, \
        the intellectual property assignment, the arbitration requirement, the \
        relocation allowance, and the equity vesting acceleration schedule";

    assert!(!corpus_plan::is_short_followup_message(long));
    assert!(corpus_plan::planner_skip_anchors(long, &previous).is_none());
}

#[test]
fn a_skipped_planner_carries_the_previous_topic_and_the_new_message() {
    let previous = anchors(&["noncompete", "clauses"]);

    let (plan, shared) = corpus_plan::followup_plan(
        "and the exceptions to those noncompete clauses?",
        &previous,
        &skip_catalog(),
    )
    .expect("short on-topic follow-up plans without the planner");

    assert_eq!(shared, ["clauses", "noncompete"]);
    assert_eq!(
        plan.queries[0],
        "and the exceptions to those noncompete clauses?"
    );
    let carried = &plan.queries[1];
    assert!(carried.contains("noncompete"), "{carried}");
    assert!(carried.contains("exceptions"), "{carried}");
    // A reused plan is a focused search, never an ordered read.
    assert!(plan.opening_document_ids.is_empty());
    assert!(!plan.start_at_beginning);
}

fn sufficiency_result(score: f32, content: &str) -> SearchResultDto {
    SearchResultDto {
        id: "chunk".to_string(),
        title: "Guide".to_string(),
        content: content.to_string(),
        score,
        path: None,
        document_id: Some("doc".to_string()),
        position: None,
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: HashMap::new(),
    }
}

#[test]
fn public_sufficiency_facade_reports_the_pipelines_own_verdict() {
    let tuning = RetrievalTuningSettingsDto::default();
    let queries = vec!["workspace snapshot retention window".to_string()];

    // Passages that never mention what was asked about: the coverage signal is
    // the one that catches this, and it must reach the caller as a reason code.
    let missed = assess_retrieval_sufficiency(
        &[sufficiency_result(
            0.9,
            "The cafeteria menu rotates weekly.",
        )],
        &queries,
        &tuning,
        true,
    );
    assert!(!missed.sufficient);
    assert!(missed.reasons.contains(&"low_term_coverage"), "{missed:?}");
    assert_eq!(missed.top_score, 0.9);
    assert_eq!(missed.term_coverage, 0.0);

    // An empty ranking is the one failure retrieval always noticed.
    let empty = assess_retrieval_sufficiency(&[], &queries, &tuning, true);
    assert!(!empty.sufficient);
    assert_eq!(empty.reasons, ["no_results"]);
}

/// A chat moved out of a space, or a document filed elsewhere since, leaves a
/// reference behind. The follow-up path reopens the last reference by id, so a
/// stale one put a document the space no longer holds straight into the prompt.
#[test]
fn a_remembered_document_outside_the_space_is_forgotten() {
    let reference = |id: &str| crate::domain::conversation::DocumentReference {
        document_id: id.to_string(),
        chunk_id: None,
        relevance_score: None,
        added_at: chrono::Utc::now(),
    };
    let allowed = HashSet::from(["still_here".to_string()]);

    let kept = super::keep_references_in_scope(
        vec![reference("still_here"), reference("filed_elsewhere")],
        &allowed,
    );

    assert_eq!(
        kept.iter()
            .map(|r| r.document_id.as_str())
            .collect::<Vec<_>>(),
        ["still_here"]
    );
    assert!(super::keep_references_in_scope(vec![reference("any")], &HashSet::new()).is_empty());
}

#[test]
fn web_query_timeout_keeps_grow_light_subject_and_discards_tracking_ids() {
    let raw = r#"Also, too, what types of grow lights, what I need. So, because, for example, I have it to war for blueberry plant, it's in a one-gallon bucket, and I have it just facing a window, and it seems to be getting enough light, because there's a ton of growth on it, and it's growing really well. I have Sansi growlites, which are very good growlites. Like I buy a giant one, I guess it's only like 30 watts, but it's super fucking bright, and it gives off a lot of light. I feel like pointing just one of those light bulbs right at the plant would be enough, because it's just a lot of these produce a ton of light. I don't know, maybe look it up and see if it's enough.

Grow light: https://www.sansiled.com/products/br30-36w-led-grow-light-bulb?currency=USD&country=US&variant=43716789043426&utm_source=google&utm_medium=cpc&utm_campaign=Google%20Shopping&stkn=e3067c49e087&gad_source=1&gad_campaignid=23836680380&gbraid=0AAAAAoPQO5oRdg3iBkBYv0p1o7_GLgAIW&gclid=Cj0KCQjw8c3VBhCsARIsAA_xJ91otG_q8tN8C19x05OVlvViMxInRhI3QZk187rS3Tk1DtDU4XJBxj8aAoPqEALw_wcB"#;
    let interpretation =
        crate::domain::qa::hyde::HyDEInterpretation::raw_only(raw, QueryType::Followup);
    let anchors = ["research", "smaller", "sunlight"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let query = select_web_search_query(raw, &interpretation, Some(&anchors));
    assert!(query.contains("grow"), "{query}");
    assert!(query.contains("light"), "{query}");
    assert!(query.contains("blueberry"), "{query}");
    assert!(!query.contains("campaign"), "{query}");
    assert!(!query.contains("0aaaa"), "{query}");
    assert!(!query.contains("e3067"), "{query}");
    assert!(
        !query.contains("research"),
        "stale anchor displaced subject: {query}"
    );
}

#[test]
fn web_query_strips_tracking_even_for_a_short_product_link() {
    let raw = "https://example.com/products/36w-grow-light?gclid=opaqueidentifier&utm_campaign=unrelated#tracking";
    let interpretation =
        crate::domain::qa::hyde::HyDEInterpretation::raw_only(raw, QueryType::Question);
    let query = select_web_search_query(raw, &interpretation, None);
    assert!(query.contains("grow light"), "{query}");
    assert!(!query.contains("opaqueidentifier"), "{query}");
    assert!(!query.contains("unrelated"), "{query}");
}

/// The intent classifier wanting the web used to switch the vault off: its
/// `needs_web` became `force_web_search`, and the plan read that as "web
/// instead of the library". Web now adds to the vault, and the two run side by
/// side so both sets of sources reach the answer.
#[test]
fn an_intent_that_needs_the_web_still_searches_the_vault() {
    let intent = crate::infrastructure::services::intent::TurnIntent {
        needs_knowledge_base: false,
        needs_web: true,
        is_followup: false,
        confidence: 0.9,
    };
    let merged = super::super::apply_turn_intent(SearchFlags::from_preferences(None), &intent);
    assert!(merged.force_web_search);
    assert!(!merged.force_kb_search);

    let decision = RouterDecisionOutcome {
        action: RouterAction::NewSearch,
        recent_doc_meta: None,
        clarify_message: None,
        router_may_search: true,
    };
    let plan =
        RetrievalPlan::from_router(&RouterSettingsDto::default(), &decision, "latest", merged);

    assert!(plan.should_search_kb);
    assert!(plan.should_search_web);
    assert!(kb_can_run_alongside_external(merged));
}

/// An Explorer turn is about the folder on screen. The router's default "new
/// search" no longer reaches into the library by itself; the knowledge-base
/// toggle does, and so does the web toggle, because web adds to the vault and
/// never replaces it.
#[test]
fn an_explorer_turn_searches_the_library_only_when_a_toggle_asks() {
    let explorer_decision = RouterDecisionOutcome {
        action: RouterAction::NewSearch,
        recent_doc_meta: None,
        clarify_message: None,
        router_may_search: false,
    };
    let settings = RouterSettingsDto::default();
    let plan = |flags| {
        RetrievalPlan::from_router(&settings, &explorer_decision, "what does main do", flags)
    };

    let untoggled = plan(flags(false, false, false, false));
    assert!(!untoggled.should_search_kb);
    assert!(!untoggled.should_search_web);

    assert!(plan(flags(true, false, false, false)).should_search_kb);
    let web = plan(flags(false, true, false, false));
    assert!(web.should_search_kb);
    assert!(web.should_search_web);
}
