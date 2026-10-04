//! Per-request outline progress and cancellation. Draft text never crosses IPC.
use crate::shared::error::{AppError, Result};
use serde::Serialize;
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub const CALL_BUDGET: Duration = Duration::from_secs(300);
const OUTLINE_BUDGET: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OutlineStage {
    ReadingSources,
    LoadingModel,
    Drafting,
    Reviewing,
    Repairing,
    CheckingRepair,
    Saving,
    Completed,
    Cancelled,
    Failed,
}

impl OutlineStage {
    fn label(self) -> &'static str {
        match self {
            Self::ReadingSources => "reading references",
            Self::LoadingModel => "loading the model",
            Self::Drafting => "writing the outline",
            Self::Reviewing => "reviewing the teaching plan",
            Self::Repairing => "revising the outline",
            Self::CheckingRepair => "checking the revised outline",
            Self::Saving => "saving the outline",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineProgressDto {
    pub stage: OutlineStage,
    pub elapsed_seconds: u32,
    pub stage_seconds: u32,
    pub response_characters: u32,
    pub model_name: Option<String>,
}

struct State {
    stage: OutlineStage,
    since: Instant,
    characters: u32,
    model: Option<String>,
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
                since: Instant::now(),
                characters: 0,
                model: None,
            }),
            cancellation: CancellationToken::new(),
            publish: None,
        }
    }
}

impl OutlineProgress {
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

    pub fn timeout_error(&self) -> AppError {
        AppError::ServiceNotAvailable(format!("Course generation timed out while {}. Your inputs are kept. Try a focused course or a faster model.", self.snapshot().stage.label()))
    }

    pub fn check_cancelled(&self) -> Result<()> {
        if self.cancellation.is_cancelled() {
            Err(AppError::ServiceNotAvailable(
                "Course generation cancelled. Your inputs are kept.".into(),
            ))
        } else {
            Ok(())
        }
    }

    pub async fn run<T>(&self, work: impl Future<Output = Result<T>>) -> Result<T> {
        self.run_bounded(work, OUTLINE_BUDGET).await
    }

    async fn run_bounded<T>(
        &self,
        work: impl Future<Output = Result<T>>,
        budget: Duration,
    ) -> Result<T> {
        tokio::pin!(work);
        let deadline = tokio::time::sleep(budget);
        tokio::pin!(deadline);
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
                    break Err(AppError::ServiceNotAvailable("Course generation cancelled. Your inputs are kept.".into()));
                }
                _ = &mut deadline => {
                    if self.snapshot().stage == OutlineStage::Saving { break work.await; }
                    break Err(self.timeout_error());
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

    #[tokio::test]
    async fn stalled_work_times_out_with_its_stage() {
        let progress = OutlineProgress::default();
        progress.stage(OutlineStage::Reviewing);
        let result = progress
            .run_bounded(
                std::future::pending::<Result<()>>(),
                Duration::from_millis(5),
            )
            .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("reviewing the teaching plan"));
        assert_eq!(progress.snapshot().stage, OutlineStage::Failed);
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
