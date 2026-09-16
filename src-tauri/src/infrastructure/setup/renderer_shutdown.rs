//! Native close/quit must let the renderer finish repository writes first.
use parking_lot::Mutex;
use tauri::{Emitter, Listener, Manager};

#[derive(Default)]
struct ShutdownState {
    ready: bool,
    approved: bool,
    next_id: u64,
    pending: Option<u64>,
}

impl ShutdownState {
    fn request(&mut self) -> Option<u64> {
        if !self.ready || self.approved {
            return None;
        }
        if let Some(id) = self.pending {
            return Some(id);
        }
        self.next_id += 1;
        self.pending = Some(self.next_id);
        self.pending
    }

    fn respond(&mut self, id: u64, saved: bool) -> bool {
        if self.pending != Some(id) {
            return false;
        }
        self.pending = None;
        self.approved = saved;
        saved
    }
}

type State = Mutex<ShutdownState>;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    request_id: u64,
    saved: bool,
}

pub fn install(app: &tauri::AppHandle) {
    app.manage(State::default());
    let handle = app.clone();
    app.listen("lattice:renderer-ready", move |_| {
        handle.state::<State>().lock().ready = true;
    });
    let handle = app.clone();
    app.listen("lattice:shutdown-response", move |event| {
        let Ok(response) = serde_json::from_str::<Response>(event.payload()) else {
            return;
        };
        let approved = handle
            .state::<State>()
            .lock()
            .respond(response.request_id, response.saved);
        if approved {
            handle.exit(0);
        }
    });
}

/// Returns true when native destruction must be prevented. A repeated quit
/// resends the pending request, allowing recovery from an IPC delivery failure.
pub fn defer(app: &tauri::AppHandle) -> bool {
    let request = app.state::<State>().lock().request();
    let Some(id) = request else { return false };
    if let Err(error) = app.emit_to("main", "lattice:shutdown-requested", id) {
        tracing::error!(%error, "Could not request renderer save; keeping app open");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_can_exit_before_the_editor_is_available() {
        assert_eq!(ShutdownState::default().request(), None);
    }

    #[test]
    fn quit_waits_for_matching_success_and_ignores_stale_replies() {
        let mut state = ShutdownState {
            ready: true,
            ..Default::default()
        };
        let id = state.request().unwrap();
        assert_eq!(state.request(), Some(id));
        assert!(!state.respond(id + 1, true));
        assert_eq!(state.request(), Some(id));
        assert!(state.respond(id, true));
        assert_eq!(state.request(), None);
    }

    #[test]
    fn failed_save_keeps_app_open_and_allows_retry() {
        let mut state = ShutdownState {
            ready: true,
            ..Default::default()
        };
        let id = state.request().unwrap();
        assert!(!state.respond(id, false));
        let retry = state.request().unwrap();
        assert_ne!(retry, id);
        assert!(!state.respond(id, true));
        assert!(state.respond(retry, true));
    }
}
