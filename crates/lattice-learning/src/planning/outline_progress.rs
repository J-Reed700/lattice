//! Per-request outline progress and cancellation. Draft text never crosses IPC.
use lattice_core::shared::{
    error::{AppError, Result},
    ipc::{ApiError, ErrorCode},
};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const MAX_RECENT_OUTCOMES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OutlineOperationKind {
    Generate,
    Repair,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineRunDto {
    pub request_id: String,
    pub kind: OutlineOperationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub course_depth: Option<crate::dto::LearningCourseDepth>,
    pub progress: LearningOutlineProgressDto,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineOutcomeDto {
    pub request_id: String,
    pub kind: OutlineOperationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineRunsSnapshotDto {
    pub active: Vec<LearningOutlineRunDto>,
    pub recent_outcomes: Vec<LearningOutlineOutcomeDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OutlineStage {
    ReadingSources,
    LoadingModel,
    GatheringReferences,
    Drafting,
    CheckingCitations,
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
    /// Progress inside a long stage ("Subtopic 3 of 8: searching"), so a deep
    /// dive's gathering and evidence linking are never a silent spinner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Set while the run waits out a temporary failure of the model server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting: Option<ModelWaitDto>,
    pub elapsed_seconds: u32,
    pub stage_seconds: u32,
    pub response_characters: u32,
    pub model_name: Option<String>,
}

/// A wait for the model server: when the next attempt is, and the code of
/// the failure being waited out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelWaitDto {
    pub retry_in_seconds: u64,
    pub error_code: ErrorCode,
}

struct State {
    stage: OutlineStage,
    detail: Option<String>,
    waiting: Option<ModelWaitDto>,
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
    /// The registry's change counter, moved with every published change.
    changed: Option<Arc<tokio::sync::watch::Sender<u64>>>,
}

impl Default for OutlineProgress {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            started: Instant::now(),
            state: Mutex::new(State {
                stage: OutlineStage::ReadingSources,
                detail: None,
                waiting: None,
                program_id: None,
                since: Instant::now(),
                characters: 0,
                model: None,
                public_failure: None,
            }),
            cancellation: CancellationToken::new(),
            publish: None,
            changed: None,
        }
    }
}

impl OutlineProgress {
    fn new_for_registry(
        id: String,
        program_id: Option<String>,
        publish: impl Fn(LearningOutlineProgressDto) + Send + Sync + 'static,
        changed: Arc<tokio::sync::watch::Sender<u64>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            id,
            started: Instant::now(),
            state: Mutex::new(State {
                stage: OutlineStage::ReadingSources,
                detail: None,
                waiting: None,
                program_id,
                since: Instant::now(),
                characters: 0,
                model: None,
                public_failure: None,
            }),
            cancellation: CancellationToken::new(),
            publish: Some(Box::new(publish)),
            changed: Some(changed),
        })
    }

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
            state.detail = None;
            state.since = Instant::now();
            state.characters = 0;
        }
        self.emit();
    }

    /// Report a step within the current stage. Cleared by the next stage.
    pub fn detail(&self, detail: impl Into<String>) {
        let detail = detail.into();
        tracing::info!(operation_id = %self.id, %detail, "Learning outline stage progress");
        self.state.lock().unwrap_or_else(|e| e.into_inner()).detail = Some(detail);
        self.emit();
    }

    /// Record that the run is waiting out `cause` for `seconds`, or, with
    /// `None`, that the model server answered. Publishes only a change.
    pub fn model_wait(&self, seconds: Option<u64>, cause: Option<&AppError>) {
        let wait = seconds
            .zip(cause)
            .map(|(retry_in_seconds, cause)| ModelWaitDto {
                retry_in_seconds,
                error_code: ApiError::from(cause.clone()).code,
            });
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.waiting == wait {
                return;
            }
            state.waiting = wait;
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
            detail: state.detail.clone(),
            waiting: state.waiting.clone(),
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
        if let Some(changed) = &self.changed {
            moved(changed);
        }
    }

    fn cancelled_error(&self) -> AppError {
        let message =
            "Course generation cancelled. Your inputs and any saved draft corrections are kept.";
        self.record_public_failure(ErrorCode::ServiceNotAvailable, message.into());
        AppError::Cancelled(message.into())
    }

    fn record_public_failure(&self, code: ErrorCode, message: String) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .public_failure = Some(ApiError {
            code,
            message,
            details: None,
        });
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

struct ActiveRunEntry {
    kind: OutlineOperationKind,
    goal: Option<String>,
    course_depth: Option<crate::dto::LearningCourseDepth>,
    progress: Arc<OutlineProgress>,
}

#[derive(Default)]
struct RegistryState {
    active: HashMap<String, ActiveRunEntry>,
    recent_outcomes: VecDeque<LearningOutlineOutcomeDto>,
}

