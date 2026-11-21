//! The backend's record of each model role's warm-up.
//!
//! Every warm-up, whether at boot or from Settings, is recorded here and
//! announced on `model:warmup-status`. A window that opens after one began
//! reads the board, then follows the events; each record carries a revision
//! from one counter, so the newer of two statuses is always known.

use lattice_core::shared::error::AppError;
use std::sync::Arc;

/// A model role that is warmed up ahead of its first use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WarmupRole {
    Chat,
    Utility,
    Embedding,
    Learning,
}

impl WarmupRole {
    pub(crate) const ALL: [WarmupRole; 4] = [
        WarmupRole::Chat,
        WarmupRole::Utility,
        WarmupRole::Embedding,
        WarmupRole::Learning,
    ];

    /// The role's name as it appears on the wire and in logs.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WarmupRole::Chat => "chat",
            WarmupRole::Utility => "utility",
            WarmupRole::Embedding => "embedding",
            WarmupRole::Learning => "learning",
        }
    }
}

/// How far a model role's warm-up has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WarmupPhase {
    Started,
    Ready,
    Skipped,
    Failed,
}

/// One model role's latest warm-up, numbered by the backend's one counter.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoleWarmupDto {
    pub role: WarmupRole,
    pub phase: WarmupPhase,
    pub error: Option<String>,
    pub degraded: Option<String>,
    pub revision: u64,
}

/// How a warm-up ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WarmupOutcome {
    /// Loaded; with the reason when it runs degraded (on the CPU).
    Ready {
        degraded: Option<String>,
    },
    Skipped,
    Failed(String),
}

impl WarmupOutcome {
    /// First-run state (`AiModelsNotInstalled`) and router opt-out
    /// (`InvalidConfig("...not configured...")`) are Skipped, not Failed —
    /// the chat input mask shouldn't treat them as load errors.
    ///
    /// `ServiceNotAvailable` carries an unusable llama-server binary; its
    /// message is already user-facing, so it goes out without the prefix.
    pub(crate) fn from_load_error(e: AppError) -> Self {
        match &e {
            AppError::AiModelsNotInstalled(_) => WarmupOutcome::Skipped,
            AppError::InvalidConfig(msg) if msg.contains("not configured") => {
                WarmupOutcome::Skipped
            }
            AppError::ModelLoadFailed(msg) | AppError::ServiceNotAvailable(msg) => {
                WarmupOutcome::Failed(msg.clone())
            }
            _ => WarmupOutcome::Failed(e.to_string()),
        }
    }
}

#[derive(Default)]
struct BoardState {
    revision: u64,
    roles: Vec<RoleWarmupDto>,
}

impl BoardState {
    fn write(
        &mut self,
        role: WarmupRole,
        phase: WarmupPhase,
        error: Option<String>,
        degraded: Option<String>,
    ) -> RoleWarmupDto {
        self.revision += 1;
        let entry = RoleWarmupDto {
            role,
            phase,
            error,
            degraded,
            revision: self.revision,
        };
        match self.roles.iter_mut().find(|held| held.role == role) {
            Some(held) => *held = entry.clone(),
            None => self.roles.push(entry.clone()),
        }
        entry
    }
}

/// Every role's latest warm-up, with one revision counter across them.
#[derive(Default)]
pub(crate) struct WarmupBoard {
    state: parking_lot::Mutex<BoardState>,
}

impl WarmupBoard {
    fn start(&self, role: WarmupRole) -> RoleWarmupDto {
        let mut state = self.state.lock();
        state.write(role, WarmupPhase::Started, None, None)
    }

    /// Ends the warm-up of `role` that began at revision `began`. Once a newer
    /// warm-up of the role has begun, the older one's end is not the role's
    /// status, and nothing is recorded.
    fn end(
        &self,
        role: WarmupRole,
        began: u64,
        phase: WarmupPhase,
        error: Option<String>,
        degraded: Option<String>,
    ) -> Option<RoleWarmupDto> {
        let mut state = self.state.lock();
        let superseded = state
            .roles
            .iter()
            .any(|held| held.role == role && held.revision != began);
        if superseded {
            return None;
        }
        Some(state.write(role, phase, error, degraded))
    }

    /// The latest record of each role that has warmed up, in role order.
    pub(crate) fn snapshot(&self) -> Vec<RoleWarmupDto> {
        let state = self.state.lock();
        WarmupRole::ALL
            .iter()
            .filter_map(|role| state.roles.iter().find(|held| held.role == *role).cloned())
            .collect()
    }
}

/// One warm-up in progress. Finishing it records its outcome; dropping it
/// without one records `Skipped`, so a warm-up that ends without an outcome
/// (cancelled at shutdown, or a panic) still ends and the window never waits
/// on it.
pub(crate) struct WarmupTicket {
    board: Arc<WarmupBoard>,
    app: Option<tauri::AppHandle>,
    role: WarmupRole,
    began: u64,
    finished: bool,
}

impl WarmupTicket {
    /// Records `role` as started and announces it.
    pub(crate) fn begin(
        board: Arc<WarmupBoard>,
        app: Option<tauri::AppHandle>,
        role: WarmupRole,
    ) -> Self {
        let status = board.start(role);
        emit(app.as_ref(), &status);
        Self {
            board,
            app,
            role,
            began: status.revision,
            finished: false,
        }
    }

    /// Records how the warm-up ended and announces it.
    pub(crate) fn finish(mut self, outcome: WarmupOutcome) {
        let (phase, error, degraded) = match outcome {
            WarmupOutcome::Ready { degraded } => (WarmupPhase::Ready, None, degraded),
            WarmupOutcome::Skipped => (WarmupPhase::Skipped, None, None),
            WarmupOutcome::Failed(message) => (WarmupPhase::Failed, Some(message), None),
        };
        self.end(phase, error, degraded);
    }

    fn end(&mut self, phase: WarmupPhase, error: Option<String>, degraded: Option<String>) {
        self.finished = true;
        if let Some(status) = self
            .board
            .end(self.role, self.began, phase, error, degraded)
        {
            emit(self.app.as_ref(), &status);
        }
    }
}

impl Drop for WarmupTicket {
    fn drop(&mut self) {
        if !self.finished {
            self.end(WarmupPhase::Skipped, None, None);
        }
    }
}

fn emit(app: Option<&tauri::AppHandle>, status: &RoleWarmupDto) {
    use tauri::Emitter;
    let Some(app) = app else {
        return;
    };
    if let Err(e) = app.emit("model:warmup-status", status) {
        tracing::warn!(
            role = status.role.as_str(),
            phase = ?status.phase,
            error = %e,
            "Failed to emit warmup status event"
        );
    }
}

#[cfg(test)]
#[path = "warmup_tests.rs"]
mod tests;
