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

pub(super) fn empty_search_response() -> SearchResponseDto {
    SearchResponseDto {
        results: vec![],
        total: 0,
        query_time_ms: 0,
    }
}
