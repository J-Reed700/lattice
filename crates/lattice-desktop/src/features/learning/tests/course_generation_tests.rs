//! Stands in for `lattice_learning::lessons::course_generation_tests`, which declared this eval.

use super::pool;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_learning::dto::GenerateLearningProgramRequestDto;
use lattice_learning::repository::LearningRepository;
use lattice_learning::service;
use serde_json::json;

/// Explicit live acceptance test: uses the configured remote llama.cpp model,
/// captures the public URL through the real web service, and saves only to a
/// temporary database. Credentials and generated content are never printed.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS and LATTICE_OUTLINE_REQUEST"]
async fn live_outline_generation_and_save() -> Result<()> {
    use crate::features::{llm::llama_cpp::LlamaCppLlm, web::services::web::WebService};
    use lattice_core::application::ports::WebServiceTrait;
    let request_path = std::env::var("LATTICE_OUTLINE_REQUEST").unwrap();
    let config = crate::tests::live_llama_cpp::settings()?;
    let header = crate::tests::live_llama_cpp::header(&config.llama_cpp.auth_header_name)?;
    let request: GenerateLearningProgramRequestDto =
        serde_json::from_slice(&std::fs::read(request_path)?)?;
    assert!(
        request.document_ids.is_empty(),
        "live fixture supports public URLs only"
    );
    service::validate_request(&request)?;
    let temp = tempfile::tempdir()?;
    let web = WebService::new(temp.path())?;
    struct ObservedModel {
        inner: std::sync::Arc<dyn LLMPort>,
        call: std::sync::atomic::AtomicUsize,
        replay_draft: Option<String>,
    }
    #[async_trait::async_trait]
    impl LLMPort for ObservedModel {
        async fn complete(
            &self,
            request: &lattice_core::application::ports::llm_port::CompletionRequest,
        ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
            self.inner.complete(request).await
        }

        async fn complete_with_progress(
            &self,
            request: &lattice_core::application::ports::llm_port::CompletionRequest,
            on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
            let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let replayed = call == 0 && self.replay_draft.is_some();
            let response = if let Some(draft) = self.replay_draft.as_ref().filter(|_| call == 0) {
                println!(
                    "Replaying a previously recorded draft; this step makes no model request."
                );
                on_text(draft.clone())?;
                lattice_core::application::ports::llm_port::CompletionResponse {
                    text: draft.clone(),
                    finish_reason: "stop".into(),
                    ..Default::default()
                }
            } else {
                let response = self.inner.complete_with_progress(request, on_text).await?;
                println!(
                    "Live model call {call}: {} input tokens, {} output tokens, finish {}",
                    response.input_tokens, response.output_tokens, response.finish_reason
                );
                response
            };
            // Optional local test artifacts contain only our prompts and public
            // answer text, never provider reasoning state or connection settings.
            if let Ok(directory) = std::env::var("LATTICE_OUTLINE_CALLS") {
                std::fs::create_dir_all(&directory)?;
                std::fs::write(
                    std::path::Path::new(&directory).join(format!("call-{call}.json")),
                    serde_json::to_vec_pretty(&json!({
                        "input":request.input,"answer":response.text,"finishReason":response.finish_reason,
                        "inputTokens":response.input_tokens,"outputTokens":response.output_tokens,"replayed":replayed
                    }))?,
                )?;
            }
            Ok(response)
        }

        fn model_name(&self) -> &str {
            self.inner.model_name()
        }
        fn max_context_tokens(&self) -> usize {
            self.inner.max_context_tokens()
        }
        fn count_tokens(&self, text: &str) -> usize {
            self.inner.count_tokens(text)
        }
        async fn is_ready(&self) -> Result<bool> {
            self.inner.is_ready().await
        }
    }
    let client = ObservedModel {
        inner: LlamaCppLlm::new(&config, header)?.into_port().await,
        call: std::sync::atomic::AtomicUsize::new(0),
        replay_draft: std::env::var("LATTICE_OUTLINE_REPLAY_DRAFT")
            .ok()
            .map(|path| -> Result<String> {
                let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
                recorded["answer"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        AppError::InvalidInput("Recorded call has no public draft text".into())
                    })
            })
            .transpose()?,
    };
    let repo = LearningRepository::new(lattice_core::shared::persistence::Database::new(
        pool().await?,
    ));
    let start = std::time::Instant::now();
    let last_stage = std::sync::Mutex::new((None, 0));
    let registry = lattice_learning::outline_progress::OutlineRunRegistry::new();
    let run = registry.register(None, move |update| {
        let mut last = last_stage.lock().unwrap();
        if last.0 != Some(update.stage) || update.elapsed_seconds >= last.1 + 30 {
            println!(
                "Live outline: {:?} at {}s, {} response characters",
                update.stage, update.elapsed_seconds, update.response_characters
            );
            *last = (Some(update.stage), update.elapsed_seconds);
        }
    })?;
    let progress = &run.progress;
    let drafts = lattice_learning::outline_draft::DraftRunRegistry::new();
    let program = progress
        .run(async {
            progress.stage(lattice_learning::outline_progress::OutlineStage::ReadingSources);
            let mut sources = Vec::new();
            let mut references = Vec::new();
            for url in &request.source_urls {
                let article = web.fetch_reference_content(url).await?;
                let captured = lattice_learning::source_library::CapturedLearningSource {
                    title: article.title.unwrap_or_else(|| url.clone()),
                    publisher: None,
                    requested_url: Some(url.clone()),
                    resolved_url: Some(article.url),
                    text: article.content,
                    truncated: article.content_truncated,
                    extraction_version: "web_reference_v1".into(),
                    signals: lattice_learning::references::trust::capture::not_extracted(
                        "test fixture",
                    ),
                };
                assert!(!captured.truncated);
                let id = uuid::Uuid::new_v4().to_string();
                sources.push(lattice_learning::sources::preview(&id, &captured));
                references.push(lattice_learning::sources::InitialReference {
                    source_id: id,
                    origin: url.clone(),
                    captured,
                    found: false,
                });
            }
            progress.model(client.model_name());
            service::generate_with_references_and_progress(
                &repo,
                &drafts,
                &client,
                request,
                sources,
                &references,
                progress,
                Some(&web),
            )
            .await
        })
        .await?;
    assert_eq!(repo.list().await?.len(), 1);
    assert!(!program.modules.is_empty());
    println!(
        "Live outline saved: {} modules, {} lessons in {}s",
        program.summary.module_count,
        program.summary.lesson_count,
        start.elapsed().as_secs()
    );
    if let Ok(path) = std::env::var("LATTICE_OUTLINE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&program)?)?;
    }
    Ok(())
}
