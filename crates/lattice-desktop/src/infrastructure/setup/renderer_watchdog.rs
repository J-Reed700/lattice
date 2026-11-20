//! Native host supervisor for the main renderer webview.
//!
//! An in-page script (`public/boot-guard.js`) can only recover failures that
//! occur after `index.html` loads inside a live WebContent process. It cannot
//! observe two host-level failure classes that otherwise leave a blank window:
//!
//! 1. Navigation / protocol stalls where `index.html` never commits or hangs
//!    before any page script executes.
//! 2. OS-level WebContent process crashes or Jetsam kills on macOS
//!    (`WKNavigationDelegate::webViewWebContentProcessDidTerminate:`), where
//!    the renderer process disappears mid-session and zero JavaScript runs.
//!
//! This supervisor tracks monotonic navigation generations, listens for the
//! renderer's liveness handshakes (`lattice:boot-mounted`, `lattice:renderer-ready`,
//! and `lattice:boot-recovery`), resets safe-quit readiness whenever the main
//! webview reloads or terminates, performs bounded off-main-thread reloads with
//! backoff, and escalates to a non-blocking native dialog if the sliding-window
//! retry budget is exhausted.
//!
//! Only Lattice's own pages are held to the boot deadline, and only in packaged
//! builds: a page the window was navigated to never sends the handshake, and a
//! dev server's cold start or a paused debugger is not a hang. WebContent crash
//! recovery applies to every page in every build.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::shared::runtime::background::BackgroundTasks;
use parking_lot::Mutex;
use tauri::webview::{PageLoadEvent, PageLoadPayload};
use tauri::{Listener, Manager, Url};

use super::renderer_shutdown;

const MAIN_WEBVIEW_LABEL: &str = "main";

/// Host-level deadline for a navigation generation to either mount React or
/// display the in-page pre-mount recovery screen (`boot-guard.js` runs its own
/// 10s guard first when `index.html` loaded normally). Packaged builds only;
/// see `boot_deadline`.
const BOOT_TIMEOUT: Duration = Duration::from_secs(15);

/// A dev server's cold start has no fair deadline, and a paused debugger is
/// not a hang, so only packaged builds hold the renderer to one. Crash
/// recovery runs in every build.
fn boot_deadline() -> Option<Duration> {
    (!tauri::is_dev()).then_some(BOOT_TIMEOUT)
}

/// Whether `url` is Lattice's own frontend rather than a page the main window
/// was navigated to: the packaged app's custom protocol (`tauri://localhost`
/// on macOS and Linux, `http(s)://tauri.localhost` on Windows) or the dev server.
fn is_app_page(url: &Url, dev_url: Option<&Url>) -> bool {
    let packaged = match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    };
    packaged || dev_url.is_some_and(|dev| url.origin() == dev.origin())
}

/// Sliding time window bounding automatic host reloads across boot timeouts and
/// WebContent crashes alike, preventing crash loops even if a page mounts
/// briefly before crashing.
const RETRY_WINDOW: Duration = Duration::from_secs(60);

/// Maximum automatic reloads allowed within `RETRY_WINDOW` before escalating.
const MAX_RELOADS_PER_WINDOW: usize = 2;

const FIRST_RELOAD_DELAY: Duration = Duration::from_millis(250);
const SECOND_RELOAD_DELAY: Duration = Duration::from_millis(1000);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Navigation started (or reload queued); awaiting renderer mount.
    Loading,
    /// Pre-mount supervisor (`boot-guard.js`) rendered its interactive recovery
    /// view, so the window is not blank and host timeouts must not clobber it.
    InPageRecovery,
    /// React tree mounted (`lattice:boot-mounted`); error boundaries are active.
    Mounted,
    /// Editor safe-shutdown handshake completed (`lattice:renderer-ready`).
    Ready,
    /// Retry budget exhausted and the window still broken; the escalation
    /// prompt is open.
    Escalated,
    /// The main webview left the app for another page. That page sends no boot
    /// handshake, so no deadline applies; a crash still reloads it.
    Foreign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryAction {
    Ignore,
    Reload {
        attempt: usize,
        max_attempts: usize,
        delay: Duration,
        generation: u64,
    },
    Escalate {
        attempts: usize,
    },
}