/// The one registry of in-flight outline runs for this process. Built by the
/// composition root and handed to the commands that start or stop a run, so a
/// duplicate request id is rejected against the same map the stop button
/// consults — and a test never sees a run another test registered.
#[derive(Clone)]
pub struct OutlineRunRegistry {
    state: Arc<Mutex<RegistryState>>,
    /// Moved when a run starts, moves on, or ends.
    changed: Arc<tokio::sync::watch::Sender<u64>>,
}

impl Default for OutlineRunRegistry {
    fn default() -> Self {
        Self {
            state: Arc::default(),
            changed: Arc::new(tokio::sync::watch::channel(0).0),
        }
    }
}

fn moved(changed: &tokio::sync::watch::Sender<u64>) {
    changed.send_modify(|count| *count = count.wrapping_add(1));
}

impl OutlineRunRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn validate_request_id(id: Option<String>) -> Result<String> {
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        uuid::Uuid::parse_str(&id)
            .map_err(|_| AppError::InvalidInput("Invalid outline request ID".into()))?;
        Ok(id)
    }

    fn insert_run(
        &self,
        state: &mut RegistryState,
        id: String,
        kind: OutlineOperationKind,
        goal: Option<String>,
        course_depth: Option<crate::dto::LearningCourseDepth>,
        program_id: Option<String>,
        publish: impl Fn(LearningOutlineProgressDto) + Send + Sync + 'static,
    ) -> OutlineRun {
        let progress = OutlineProgress::new_for_registry(
            id.clone(),
            program_id,
            publish,
            Arc::clone(&self.changed),
        );
        state.active.insert(
            id,
            ActiveRunEntry {
                kind,
                goal,
                course_depth,
                progress: Arc::clone(&progress),
            },
        );
        moved(&self.changed);
        OutlineRun {
            progress,
            kind,
            registry: self.clone(),
        }
    }

    pub fn register(
        &self,
        id: Option<String>,
        publish: impl Fn(LearningOutlineProgressDto) + Send + Sync + 'static,
    ) -> Result<OutlineRun> {
        let id = Self::validate_request_id(id)?;
        let mut state = self.lock();
        if state.active.contains_key(&id) {
            return Err(AppError::InvalidInput(
                "This outline request is already running.".into(),
            ));
        }
        Ok(self.insert_run(
            &mut state,
            id,
            OutlineOperationKind::Generate,
            None,
            None,
            None,
            publish,
        ))
    }

    /// Registers an outline generation run, enforcing single-flight generation
    /// and clearing stale earlier generation outcomes.
    pub fn register_generate(
        &self,
        id: Option<String>,
        goal: String,
        course_depth: Option<crate::dto::LearningCourseDepth>,
        publish: impl Fn(LearningOutlineProgressDto) + Send + Sync + 'static,
    ) -> Result<OutlineRun> {
        let id = Self::validate_request_id(id)?;
        let mut state = self.lock();
        if state.active.contains_key(&id) {
            return Err(AppError::InvalidInput(
                "This outline request is already running.".into(),
            ));
        }
        if state
            .active
            .values()
            .any(|entry| entry.kind == OutlineOperationKind::Generate)
        {
            return Err(AppError::Conflict(
                "A course outline is already being generated. Return to Studio to follow its progress or cancel it first.".into(),
            ));
        }
        state
            .recent_outcomes
            .retain(|outcome| outcome.kind != OutlineOperationKind::Generate);
        Ok(self.insert_run(
            &mut state,
            id,
            OutlineOperationKind::Generate,
            Some(goal),
            course_depth,
            None,
            publish,
        ))
    }

    /// Registers an outline repair run for `program_id`, enforcing single-flight
    /// per course and pre-populating `program_id` on initial progress.
    pub fn register_repair(
        &self,
        id: Option<String>,
        program_id: String,
        publish: impl Fn(LearningOutlineProgressDto) + Send + Sync + 'static,
    ) -> Result<OutlineRun> {
        let id = Self::validate_request_id(id)?;
        let mut state = self.lock();
        if state.active.contains_key(&id) {
            return Err(AppError::InvalidInput(
                "This outline request is already running.".into(),
            ));
        }
        if state.active.values().any(|entry| {
            entry.progress.snapshot().program_id.as_deref() == Some(program_id.as_str())
        }) {
            return Err(AppError::Conflict(
                "An outline operation is already running for this course.".into(),
            ));
        }
        state.recent_outcomes.retain(|outcome| {
            !(outcome.kind == OutlineOperationKind::Repair
                && outcome.program_id.as_deref() == Some(program_id.as_str()))
        });
        Ok(self.insert_run(
            &mut state,
            id,
            OutlineOperationKind::Repair,
            None,
            None,
            Some(program_id),
            publish,
        ))
    }

    /// Every run in flight by its request id, oldest first.
    pub fn running(&self) -> Vec<(String, LearningOutlineProgressDto)> {
        let state = self.lock();
        let mut runs: Vec<(Instant, String, LearningOutlineProgressDto)> = state
            .active
            .values()
            .map(|entry| {
                (
                    entry.progress.started,
                    entry.progress.id.clone(),
                    entry.progress.snapshot(),
                )
            })
            .collect();
        runs.sort_by_key(|(started, _, _)| *started);
        runs.into_iter().map(|(_, id, run)| (id, run)).collect()
    }

    /// Returns a snapshot of active runs (oldest first) and recent outcomes.
    pub fn snapshot_runs(&self) -> LearningOutlineRunsSnapshotDto {
        let state = self.lock();
        let mut active: Vec<(Instant, LearningOutlineRunDto)> = state
            .active
            .values()
            .map(|entry| {
                (
                    entry.progress.started,
                    LearningOutlineRunDto {
                        request_id: entry.progress.id.clone(),
                        kind: entry.kind,
                        goal: entry.goal.clone(),
                        course_depth: entry.course_depth,
                        progress: entry.progress.snapshot(),
                    },
                )
            })
            .collect();
        active.sort_by_key(|(started, _)| *started);
        LearningOutlineRunsSnapshotDto {
            active: active.into_iter().map(|(_, run)| run).collect(),
            recent_outcomes: state.recent_outcomes.iter().cloned().collect(),
        }
    }

    fn record_outcome(&self, outcome: LearningOutlineOutcomeDto) {
        let mut state = self.lock();
        state
            .recent_outcomes
            .retain(|existing| existing.request_id != outcome.request_id);
        state.recent_outcomes.push_back(outcome);
        while state.recent_outcomes.len() > MAX_RECENT_OUTCOMES {
            state.recent_outcomes.pop_front();
        }
        moved(&self.changed);
    }

    /// Moves when a run starts, moves on, or ends.
    pub fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Some(entry) = self.lock().active.get(id) {
            if entry.progress.snapshot().stage != OutlineStage::Saving {
                entry.progress.cancellation.cancel();
                return true;
            }
        }
        false
    }
}

