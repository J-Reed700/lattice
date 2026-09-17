use crate::domain::qa::hyde::QueryType;
use crate::features::search::dto::SearchResponseDto;

use super::SearchFlags;

pub(super) fn should_execute_external_lookup(
    external_as_fallback: bool,
    kb_attempted: bool,
    kb_has_results: bool,
    kb_low_confidence: bool,
) -> bool {
    if !external_as_fallback {
        return true;
    }
    if !kb_attempted {
        return true;
    }
    if !kb_has_results {
        return true;
    }
    kb_low_confidence
}

pub(super) fn should_use_external_as_fallback(
    search_flags: SearchFlags,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
) -> bool {
    if search_flags.force_kb_search
        && !search_flags.force_web_search
        && !search_flags.force_wiki_search
    {
        return true;
    }

    // When both KB and external tools are enabled, follow-ups should prefer KB first.
    // This uses HyDE query type classification instead of keyword heuristics.
    search_flags.force_kb_search
        && (search_flags.force_web_search || search_flags.force_wiki_search)
        && (search_flags.force_followup_mode
            || matches!(interpretation.query_type, QueryType::Followup))
}

/// Whether KB retrieval may run side by side with the wiki/web phase, decided
/// from the flags before KB runs.
///
/// KB results can cancel the external phase only when external search is a
/// KB fallback. That gate reads the query type of KB's interpretation, and KB
/// retrieval never marks a turn as a follow-up on its own: only
/// `force_followup_mode` does, and the gate reads that flag directly. So a
/// plain question stands in for KB's interpretation here. The pipeline still
/// re-evaluates the gate with KB's real interpretation once both finish.
pub(super) fn kb_can_run_alongside_external(search_flags: SearchFlags) -> bool {
    let external_planned = search_flags.force_web_search || search_flags.force_wiki_search;
    let kb_interpretation =
        crate::domain::qa::hyde::HyDEInterpretation::raw_only(String::new(), QueryType::Question);
    external_planned && !should_use_external_as_fallback(search_flags, &kb_interpretation)
}

pub(super) fn empty_search_response() -> SearchResponseDto {
    SearchResponseDto {
        results: vec![],
        total: 0,
        query_time_ms: 0,
    }
}