/// What to do once the user answers the escalation prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptOutcome {
    /// Leave the window alone: it mounted while the prompt was up and may hold
    /// unsaved work, or (after Wait) it shows a page no deadline applies to.
    LeaveAlone,
    /// Reload the window as this navigation generation.
    Reload(u64),
    /// Give this generation a fresh boot deadline. A window still blank when
    /// it lapses resumes recovery, so Wait never strands a broken window.
    Rearm(u64),
}

struct WatchdogState {
    generation: u64,
    phase: Phase,
    reload_attempts: VecDeque<Instant>,
    /// An escalation dialog is on screen; further failures must not stack another.
    prompt_open: bool,
}

impl Default for WatchdogState {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchdogState {
    fn new() -> Self {
        Self {
            generation: 1,
            phase: Phase::Loading,
            reload_attempts: VecDeque::with_capacity(MAX_RELOADS_PER_WINDOW),
            prompt_open: false,
        }
    }

    fn current_loading_generation(&self) -> Option<u64> {
        (self.phase == Phase::Loading).then_some(self.generation)
    }

    fn on_page_load_started(&mut self) -> u64 {
        self.generation += 1;
        self.phase = Phase::Loading;
        self.generation
    }

    /// Invalidates timers and pending reloads from the previous page.
    fn on_foreign_page(&mut self) {
        self.generation += 1;
        self.phase = Phase::Foreign;
    }

    fn on_boot_mounted(&mut self) {
        if self.phase != Phase::Ready {
            self.phase = Phase::Mounted;
        }
    }

    fn on_renderer_ready(&mut self) {
        self.phase = Phase::Ready;
    }

    fn on_in_page_recovery(&mut self) {
        if matches!(self.phase, Phase::Loading | Phase::Escalated) {
            self.phase = Phase::InPageRecovery;
        }
    }