pub struct OutlineRun {
    pub progress: Arc<OutlineProgress>,
    kind: OutlineOperationKind,
    registry: OutlineRunRegistry,
}

impl OutlineRun {
    pub fn record_completed(&self, program_id: &str) {
        self.registry.record_outcome(LearningOutlineOutcomeDto {
            request_id: self.progress.id.clone(),
            kind: self.kind,
            program_id: Some(program_id.to_string()),
            cancelled: false,
            error: None,
        });
    }

    pub fn record_failed(&self, error: &ApiError, cancelled: bool) {
        let program_id = self.progress.snapshot().program_id;
        self.registry.record_outcome(LearningOutlineOutcomeDto {
            request_id: self.progress.id.clone(),
            kind: self.kind,
            program_id,
            cancelled,
            error: Some(error.clone()),
        });
    }
}

impl Drop for OutlineRun {
    fn drop(&mut self) {
        self.registry.lock().active.remove(&self.progress.id);
        moved(&self.registry.changed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> OutlineRunRegistry {
        OutlineRunRegistry::new()
    }

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
    fn a_wait_on_a_busy_database_publishes_the_database_code() {
        let progress = OutlineProgress::default();
        progress.model_wait(
            Some(30),
            Some(&AppError::DatabaseBusy("database is locked".into())),
        );
        let wire = serde_json::to_value(progress.snapshot().waiting).unwrap();
        assert_eq!(wire["errorCode"], "DATABASE_BUSY");
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
        let registry = registry();
        let id = uuid::Uuid::new_v4().to_string();
        let run = registry.register(Some(id.clone()), |_| {}).unwrap();
        assert!(registry.register(Some(id.clone()), |_| {}).is_err());
        assert!(registry.cancel(&id));
        assert!(matches!(
            run.progress.run(std::future::pending::<Result<()>>()).await,
            Err(AppError::Cancelled(_))
        ));
        assert_eq!(run.progress.snapshot().stage, OutlineStage::Cancelled);
        drop(run);
        assert!(!registry.cancel(&id));
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

    #[test]
    fn a_wait_for_the_model_server_is_a_field_while_it_lasts_and_gone_after() {
        use crate::patient_model::WaitObserver;
        let progress = OutlineProgress::default();
        let quiet = serde_json::to_value(progress.snapshot()).unwrap();
        assert!(quiet.get("waiting").is_none(), "{quiet}");

        progress.waiting(30, &AppError::Network("connection reset".into()));
        let waiting = serde_json::to_value(progress.snapshot()).unwrap();
        assert_eq!(
            waiting["waiting"],
            serde_json::json!({"retryInSeconds": 30, "errorCode": "NETWORK_ERROR"}),
            "{waiting}"
        );
        assert_eq!(
            waiting["detail"],
            "Waiting for the model server (retrying in 30s)"
        );

        progress.answered();
        let answered = serde_json::to_value(progress.snapshot()).unwrap();
        assert!(answered.get("waiting").is_none(), "{answered}");
    }

    #[test]
    fn register_generate_enforces_single_flight_until_dropped() {
        use crate::dto::LearningCourseDepth;

        let registry = registry();
        let run1 = registry
            .register_generate(
                None,
                "Learn Rust async".into(),
                Some(LearningCourseDepth::Course),
                |_| {},
            )
            .unwrap();

        let err = match registry.register_generate(
            None,
            "Learn Linear Algebra".into(),
            Some(LearningCourseDepth::Focused),
            |_| {},
        ) {
            Ok(_) => panic!("expected conflict for second concurrent generate"),
            Err(err) => err,
        };
        assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");

        drop(run1);

        let _run2 = registry
            .register_generate(
                None,
                "Learn Linear Algebra".into(),
                Some(LearningCourseDepth::Focused),
                |_| {},
            )
            .unwrap();
    }

    #[test]
    fn register_repair_populates_program_id_and_enforces_single_flight_per_course() {
        let registry = registry();
        let program_a = uuid::Uuid::new_v4().to_string();
        let program_b = uuid::Uuid::new_v4().to_string();

        let repair_a = registry
            .register_repair(None, program_a.clone(), |_| {})
            .unwrap();
        assert_eq!(
            repair_a.progress.snapshot().program_id.as_deref(),
            Some(program_a.as_str())
        );

        let err = match registry.register_repair(None, program_a.clone(), |_| {}) {
            Ok(_) => panic!("expected conflict for concurrent repair on same course"),
            Err(err) => err,
        };
        assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");

        let _repair_b = registry
            .register_repair(None, program_b.clone(), |_| {})
            .unwrap();
        assert_eq!(
            _repair_b.progress.snapshot().program_id.as_deref(),
            Some(program_b.as_str())
        );
    }

    #[test]
    fn snapshot_runs_exposes_metadata_and_recent_outcomes_with_stale_generate_cleared() {
        use crate::dto::LearningCourseDepth;

        let registry = registry();
        let req1 = uuid::Uuid::new_v4().to_string();
        let run1 = registry
            .register_generate(
                Some(req1.clone()),
                "Learn category theory".into(),
                Some(LearningCourseDepth::DeepDive),
                |_| {},
            )
            .unwrap();

        let snap = registry.snapshot_runs();
        assert_eq!(snap.active.len(), 1);
        assert_eq!(snap.active[0].request_id, req1);
        assert_eq!(snap.active[0].kind, OutlineOperationKind::Generate);
        assert_eq!(
            snap.active[0].goal.as_deref(),
            Some("Learn category theory")
        );
        assert_eq!(
            snap.active[0].course_depth,
            Some(LearningCourseDepth::DeepDive)
        );
        assert!(snap.recent_outcomes.is_empty());

        let api_err = ApiError {
            code: ErrorCode::ServiceNotAvailable,
            message: "cancelled".into(),
            details: None,
        };
        run1.record_failed(&api_err, true);
        drop(run1);

        let snap_after_fail = registry.snapshot_runs();
        assert!(snap_after_fail.active.is_empty());
        assert_eq!(snap_after_fail.recent_outcomes.len(), 1);
        assert_eq!(snap_after_fail.recent_outcomes[0].request_id, req1);
        assert_eq!(
            snap_after_fail.recent_outcomes[0].kind,
            OutlineOperationKind::Generate
        );
        assert!(snap_after_fail.recent_outcomes[0].cancelled);
        assert_eq!(
            snap_after_fail.recent_outcomes[0]
                .error
                .as_ref()
                .map(|e| e.message.as_str()),
            Some("cancelled")
        );

        let req2 = uuid::Uuid::new_v4().to_string();
        let run2 = registry
            .register_generate(
                Some(req2.clone()),
                "Learn compilers".into(),
                Some(LearningCourseDepth::Course),
                |_| {},
            )
            .unwrap();
        let snap_new_gen = registry.snapshot_runs();
        assert!(
            snap_new_gen.recent_outcomes.is_empty(),
            "starting a new generate should clear stale generate outcomes"
        );

        run2.record_completed("prog-123");
        drop(run2);
        let snap_done = registry.snapshot_runs();
        assert_eq!(snap_done.recent_outcomes.len(), 1);
        assert_eq!(snap_done.recent_outcomes[0].request_id, req2);
        assert_eq!(
            snap_done.recent_outcomes[0].program_id.as_deref(),
            Some("prog-123")
        );
        assert!(!snap_done.recent_outcomes[0].cancelled);
        assert!(snap_done.recent_outcomes[0].error.is_none());
    }
}
