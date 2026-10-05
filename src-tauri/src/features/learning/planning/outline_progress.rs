//! Per-request outline progress and cancellation. Draft text never crosses IPC.
use crate::shared::{
    error::{AppError, Result},
    ipc::{ApiError, ErrorCode},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OutlineStage {
    ReadingSources,
    LoadingModel,
    Drafting,
    Reviewing,
    Researching,
    Repairing,
    CheckingRepair,
    Saving,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineProgressDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
    pub stage: OutlineStage,
    pub elapsed_seconds: u32,
    pub stage_seconds: u32,
    pub response_characters: u32,
    pub model_name: Option<String>,
}

struct State {
    stage: OutlineStage,
    program_id: Option<String>,
    since: Instant,
    characters: u32,
    model: Option<String>,
    public_failure: Option<ApiError>,
}

pub struct OutlineProgress {
    id: String,
    started: Instant,
    state: Mutex<State>,
    cancellation: CancellationToken,
    publish: Option<Box<dyn Fn(LearningOutlineProgressDto) + Send + Sync>>,
}

impl Default for OutlineProgress {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            started: Instant::now(),
            state: Mutex::new(State {
                stage: OutlineStage::ReadingSources,
                program_id: None,
                since: Instant::now(),
                characters: 0,
                model: None,
                public_failure: None,
            }),
            cancellation: CancellationToken::new(),
            publish: None,
        }
    }
}

impl OutlineProgress {
    pub fn saved(&self, id: &str) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .program_id = Some(id.into());
        self.emit();
    }
    pub fn stage(&self, stage: OutlineStage) {
        let previous = self.snapshot();
        tracing::info!(operation_id = %self.id, previous_stage = ?previous.stage, stage = ?stage, elapsed_seconds = previous.elapsed_seconds, response_characters = previous.response_characters, "Learning outline stage changed");
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.stage = stage;
            state.since = Instant::now();
            state.characters = 0;
        }
        self.emit();
    }

    pub fn model(&self, name: &str) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).model = Some(name.into());
    }

    pub fn received(&self, text: &str) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.characters = state
            .characters
            .saturating_add(text.chars().count().min(u32::MAX as usize) as u32);
    }

    pub fn snapshot(&self) -> LearningOutlineProgressDto {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        LearningOutlineProgressDto {
            program_id: state.program_id.clone(),
            stage: state.stage,
            elapsed_seconds: self.started.elapsed().as_secs().min(u32::MAX as u64) as u32,
            stage_seconds: state.since.elapsed().as_secs().min(u32::MAX as u64) as u32,
            response_characters: state.characters,
            model_name: state.model.clone(),
        }
    }

    fn emit(&self) {
        if let Some(publish) = &self.publish {
            publish(self.snapshot());
        }
    }

    fn cancelled_error(&self) -> AppError {
        self.public_failure(
            ErrorCode::ServiceNotAvailable,
            "Course generation cancelled. Your inputs and any saved draft corrections are kept."
                .into(),
        )
    }

    fn public_failure(&self, code: ErrorCode, message: String) -> AppError {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .public_failure = Some(ApiError {
            code,
            message: message.clone(),
            details: None,
        });
        AppError::ServiceNotAvailable(message)
    }

    /// Preserve our safe, stage-specific explanation across IPC. Other errors
    /// retain the shared sanitization policy; provider details are not surfaced.
    pub fn api_error(&self, error: AppError) -> ApiError {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .public_failure
            .clone()
            .unwrap_or_else(|| ApiError::from(error))
    }

    pub fn check_cancelled(&self) -> Result<()> {
        if self.cancellation.is_cancelled() {
            Err(self.cancelled_error())
        } else {
            Ok(())
        }
    }

    pub async fn run<T>(&self, work: impl Future<Output = Result<T>>) -> Result<T> {
        // Deliberately no overall deadline: elapsed time is progress information,
        // not evidence that a multi-step course generation has failed.
        tokio::pin!(work);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(2));
        let mut last_log = Instant::now();
        let result = loop {
            tokio::select! {
                biased;
                result = &mut work => break result,
                _ = self.cancellation.cancelled() => {
                    // Once the atomic save starts, finish it and return the saved
                    // program. Never report cancellation after a successful commit.
                    if self.snapshot().stage == OutlineStage::Saving { break work.await; }
                    break Err(self.cancelled_error());
                }
                _ = heartbeat.tick() => {
                    self.emit();
                    if last_log.elapsed() >= Duration::from_secs(30) {
                        let update = self.snapshot();
                        tracing::info!(operation_id = %self.id, stage = ?update.stage, elapsed_seconds = update.elapsed_seconds, response_characters = update.response_characters, "Learning outline is running");
                        last_log = Instant::now();
                    }
                },
            }
        };
        match &result {
            Ok(_) => self.stage(OutlineStage::Completed),
            Err(error) => {
                tracing::warn!(operation_id = %self.id, stage = ?self.snapshot().stage, %error, "Learning outline stopped");
                self.stage(if self.cancellation.is_cancelled() {
                    OutlineStage::Cancelled
                } else {
                    OutlineStage::Failed
                });
            }
        }
        result
    }
}