    fn on_boot_timeout(&mut self, generation: u64, now: Instant) -> RecoveryAction {
        if self.generation != generation || self.phase != Phase::Loading {
            return RecoveryAction::Ignore;
        }
        self.next_recovery(now)
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    fn on_process_terminated(&mut self, now: Instant) -> RecoveryAction {
        self.next_recovery(now)
    }

    /// Closes the escalation prompt and applies the answer to the window as it
    /// is now, which may have changed while the prompt was up.
    fn on_prompt_answered(&mut self, reload: bool) -> PromptOutcome {
        self.prompt_open = false;
        match self.phase {
            Phase::Mounted | Phase::Ready => PromptOutcome::LeaveAlone,
            Phase::InPageRecovery | Phase::Foreign if !reload => PromptOutcome::LeaveAlone,
            _ => {
                self.generation += 1;
                self.phase = Phase::Loading;
                if reload {
                    self.reload_attempts.clear();
                    PromptOutcome::Reload(self.generation)
                } else {
                    PromptOutcome::Rearm(self.generation)
                }
            }
        }
    }

    fn should_dispatch_reload(&self, generation: u64) -> bool {
        self.generation == generation && self.phase == Phase::Loading
    }

    fn prune_window(&mut self, now: Instant) {
        while let Some(&oldest) = self.reload_attempts.front() {
            if now.saturating_duration_since(oldest) > RETRY_WINDOW {
                self.reload_attempts.pop_front();
            } else {
                break;
            }
        }
    }

    fn next_recovery(&mut self, now: Instant) -> RecoveryAction {
        if self.prompt_open {
            // The open prompt already covers this window. Recording that it is
            // broken again keeps a Reload answer from being skipped as though a
            // mount seen earlier behind the prompt had stuck.
            self.phase = Phase::Escalated;
            return RecoveryAction::Ignore;
        }
        self.prune_window(now);
        if self.reload_attempts.len() >= MAX_RELOADS_PER_WINDOW {
            self.prompt_open = true;
            self.phase = Phase::Escalated;
            return RecoveryAction::Escalate {
                attempts: self.reload_attempts.len(),
            };
        }
        self.reload_attempts.push_back(now);
        let attempt = self.reload_attempts.len();
        let delay = if attempt == 1 {
            FIRST_RELOAD_DELAY
        } else {
            SECOND_RELOAD_DELAY
        };
        self.generation += 1;
        self.phase = Phase::Loading;
        RecoveryAction::Reload {
            attempt,
            max_attempts: MAX_RELOADS_PER_WINDOW,
            delay,
            generation: self.generation,
        }
    }
}

type State = Mutex<WatchdogState>;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootTelemetryPayload {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    url: String,
}

/// Registers watchdog state and event listeners before backend initialization
/// so early renderer handshakes are never missed.
pub fn install(app: &tauri::AppHandle) {
    app.manage(State::default());

    let handle = app.clone();
    app.listen("lattice:boot-mounted", move |_| {
        if let Some(state) = handle.try_state::<State>() {
            state.lock().on_boot_mounted();
        }
    });

    let handle = app.clone();
    app.listen("lattice:renderer-ready", move |_| {
        if let Some(state) = handle.try_state::<State>() {
            state.lock().on_renderer_ready();
        }
    });

    app.listen("lattice:boot-auto-retry", move |event| {
        if let Ok(report) = serde_json::from_str::<BootTelemetryPayload>(event.payload()) {
            tracing::warn!(
                kind = %report.kind,
                url = %report.url,
                detail = %report.detail,
                "Pre-mount supervisor scheduled automatic transient reload"
            );
        } else {
            tracing::warn!("Pre-mount supervisor scheduled automatic transient reload");
        }
    });

    let handle = app.clone();
    app.listen("lattice:boot-recovery", move |event| {
        if let Some(state) = handle.try_state::<State>() {
            state.lock().on_in_page_recovery();
        }
        if let Ok(report) = serde_json::from_str::<BootTelemetryPayload>(event.payload()) {
            tracing::error!(
                kind = %report.kind,
                url = %report.url,
                detail = %report.detail,
                "Pre-mount supervisor rendered interactive recovery screen"
            );
        } else {
            tracing::error!("Pre-mount supervisor rendered interactive recovery screen");
        }
    });
}

/// Arms the boot watchdog for the initial window load once backend startup has
/// finished, ensuring slow first-run database migrations never count against
/// the renderer's mount deadline.
pub fn start(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let generation = state.lock().current_loading_generation();
    if let Some(generation) = generation {
        arm_boot_watchdog(app, generation);
    }
}

/// Hooked into `tauri::Builder::on_page_load`.
pub fn on_page_load(webview: &tauri::Webview, payload: &PageLoadPayload<'_>) {
    if webview.label() != MAIN_WEBVIEW_LABEL || payload.event() != PageLoadEvent::Started {
        return;
    }
    let app = webview.app_handle();
    renderer_shutdown::mark_unready(app);
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let dev_url = if tauri::is_dev() {
        app.config().build.dev_url.as_ref()
    } else {
        None
    };
    let url = payload.url();
    if !is_app_page(url, dev_url) {
        state.lock().on_foreign_page();
        // Host only: paths and queries can carry private data.
        tracing::info!(
            host = url.host_str().unwrap_or(""),
            "Main webview left the app; boot deadline not applied"
        );
        return;
    }
    let generation = state.lock().on_page_load_started();
    arm_boot_watchdog(app, generation);
}

/// Hooked into `tauri::Builder::on_web_content_process_terminate` on macOS.
///
/// Reloads are dispatched from a `BackgroundTasks` worker rather than inline on
/// the AppKit main thread so `tauri-runtime-wry` posts `WebviewMessage::Reload`
/// via `TaoEventLoopProxy` instead of re-entering `windows.borrow()` inside a
/// `WKNavigationDelegate` callback.
#[cfg(target_os = "macos")]
pub fn on_web_content_process_terminate(webview: &tauri::Webview) {
    if webview.label() != MAIN_WEBVIEW_LABEL {
        tracing::warn!(
            label = webview.label(),
            "Auxiliary webview WebContent process terminated"
        );
        return;
    }
    let app = webview.app_handle();
    renderer_shutdown::mark_unready(app);
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let action = state.lock().on_process_terminated(Instant::now());
    execute_action(app, action, "WKWebView WebContent process terminated");
}

fn spawn_tracked<F>(tasks: &BackgroundTasks, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        tasks.spawn(future);
    } else {
        let runtime = tauri::async_runtime::handle();
        tasks.spawn_on(runtime.inner(), future);
    }
}

fn arm_boot_watchdog(app: &tauri::AppHandle, generation: u64) {
    let Some(deadline) = boot_deadline() else {
        return;
    };
    let Some(tasks) = app.try_state::<Arc<BackgroundTasks>>() else {
        return;
    };
    let cancel = tasks.token();
    let handle = app.clone();
    spawn_tracked(&tasks, async move {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(deadline) => {}
        }
        let Some(state) = handle.try_state::<State>() else {
            return;
        };
        let action = state.lock().on_boot_timeout(generation, Instant::now());
        execute_action(
            &handle,
            action,
            "Main webview did not finish booting within host deadline",
        );
    });
}

