use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub operation_id: Option<String>,
    pub current: usize,
    pub total: usize,
    pub filename: Option<String>,
    pub message: Option<String>,
    pub percentage: f32,
    pub eta_ms: Option<u64>,
}

impl ProgressEvent {
    pub fn new(current: usize, total: usize) -> Self {
        let percentage = if total > 0 {
            (current as f32 / total as f32) * 100.0
        } else {
            0.0
        };

        Self {
            operation_id: None,
            current,
            total,
            filename: None,
            message: None,
            percentage,
            eta_ms: None,
        }
    }

    pub fn with_filename(mut self, filename: String) -> Self {
        self.message = Some(format!("Processing {}", filename));
        self.filename = Some(filename);
        self
    }

    pub fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    pub fn with_operation_id(mut self, id: String) -> Self {
        self.operation_id = Some(id);
        self
    }

    pub fn with_eta(mut self, eta_ms: u64) -> Self {
        self.eta_ms = Some(eta_ms);
        self
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CompleteEvent {
    pub operation_id: Option<String>,
    pub path: Option<String>,
    pub message: Option<String>,
}

impl CompleteEvent {
    pub fn new() -> Self {
        Self {
            operation_id: None,
            path: None,
            message: None,
        }
    }

    pub fn with_path(mut self, path: String) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    pub fn with_operation_id(mut self, id: String) -> Self {
        self.operation_id = Some(id);
        self
    }
}

impl Default for CompleteEvent {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEvent {
    pub operation_id: Option<String>,
    pub message: String,
    pub path: Option<String>,
}

impl ErrorEvent {
    pub fn new(message: String) -> Self {
        Self {
            operation_id: None,
            message,
            path: None,
        }
    }

    pub fn with_path(mut self, path: String) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_operation_id(mut self, id: String) -> Self {
        self.operation_id = Some(id);
        self
    }
}

/// Emit progress events to the frontend
pub struct ProgressEmitter<'a> {
    app: &'a AppHandle,
    operation_type: &'a str,
}

impl<'a> ProgressEmitter<'a> {
    pub fn new(app: &'a AppHandle, operation_type: &'a str) -> Self {
        Self {
            app,
            operation_type,
        }
    }

    pub fn emit_progress(&self, event: ProgressEvent) -> Result<(), tauri::Error> {
        let event_name = format!("{}-progress", self.operation_type);
        self.app
            .emit_to(tauri::EventTarget::Any, &event_name, event)
    }

    pub fn emit_complete(&self, event: CompleteEvent) -> Result<(), tauri::Error> {
        let event_name = format!("{}-complete", self.operation_type);
        self.app
            .emit_to(tauri::EventTarget::Any, &event_name, event)
    }

    pub fn emit_error(&self, event: ErrorEvent) -> Result<(), tauri::Error> {
        let event_name = format!("{}-error", self.operation_type);
        self.app
            .emit_to(tauri::EventTarget::Any, &event_name, event)
    }
}
