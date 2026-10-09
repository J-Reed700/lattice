//! Fetch immutable repair evidence through the existing DNS-safe web service.
use crate::features::learning::{
    dto::*,
    outline_draft::*,
    outline_progress::{OutlineProgress, OutlineStage},
    repository::LearningRepository,
    source_library::CapturedLearningSource,
    sources::{preview, InitialReference},
};
use crate::{
    features::{function_calling::dto::WebSearchInput, web::traits::WebServiceTrait},
    shared::error::Result,
};
use std::collections::HashSet;

/// One search per stable module position (plus the course goal) in an operation.
/// Changing wording during repair cannot reset this and create an endless search.
#[derive(Default)]
pub(in crate::features::learning) struct ResearchAttempts {
    scopes: HashSet<Option<usize>>,
    queries: HashSet<String>,
    urls: HashSet<String>,
}

pub(in crate::features::learning) async fn research(
    repo: &LearningRepository,
    web: &dyn WebServiceTrait,
    program: &mut LearningProgramDto,
    draft: &mut OutlineDraft,
    sources: &mut Vec<LearningSourceDto>,
    progress: &OutlineProgress,
    attempts: &mut ResearchAttempts,
) -> Result<usize> {
    let mut scopes: Vec<_> = draft
        .review
        .issues
        .iter()
        .map(|issue| {
            issue
                .path
                .strip_prefix("/modules/")
                .and_then(|s| s.split('/').next())
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|i| {
                    draft
                        .candidate
                        .get("modules")
                        .and_then(|m| m.get(*i))
                        .is_some()
                })
        })
        .collect();
    if sources.is_empty() {
        scopes.insert(0, None);
    }
    scopes.retain(|scope| attempts.scopes.insert(*scope));
    if scopes.is_empty() {
        return Ok(0);
    }
    progress.stage(OutlineStage::Researching);
    attempts
        .urls
        .extend(sources.iter().filter_map(|s| s.url.clone()));
    let mut added = 0;
    let mut unavailable = 0;
    // One query per affected module. Search titles and the learning goal,
    // rather than transmitting whole documents or their private quotations.
    for scope in scopes {
        let module = scope.and_then(|i| draft.candidate.get("modules")?.get(i));
        let title = module
            .and_then(|m| m.get("title"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let query = format!(
            "{} {} authoritative reference",
            draft.request.goal.chars().take(250).collect::<String>(),
            title.chars().take(200).collect::<String>()
        );
        if !attempts.queries.insert(query.clone()) {
            continue;
        }
        progress.check_cancelled()?;
        let input: WebSearchInput =
            serde_json::from_value(serde_json::json!({"query":query,"max_results":4,"depth":1}))?;
        let results = match web.search_web(&input).await {
            Ok(r) => r,
            Err(error) => {
                tracing::warn!(%error,"Outline repair search unavailable");
                unavailable += 1;
                continue;
            }
        };
        // Search snippets never become evidence: fetch full pages and retain
        // only complete captures. Limit pages per query, not workflow duration.
        for result in results
            .results
            .into_iter()
            .filter(|r| !attempts.urls.contains(&r.url))
            .take(2)
            .collect::<Vec<_>>()
        {
            progress.check_cancelled()?;
            if !attempts.urls.insert(result.url.clone()) {
                continue;
            }
            let article = match web.fetch_reference_content(&result.url).await {
                Ok(a) => a,
                Err(error) => {
                    tracing::warn!(%error,"Outline repair reference unavailable");
                    unavailable += 1;
                    continue;
                }
            };
            let captured = CapturedLearningSource {
                title: article.title.unwrap_or(result.title),
                publisher: None,
                requested_url: Some(result.url.clone()),
                resolved_url: Some(article.url),
                text: article.content,
                truncated: article.content_truncated,
                extraction_version:
                    crate::features::web::services::REFERENCE_TEXT_EXTRACTION_VERSION.into(),
            };
            if crate::features::learning::sources::validate_capture(&captured).is_err() {
                unavailable += 1;
                continue;
            }
            // Redirects can lead multiple results to the same saved page.
            if captured
                .resolved_url
                .as_ref()
                .is_some_and(|url| sources.iter().any(|s| s.url.as_ref() == Some(url)))
            {
                continue;
            }
            if let Some(url) = &captured.resolved_url {
                attempts.urls.insert(url.clone());
            }
            if sources
                .iter()
                .any(|s| s.excerpt.trim() == captured.text.trim())
            {
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            let source = preview(&id, &captured);
            let mut full = source.clone();
            full.excerpt = captured.text.clone();
            draft.source_ids.push(id.clone());
            draft.review.status = LearningOutlineReviewStatus::Unchecked;
            draft.review.note="Additional reference saved. Its relevance and the revised claims still need review.".into();
            repo.checkpoint_outline(
                program,
                draft,
                std::slice::from_ref(&source),
                &[InitialReference {
                    source_id: id,
                    origin: result.url,
                    captured,
                }],
            )
            .await?;
            sources.push(full);
            added += 1;
        }
    }
    draft.review.note=format!("Research saved {added} additional references; {unavailable} searches or pages were unavailable. Continuing with the captured evidence.");
    repo.checkpoint_outline(program, draft, &[], &[]).await?;
    Ok(added)
}