fn execute_action(app: &tauri::AppHandle, action: RecoveryAction, reason: &'static str) {
    match action {
        RecoveryAction::Ignore => {}
        RecoveryAction::Reload {
            attempt,
            max_attempts,
            delay,
            generation,
        } => {
            tracing::warn!(
                attempt,
                max_attempts,
                delay_ms = delay.as_millis() as u64,
                generation,
                reason,
                "Recovering main webview via bounded host reload"
            );
            schedule_reload(app, delay, generation, reason);
        }
        RecoveryAction::Escalate { attempts } => {
            tracing::error!(
                attempts,
                reason,
                "Main webview recovery budget exhausted; presenting native recovery prompt"
            );
            show_escalation_dialog(app);
        }
    }
}

fn schedule_reload(app: &tauri::AppHandle, delay: Duration, generation: u64, reason: &'static str) {
    let Some(tasks) = app.try_state::<Arc<BackgroundTasks>>() else {
        return;
    };
    let cancel = tasks.token();
    let handle = app.clone();
    spawn_tracked(&tasks, async move {
        if !delay.is_zero() {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return,
                () = tokio::time::sleep(delay) => {}
            }
        }
        let should_reload = handle
            .try_state::<State>()
            .is_some_and(|state| state.lock().should_dispatch_reload(generation));
        if !should_reload {
            return;
        }
        renderer_shutdown::mark_unready(&handle);
        let Some(window) = handle.get_webview_window(MAIN_WEBVIEW_LABEL) else {
            return;
        };
        if let Err(error) = window.reload() {
            tracing::error!(%error, reason, "Failed to reload main webview");
            // Still arm the deadline so `Phase::Loading` does not stay stranded
            // if the reload call itself errored before navigation started.
        }
        // Arm a deadline for `generation` as well: if the target server refuses
        // or hangs before `didCommitNavigation`, `PageLoadEvent::Started` will
        // not fire, so this timer ensures a hung reload still escalates cleanly.
        arm_boot_watchdog(&handle, generation);
    });
}

