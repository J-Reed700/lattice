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
    // Web the intent step guessed at is a fallback by definition. The intent
    // step runs only when no toggle is set, so no wiki search shares the gate.
    if search_flags.web_fallback {
        return true;
    }
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
/// re-evaluates the gate with KB's real interpretation once both finish. A
/// web search the intent step guessed at is always a fallback, so it never
/// runs beside the vault.
pub(super) fn kb_can_run_alongside_external(search_flags: SearchFlags) -> bool {
    let external_planned = search_flags.web_planned() || search_flags.force_wiki_search;
    let kb_interpretation =
        crate::domain::qa::hyde::HyDEInterpretation::raw_only(String::new(), QueryType::Question);
    external_planned && !should_use_external_as_fallback(search_flags, &kb_interpretation)
}

/// Why the pipeline searched the web, as the turn record says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WebSearchReason {
    /// The user's web-search toggle or the query turn mode asked for it.
    Requested,
    /// The intent step wanted the web, and the vault found nothing.
    VaultEmpty,
    /// The intent step wanted the web, and the sufficiency check judged the
    /// vault's passages too weak to answer from.
    VaultInsufficient,
}

impl WebSearchReason {
    /// The web-search step's label. The query stays in the step's detail,
    /// where the research deck reads it.
    pub(super) fn step_label(self) -> &'static str {
        match self {
            Self::Requested => "Searching the web, as you asked",
            Self::VaultEmpty => "Searching the web, since your documents had nothing on this",
            Self::VaultInsufficient => "Searching the web, since your documents fell short",
        }
    }
}

/// Whether a web search the intent step guessed at may run, decided once the
/// vault has answered, and why. `None` keeps the question on the machine.
///
/// The vault must have been searched: a web query is never sent for a
/// question the user's own files were not asked first. Then the web runs only
/// when the vault found nothing or its evidence was judged insufficient. A
/// vault with passages and no verdict counts as sufficient, because only a
/// verdict can say it fell short.
pub(super) fn intent_web_fallback(
    kb_attempted: bool,
    kb_has_results: bool,
    kb_sufficient: Option<bool>,
) -> Option<WebSearchReason> {
    if !kb_attempted {
        return None;
    }
    if !kb_has_results {
        return Some(WebSearchReason::VaultEmpty);
    }
    (kb_sufficient == Some(false)).then_some(WebSearchReason::VaultInsufficient)
}

pub(super) fn empty_search_response() -> SearchResponseDto {
    SearchResponseDto {
        results: vec![],
        total: 0,
        query_time_ms: 0,
    }
}
