//! Validating a whole settings document into errors and warnings.
use super::SettingsRepository;
use crate::features::settings::dto::{
    LLMProvider, SettingsDto, ValidationResult, VectorIndexCompressionModeDto,
    MAX_LLM_STALL_TIMEOUT_SECONDS, MAX_VECTOR_COMPRESSION_DIMS, MIN_LLM_STALL_TIMEOUT_SECONDS,
    MIN_VECTOR_COMPRESSION_DIMS,
};
use url::Url;

impl SettingsRepository {
    /// Validate settings and return validation result.
    ///
    /// Uses the same validation logic as the mock implementation.
    pub(super) fn validate_settings(&self, settings: &SettingsDto) -> ValidationResult {
        let mut result = ValidationResult::success();

        if settings.indexing.chunk_size == 0 {
            result.add_error("indexing", "chunk_size must be greater than 0".to_string());
        }
        if settings.indexing.chunk_size > 10000 {
            result.add_warning("indexing", "chunk_size is very large".to_string());
        }
        if settings.indexing.chunk_overlap >= settings.indexing.chunk_size {
            result.add_error(
                "indexing",
                "chunk_overlap must be less than chunk_size".to_string(),
            );
        }
        if settings.indexing.batch_size == 0 {
            result.add_error("indexing", "batch_size must be greater than 0".to_string());
        }

        if settings.search.max_results == 0 {
            result.add_error("search", "max_results must be greater than 0".to_string());
        }
        if settings.search.similarity_threshold < 0.0 || settings.search.similarity_threshold > 1.0
        {
            result.add_error(
                "search",
                "similarity_threshold must be between 0.0 and 1.0".to_string(),
            );
        }
        if settings.search.hybrid_search_alpha < 0.0 || settings.search.hybrid_search_alpha > 1.0 {
            result.add_error(
                "search",
                "hybrid_search_alpha must be between 0.0 and 1.0".to_string(),
            );
        }
        // Only checked when truncation is on: the numbers are remembered while
        // the mode is `none`, and rejecting a remembered value the index never
        // reads would make the whole settings document unsaveable.
        if settings.search.vector_index_compression.mode == VectorIndexCompressionModeDto::Truncated
        {
            let dims = settings.search.vector_index_compression.dims;
            if !(MIN_VECTOR_COMPRESSION_DIMS..=MAX_VECTOR_COMPRESSION_DIMS).contains(&dims) {
                result.add_error(
                    "search",
                    format!(
                        "vector_index_compression.dims must be between {} and {}",
                        MIN_VECTOR_COMPRESSION_DIMS, MAX_VECTOR_COMPRESSION_DIMS
                    ),
                );
            }
        }
        let tuning = &settings.search.retrieval_tuning;
        if tuning.kb_search_min_limit == 0
            || tuning.kb_search_min_limit > tuning.kb_search_max_limit
        {
            result.add_error(
                "search",
                "kb_search_min_limit must be > 0 and <= kb_search_max_limit".to_string(),
            );
        }
        if tuning.doc_shortlist_candidate_min == 0
            || tuning.doc_shortlist_candidate_min > tuning.doc_shortlist_candidate_max
        {
            result.add_error(
                "search",
                "doc_shortlist_candidate_min must be > 0 and <= doc_shortlist_candidate_max"
                    .to_string(),
            );
        }
        if tuning.doc_shortlist_doc_min == 0
            || tuning.doc_shortlist_doc_min > tuning.doc_shortlist_doc_max
        {
            result.add_error(
                "search",
                "doc_shortlist_doc_min must be > 0 and <= doc_shortlist_doc_max".to_string(),
            );
        }
        if tuning.shortlist_gate_min_candidates == 0 || tuning.shortlist_gate_min_docs == 0 {
            result.add_error(
                "search",
                "shortlist gate minimums must be greater than 0".to_string(),
            );
        }
        if tuning.wiki_search_max_results == 0 || tuning.web_search_max_results == 0 {
            result.add_error(
                "search",
                "wiki_search_max_results and web_search_max_results must be greater than 0"
                    .to_string(),
            );
        }
        if tuning.web_fetch_page_count > 10 {
            result.add_error(
                "search",
                "web_fetch_page_count must be 10 or fewer".to_string(),
            );
        }
        if tuning.web_fetch_page_count > 0
            && (tuning.web_page_max_chars == 0 || tuning.web_page_fetch_timeout_secs == 0)
        {
            result.add_error(
                "search",
                "web_page_max_chars and web_page_fetch_timeout_secs must be greater than 0 when page fetching is enabled"
                    .to_string(),
            );
        }
        if tuning.deep_research_depth == 0 || tuning.deep_research_depth > 4 {
            result.add_error(
                "search",
                "deep_research_depth must be between 1 and 4".to_string(),
            );
        }
        if tuning.deep_research_branch_queries == 0 || tuning.deep_research_branch_queries > 4 {
            result.add_error(
                "search",
                "deep_research_branch_queries must be between 1 and 4".to_string(),
            );
        }
        if tuning.wiki_snippet_max_chars == 0
            || tuning.web_snippet_max_chars == 0
            || tuning.external_search_query_max_chars == 0
            || tuning.rerank_query_max_chars == 0
        {
            result.add_error(
                "search",
                "snippet/query character limits must be greater than 0".to_string(),
            );
        }
        if tuning.external_search_max_wiki_terms == 0 || tuning.external_search_max_web_terms == 0 {
            result.add_error(
                "search",
                "external_search_max_wiki_terms and external_search_max_web_terms must be > 0"
                    .to_string(),
            );
        }
        if tuning.rerank_max_candidates == 0 {
            result.add_error(
                "search",
                "rerank_max_candidates must be greater than 0".to_string(),
            );
        }
        if tuning.overlap_min_hits_for_multi_term == 0 {
            result.add_error(
                "search",
                "overlap_min_hits_for_multi_term must be greater than 0".to_string(),
            );
        }
        if !(0.0..=1.0).contains(&tuning.sufficiency_min_top_score)
            || !(0.0..=1.0).contains(&tuning.sufficiency_min_term_coverage)
        {
            result.add_error(
                "search",
                "sufficiency thresholds must be between 0.0 and 1.0".to_string(),
            );
        }
        if tuning.doc_support_multi_hit_ratio_factor <= 0.0
            || tuning.doc_support_single_hit_ratio_factor <= 0.0
        {
            result.add_error(
                "search",
                "doc support ratio factors must be greater than 0.0".to_string(),
            );
        }
        if !(0.0..=1.0).contains(&tuning.doc_support_multi_hit_ratio_min)
            || !(0.0..=1.0).contains(&tuning.doc_support_multi_hit_ratio_max)
            || tuning.doc_support_multi_hit_ratio_min > tuning.doc_support_multi_hit_ratio_max
        {
            result.add_error(
                "search",
                "doc_support_multi_hit_ratio bounds must be between 0.0 and 1.0 and ordered"
                    .to_string(),
            );
        }
        if !(0.0..=1.0).contains(&tuning.doc_support_single_hit_ratio_min)
            || !(0.0..=1.0).contains(&tuning.doc_support_single_hit_ratio_max)
            || tuning.doc_support_single_hit_ratio_min > tuning.doc_support_single_hit_ratio_max
        {
            result.add_error(
                "search",
                "doc_support_single_hit_ratio bounds must be between 0.0 and 1.0 and ordered"
                    .to_string(),
            );
        }

        if settings.llm.temperature < 0.0 || settings.llm.temperature > 2.0 {
            result.add_error("llm", "temperature must be between 0.0 and 2.0".to_string());
        }
        if settings.llm.top_p < 0.0 || settings.llm.top_p > 1.0 {
            result.add_error("llm", "top_p must be between 0.0 and 1.0".to_string());
        }
        if settings.llm.top_k < 0 {
            result.add_error(
                "llm",
                "top_k must be greater than or equal to 0".to_string(),
            );
        }
        if settings.llm.repeat_penalty <= 0.0 {
            result.add_error("llm", "repeat_penalty must be greater than 0.0".to_string());
        }
        if settings.llm.max_tokens == 0 {
            result.add_error("llm", "max_tokens must be greater than 0".to_string());
        }
        if settings.llm.context_window == 0 {
            result.add_error("llm", "context_window must be greater than 0".to_string());
        }
        if settings.llm.provider == LLMProvider::Ollama {
            if settings.llm.ollama_url.is_empty() {
                result.add_error("llm", "ollama_url cannot be empty".to_string());
            }
            if settings.llm.model.is_empty() {
                result.add_error("llm", "model cannot be empty".to_string());
            }
        }
        if let Err(error) =
            crate::features::llm::llama_cpp::validate_connection(&settings.llm.llama_cpp)
        {
            result.add_error("llm", error.to_string());
        }
        let auth_name = settings.llm.ollama_auth_header_name.trim();
        let auth_value = settings.llm.ollama_auth_header_value.trim();
        if (!auth_name.is_empty() && auth_value.is_empty())
            || (auth_name.is_empty() && !auth_value.is_empty())
        {
            result.add_error(
                "llm",
                "ollama_auth_header_name and ollama_auth_header_value must both be set".to_string(),
            );
        }
        if !(MIN_LLM_STALL_TIMEOUT_SECONDS..=MAX_LLM_STALL_TIMEOUT_SECONDS)
            .contains(&settings.llm.timeout_seconds)
        {
            result.add_error(
                "llm",
                format!(
                    "timeout_seconds is the stall timeout — the silence a response may go without \
                     being declared dead — and must be between {} and {} seconds",
                    MIN_LLM_STALL_TIMEOUT_SECONDS, MAX_LLM_STALL_TIMEOUT_SECONDS
                ),
            );
        }
        for path in &settings.llm.external_model_directories {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                result.add_error(
                    "llm",
                    "external_model_directories cannot contain empty paths".to_string(),
                );
                continue;
            }
            if trimmed.contains('\0') {
                result.add_error(
                    "llm",
                    "external_model_directories cannot contain null bytes".to_string(),
                );
                continue;
            }
            if !std::path::Path::new(trimmed).is_absolute() {
                result.add_warning(
                    "llm",
                    format!(
                        "external_model_directories path '{}' is not absolute and may be ignored",
                        trimmed
                    ),
                );
            }
        }
        if settings.llm.prompts.rag_prompt_template.trim().is_empty() {
            result.add_error("llm", "rag_prompt_template cannot be empty".to_string());
        }
        if settings
            .llm
            .prompts
            .greeting_prompt_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "greeting_prompt_template cannot be empty".to_string(),
            );
        }
        if settings
            .llm
            .prompts
            .no_context_prompt_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "no_context_prompt_template cannot be empty".to_string(),
            );
        }
        if settings.llm.tool_output.max_chars == 0 {
            result.add_error(
                "llm",
                "tool_output.max_chars must be greater than 0".to_string(),
            );
        }
        if settings.llm.tool_output.excerpt_chars == 0 {
            result.add_error(
                "llm",
                "tool_output.excerpt_chars must be greater than 0".to_string(),
            );
        }
        if settings.llm.tool_output.max_results == 0 {
            result.add_error(
                "llm",
                "tool_output.max_results must be greater than 0".to_string(),
            );
        }
        if settings.llm.tool_output.highlight_terms_max == 0 {
            result.add_warning(
                "llm",
                "tool_output.highlight_terms_max is 0; excerpts will be less relevant".to_string(),
            );
        }
        if !settings.llm.router.enabled {
            result.add_warning(
                "llm",
                "router.enabled is false; follow-up routing will use deterministic fallback behavior"
                    .to_string(),
            );
        }
        if settings.llm.router.enabled && settings.llm.router.model.trim().is_empty() {
            result.add_error(
                "llm",
                "router.model cannot be empty; configure a small router model (e.g. Phi-3.5 Mini or TinyLlama)"
                    .to_string(),
            );
        }
        if settings
            .llm
            .tool_output
            .templates
            .default_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "tool_output.templates.default_template cannot be empty".to_string(),
            );
        }
        if settings
            .llm
            .tool_output
            .templates
            .get_document_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "tool_output.templates.get_document_template cannot be empty".to_string(),
            );
        }
        if settings
            .llm
            .tool_output
            .templates
            .semantic_search_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "tool_output.templates.semantic_search_template cannot be empty".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .rag_prompt_template
            .contains("{context}")
        {
            result.add_warning(
                "llm",
                "rag_prompt_template missing {context} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .rag_prompt_template
            .contains("{question}")
        {
            result.add_warning(
                "llm",
                "rag_prompt_template missing {question} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .greeting_prompt_template
            .contains("{question}")
        {
            result.add_warning(
                "llm",
                "greeting_prompt_template missing {question} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .no_context_prompt_template
            .contains("{question}")
        {
            result.add_warning(
                "llm",
                "no_context_prompt_template missing {question} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .tool_output
            .templates
            .default_template
            .contains("{json}")
        {
            result.add_warning(
                "llm",
                "tool_output.templates.default_template missing {json} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .tool_output
            .templates
            .get_document_template
            .contains("{excerpt}")
        {
            result.add_warning(
                "llm",
                "tool_output.templates.get_document_template missing {excerpt} placeholder"
                    .to_string(),
            );
        }
        if !settings
            .llm
            .tool_output
            .templates
            .semantic_search_template
            .contains("{results}")
        {
            result.add_warning(
                "llm",
                "tool_output.templates.semantic_search_template missing {results} placeholder"
                    .to_string(),
            );
        }
        let mut custom_tool_names = std::collections::HashSet::new();
        for tool in &settings.llm.custom_tools {
            if !tool.enabled {
                continue;
            }

            let tool_name = tool.name.trim();
            if tool_name.is_empty() {
                result.add_error("llm", "custom_tools.name cannot be empty".to_string());
            } else {
                let valid_name = tool_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid_name {
                    result.add_error(
                        "llm",
                        format!(
                            "custom_tools.name '{}' must use only alphanumeric characters and underscores",
                            tool_name
                        ),
                    );
                }
                if !custom_tool_names.insert(tool_name.to_string()) {
                    result.add_error("llm", format!("Duplicate custom tool name '{}'", tool_name));
                }
            }

            if tool.description.trim().is_empty() {
                result.add_error(
                    "llm",
                    format!("custom tool '{}' description cannot be empty", tool_name),
                );
            }

            if tool.query_param.trim().is_empty() {
                result.add_error(
                    "llm",
                    format!(
                        "custom tool '{}' query_param cannot be empty",
                        if tool_name.is_empty() {
                            "<unnamed>"
                        } else {
                            tool_name
                        }
                    ),
                );
            }

            if tool.default_max_results == 0 {
                result.add_error(
                    "llm",
                    format!(
                        "custom tool '{}' default_max_results must be greater than 0",
                        if tool_name.is_empty() {
                            "<unnamed>"
                        } else {
                            tool_name
                        }
                    ),
                );
            }

            match Url::parse(tool.endpoint.trim()) {
                Ok(parsed) => {
                    if !matches!(parsed.scheme(), "https" | "http") {
                        result.add_error(
                            "llm",
                            format!(
                                "custom tool '{}' endpoint must use http or https",
                                if tool_name.is_empty() {
                                    "<unnamed>"
                                } else {
                                    tool_name
                                }
                            ),
                        );
                    }
                }
                Err(_) => {
                    result.add_error(
                        "llm",
                        format!(
                            "custom tool '{}' endpoint must be a valid URL",
                            if tool_name.is_empty() {
                                "<unnamed>"
                            } else {
                                tool_name
                            }
                        ),
                    );
                }
            }
        }

        if settings.ui.font_size < 8 || settings.ui.font_size > 72 {
            result.add_error("ui", "font_size must be between 8 and 72".to_string());
        }
        if settings.ui.results_per_page == 0 {
            result.add_error("ui", "results_per_page must be greater than 0".to_string());
        }
        if !["light", "dark", "system"].contains(&settings.ui.theme.as_str()) {
            result.add_warning(
                "ui",
                format!("Unknown theme '{}', using 'system'", settings.ui.theme),
            );
        }

        if settings.sync.sync_enabled && settings.sync.sync_url.is_empty() {
            result.add_error(
                "sync",
                "sync_url is required when sync is enabled".to_string(),
            );
        }
        if settings.sync.sync_interval_minutes == 0 {
            result.add_error(
                "sync",
                "sync_interval_minutes must be greater than 0".to_string(),
            );
        }

        // Validate backup settings.
        // `backup_path` is intentionally empty: scheduled backups always use
        // the application-owned backups directory. The field is retained for
        // settings-file compatibility, but accepting an arbitrary value here
        // would only defer a guaranteed adapter rejection until the scheduler
        // runs. Reject it while saving so the failure is immediate and visible.
        if !settings.backup.backup_path.is_empty() {
            result.add_error(
                "backup",
                "custom backup locations are not supported; leave backup_path empty to use the application's protected backups directory".to_string(),
            );
        }
        if settings.backup.backup_retention_days == 0 {
            result.add_warning(
                "backup",
                "backup_retention_days is 0, backups won't be retained".to_string(),
            );
        }
        if !["daily", "weekly", "monthly"].contains(&settings.backup.backup_frequency.as_str()) {
            result.add_warning(
                "backup",
                format!(
                    "Unknown backup frequency '{}', using 'daily'",
                    settings.backup.backup_frequency
                ),
            );
        }

        result
    }
}