fn show_escalation_dialog(app: &tauri::AppHandle) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let handle = app.clone();
    app.dialog()
        .message(
            "The window stopped responding, and reloading it automatically didn't help. Reload it again?",
        )
        .title("Lattice isn't responding")
        .kind(MessageDialogKind::Error)
        // "Wait" is deliberate: if the window recovers while the dialog is up,
        // a non-destructive choice must exist.
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Reload".to_string(),
            "Wait".to_string(),
        ))
        .show(move |reload_clicked| {
            let Some(state) = handle.try_state::<State>() else {
                return;
            };
            let outcome = state.lock().on_prompt_answered(reload_clicked);
            match outcome {
                PromptOutcome::LeaveAlone => {
                    if reload_clicked {
                        tracing::info!(
                            "Reload skipped: the window recovered while the prompt was open"
                        );
                    }
                }
                PromptOutcome::Reload(generation) => {
                    tracing::info!(
                        generation,
                        "User requested manual webview reload from native recovery dialog"
                    );
                    schedule_reload(
                        &handle,
                        Duration::ZERO,
                        generation,
                        "Manual reload from recovery dialog",
                    );
                }
                PromptOutcome::Rearm(generation) => {
                    tracing::info!(
                        generation,
                        "User chose to wait; the window gets a fresh boot deadline"
                    );
                    arm_boot_watchdog(&handle, generation);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_mount_and_renderer_ready_disarm_pending_boot_timeout() {
        let start = Instant::now();
        let mut mounted = WatchdogState::new();
        let initial_gen = mounted.current_loading_generation().unwrap();
        mounted.on_boot_mounted();
        assert_eq!(mounted.current_loading_generation(), None);
        assert_eq!(
            mounted.on_boot_timeout(initial_gen, start + BOOT_TIMEOUT),
            RecoveryAction::Ignore
        );

        let mut ready = WatchdogState::new();
        let ready_gen = ready.current_loading_generation().unwrap();
        ready.on_renderer_ready();
        assert_eq!(
            ready.on_boot_timeout(ready_gen, start + BOOT_TIMEOUT),
            RecoveryAction::Ignore
        );
    }

    #[test]
    fn in_page_recovery_disarms_host_boot_timeout_without_masking_ready() {
        let start = Instant::now();
        let mut state = WatchdogState::new();
        let gen = state.current_loading_generation().unwrap();
        state.on_in_page_recovery();
        assert_eq!(
            state.on_boot_timeout(gen, start + BOOT_TIMEOUT),
            RecoveryAction::Ignore
        );
    }

    #[test]
    fn stale_generation_timeout_is_ignored_after_navigation() {
        let start = Instant::now();
        let mut state = WatchdogState::new();
        let first_gen = state.current_loading_generation().unwrap();
        let second_gen = state.on_page_load_started();
        assert_ne!(first_gen, second_gen);

        assert_eq!(
            state.on_boot_timeout(first_gen, start + BOOT_TIMEOUT),
            RecoveryAction::Ignore
        );
    }

    #[test]
    fn boot_timeouts_reload_with_backoff_then_escalate() {
        let t0 = Instant::now();
        let mut state = WatchdogState::new();
        let gen1 = state.current_loading_generation().unwrap();

        let t1 = t0 + BOOT_TIMEOUT;
        let gen2 = match state.on_boot_timeout(gen1, t1) {
            RecoveryAction::Reload {
                attempt: 1,
                max_attempts: 2,
                delay,
                generation,
            } => {
                assert_eq!(delay, FIRST_RELOAD_DELAY);
                assert!(state.should_dispatch_reload(generation));
                generation
            }
            other => panic!("expected first reload, got {other:?}"),
        };

        let t2 = t1 + BOOT_TIMEOUT;
        let gen3 = match state.on_boot_timeout(gen2, t2) {
            RecoveryAction::Reload {
                attempt: 2,
                max_attempts: 2,
                delay,
                generation,
            } => {
                assert_eq!(delay, SECOND_RELOAD_DELAY);
                generation
            }
            other => panic!("expected second reload, got {other:?}"),
        };

        let t3 = t2 + BOOT_TIMEOUT;
        assert_eq!(
            state.on_boot_timeout(gen3, t3),
            RecoveryAction::Escalate { attempts: 2 }
        );
        // While the prompt is open, duplicate termination events do not stack dialogs.
        assert_eq!(state.on_process_terminated(t3), RecoveryAction::Ignore);
    }

    #[test]
    fn process_crash_loop_after_mount_still_respects_sliding_window_budget() {
        let t0 = Instant::now();
        let mut state = WatchdogState::new();
        state.on_boot_mounted();
        state.on_renderer_ready();

        // First crash at t0 + 1s reloads.
        let action1 = state.on_process_terminated(t0 + Duration::from_secs(1));
        assert!(matches!(
            action1,
            RecoveryAction::Reload {
                attempt: 1,
                max_attempts: 2,
                ..
            }
        ));

        // Even if the reload mounts briefly, a second crash 2s later counts
        // against the same 60s window.
        state.on_boot_mounted();
        let action2 = state.on_process_terminated(t0 + Duration::from_secs(3));
        assert!(matches!(
            action2,
            RecoveryAction::Reload {
                attempt: 2,
                max_attempts: 2,
                ..
            }
        ));

        state.on_boot_mounted();
        let action3 = state.on_process_terminated(t0 + Duration::from_secs(5));
        assert_eq!(action3, RecoveryAction::Escalate { attempts: 2 });
    }

    #[test]
    fn sliding_window_expires_and_manual_retry_resets_budget() {
        let t0 = Instant::now();
        let mut state = WatchdogState::new();

        let _ = state.on_process_terminated(t0);
        let _ = state.on_process_terminated(t0 + Duration::from_secs(2));
        assert_eq!(
            state.on_process_terminated(t0 + Duration::from_secs(4)),
            RecoveryAction::Escalate { attempts: 2 }
        );

        // User clicks "Reload" in native escalation dialog: budget resets immediately.
        let PromptOutcome::Reload(manual_gen) = state.on_prompt_answered(true) else {
            panic!("a Reload answer on a broken window must reload it");
        };
        assert!(state.should_dispatch_reload(manual_gen));
        state.on_renderer_ready();
        assert!(!state.should_dispatch_reload(manual_gen));

        // Crash happens, then >60s pass cleanly: sliding window expires.
        let _ = state.on_process_terminated(t0 + Duration::from_secs(10));
        state.on_renderer_ready();
        let fresh = state.on_process_terminated(
            t0 + Duration::from_secs(10) + RETRY_WINDOW + Duration::from_secs(1),
        );
        assert!(matches!(
            fresh,
            RecoveryAction::Reload {
                attempt: 1,
                max_attempts: 2,
                ..
            }
        ));
    }

    fn escalated(t0: Instant) -> WatchdogState {
        let mut state = WatchdogState::new();
        let _ = state.on_process_terminated(t0);
        let _ = state.on_process_terminated(t0 + Duration::from_secs(1));
        assert_eq!(
            state.on_process_terminated(t0 + Duration::from_secs(2)),
            RecoveryAction::Escalate { attempts: 2 }
        );
        state
    }

    #[test]
    fn prompts_never_stack() {
        let t0 = Instant::now();
        let mut state = escalated(t0);
        let t = t0 + Duration::from_secs(3);

        assert_eq!(state.on_process_terminated(t), RecoveryAction::Ignore);
        let generation = state.on_page_load_started();
        assert_eq!(state.on_boot_timeout(generation, t), RecoveryAction::Ignore);

        assert!(matches!(
            state.on_prompt_answered(true),
            PromptOutcome::Reload(_)
        ));
        assert!(matches!(
            state.on_process_terminated(t),
            RecoveryAction::Reload { attempt: 1, .. }
        ));
    }

    #[test]
    fn reload_answer_leaves_a_recovered_window_alone() {
        let t0 = Instant::now();
        let mut state = escalated(t0);
        state.on_boot_mounted();

        assert_eq!(state.on_prompt_answered(true), PromptOutcome::LeaveAlone);
        assert!(!state.prompt_open);
        // The prompt no longer swallows failures, and mounting doesn't refund the
        // budget: a third crash inside the window is still a crash loop.
        assert_eq!(
            state.on_process_terminated(t0 + Duration::from_secs(3)),
            RecoveryAction::Escalate { attempts: 2 }
        );
    }

    #[test]
    fn a_crash_behind_the_prompt_is_not_mistaken_for_recovery() {
        let t0 = Instant::now();
        let mut state = escalated(t0);
        state.on_boot_mounted();
        assert_eq!(
            state.on_process_terminated(t0 + Duration::from_secs(3)),
            RecoveryAction::Ignore
        );
        assert!(matches!(
            state.on_prompt_answered(true),
            PromptOutcome::Reload(_)
        ));
    }

    #[test]
    fn wait_rearms_a_still_broken_window_instead_of_stranding_it() {
        let t0 = Instant::now();
        let mut state = escalated(t0);
        let PromptOutcome::Rearm(generation) = state.on_prompt_answered(false) else {
            panic!("Wait on a broken window must arm a fresh deadline");
        };
        assert!(!state.prompt_open);
        // Still blank when the new deadline lapses inside the retry window, so
        // the user is asked again rather than left with a dead window.
        assert_eq!(
            state.on_boot_timeout(generation, t0 + Duration::from_secs(3) + BOOT_TIMEOUT),
            RecoveryAction::Escalate { attempts: 2 }
        );
    }

    #[test]
    fn wait_leaves_a_window_that_came_back_alone() {
        let mut state = escalated(Instant::now());
        state.on_boot_mounted();
        assert_eq!(state.on_prompt_answered(false), PromptOutcome::LeaveAlone);
        assert!(!state.prompt_open);
        assert_eq!(state.current_loading_generation(), None);
    }

    #[test]
    fn foreign_page_invalidates_timers_but_crashes_still_reload() {
        let t0 = Instant::now();
        let mut state = WatchdogState::new();
        let generation = state.on_page_load_started();
        state.on_foreign_page();
        assert_eq!(
            state.on_boot_timeout(generation, t0 + BOOT_TIMEOUT),
            RecoveryAction::Ignore
        );
        assert_eq!(state.current_loading_generation(), None);
        assert!(matches!(
            state.on_process_terminated(t0 + BOOT_TIMEOUT),
            RecoveryAction::Reload { attempt: 1, .. }
        ));
    }

    #[test]
    fn app_pages_are_told_apart_from_foreign_ones() {
        let u = |s: &str| Url::parse(s).unwrap();
        let dev = u("http://localhost:5173");
        let cases = [
            ("tauri://localhost/", None, true),
            ("http://tauri.localhost/", None, true),
            ("https://tauri.localhost/x", None, true),
            ("http://localhost:5173/#/notes", Some(&dev), true),
            ("http://localhost:5174/", Some(&dev), false),
            ("https://example.com/", None, false),
            ("http://localhost:5173/", None, false),
        ];
        for (url, dev_url, expected) in cases {
            assert_eq!(is_app_page(&u(url), dev_url), expected, "{url}");
        }
    }
}