static ACTIVE: LazyLock<Mutex<HashMap<String, Arc<OutlineProgress>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct OutlineRun(pub Arc<OutlineProgress>);
impl OutlineRun {
    pub fn register(
        id: Option<String>,
        publish: impl Fn(LearningOutlineProgressDto) + Send + Sync + 'static,
    ) -> Result<Self> {
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        uuid::Uuid::parse_str(&id)
            .map_err(|_| AppError::InvalidInput("Invalid outline request ID".into()))?;
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if active.contains_key(&id) {
            return Err(AppError::InvalidInput(
                "This outline request is already running.".into(),
            ));
        }
        let progress = Arc::new(OutlineProgress {
            id: id.clone(),
            publish: Some(Box::new(publish)),
            ..Default::default()
        });
        active.insert(id, progress.clone());
        Ok(Self(progress))
    }
}
impl Drop for OutlineRun {
    fn drop(&mut self) {
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0.id);
    }
}

pub fn cancel(id: &str) -> bool {
    if let Some(progress) = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
        if progress.snapshot().stage != OutlineStage::Saving {
            progress.cancellation.cancel();
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn long_running_work_has_no_overall_deadline() {
        let progress = OutlineProgress::default();
        let result = progress
            .run(async {
                for stage in [
                    OutlineStage::Drafting,
                    OutlineStage::Reviewing,
                    OutlineStage::Repairing,
                    OutlineStage::CheckingRepair,
                ] {
                    progress.stage(stage);
                    tokio::time::sleep(Duration::from_secs(600)).await;
                }
                Ok("reviewed")
            })
            .await
            .unwrap();
        assert_eq!(result, "reviewed");
        assert_eq!(progress.snapshot().stage, OutlineStage::Completed);
    }

    #[test]
    fn cancellation_explanation_survives_ipc() {
        let progress = OutlineProgress::default();
        progress.cancellation.cancel();
        let error = progress.check_cancelled().unwrap_err();
        progress.stage(OutlineStage::Cancelled);
        let wire = serde_json::to_value(progress.api_error(error)).unwrap();
        assert_eq!(
            wire["message"],
            "Course generation cancelled. Your inputs and any saved draft corrections are kept."
        );
    }

    #[test]
    fn unrelated_errors_keep_the_shared_sanitization_policy() {
        let progress = OutlineProgress::default();
        let wire = progress.api_error(AppError::ServiceNotAvailable(
            "private provider detail".into(),
        ));
        assert_eq!(wire.message, "Service not available");
    }

    #[tokio::test]
    async fn cancellation_drops_work_and_registration_is_cleaned_up() {
        let id = uuid::Uuid::new_v4().to_string();
        let run = OutlineRun::register(Some(id.clone()), |_| {}).unwrap();
        assert!(OutlineRun::register(Some(id.clone()), |_| {}).is_err());
        assert!(cancel(&id));
        assert!(run
            .0
            .run(std::future::pending::<Result<()>>())
            .await
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        assert_eq!(run.0.snapshot().stage, OutlineStage::Cancelled);
        drop(run);
        assert!(!cancel(&id));
    }

    #[tokio::test]
    async fn cancellation_never_hides_a_committed_program() {
        let progress = OutlineProgress::default();
        let result = progress
            .run(async {
                progress.stage(OutlineStage::Saving);
                progress.cancellation.cancel();
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok("saved")
            })
            .await
            .unwrap();
        assert_eq!(result, "saved");
        assert_eq!(progress.snapshot().stage, OutlineStage::Completed);
    }

    #[test]
    fn stage_changes_reset_activity_without_exposing_draft_text() {
        let progress = OutlineProgress::default();
        progress.model("test model");
        progress.stage(OutlineStage::Drafting);
        progress.received("private draft 🦀");
        assert_eq!(progress.snapshot().response_characters, 15);
        assert!(!serde_json::to_string(&progress.snapshot())
            .unwrap()
            .contains("private"));
        progress.stage(OutlineStage::Reviewing);
        assert_eq!(progress.snapshot().response_characters, 0);
        assert_eq!(
            progress.snapshot().model_name.as_deref(),
            Some("test model")
        );
    }
}
