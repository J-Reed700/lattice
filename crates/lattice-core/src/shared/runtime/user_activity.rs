//! Keep explicit user work running while its window is inactive and the PC is
//! idle. While a guard is held, macOS skips App Nap and idle system sleep and
//! Windows holds a system-required power request. The display may still sleep,
//! and lid-close or a chosen Sleep still works. The hold lasts only for active
//! work, not queueing or retry backoff. Other platforms hold nothing.

pub struct UserActivity {
    #[cfg(target_os = "macos")]
    _activity: Option<macos::Activity>,
    #[cfg(windows)]
    _power: Option<windows_power::Request>,
}

impl UserActivity {
    pub fn begin(reason: &str) -> Self {
        #[cfg(target_os = "macos")]
        {
            let activity = match macos::Activity::begin(reason) {
                Ok(activity) => Some(activity),
                Err(error) => {
                    tracing::warn!(%error, "Could not protect active user work from App Nap");
                    None
                }
            };
            Self {
                _activity: activity,
            }
        }
        #[cfg(windows)]
        {
            let power = match windows_power::Request::begin(reason) {
                Ok(request) => Some(request),
                Err(error) => {
                    tracing::warn!(%error, "Could not keep the PC awake for active user work");
                    None
                }
            };
            Self { _power: power }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = reason;
            Self {}
        }
    }
}

#[cfg(windows)]
mod windows_power {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::{
                Power::{
                    PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired,
                    PowerSetRequest,
                },
                Threading::{
                    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
                },
            },
        },
    };

    use crate::shared::error::AppError;

    // POWER_REQUEST_CONTEXT_VERSION in winnt.h.
    const CONTEXT_VERSION: u32 = 0;

    // A power request object is per process and not tied to a thread, unlike
    // SetThreadExecutionState, so it survives tokio moving the task.
    pub(super) struct Request {
        handle: Option<HANDLE>,
        set: bool,
    }

    fn win(error: windows::core::Error) -> AppError {
        AppError::ServiceNotAvailable(error.to_string())
    }

    impl Request {
        pub(super) fn begin(reason: &str) -> Result<Self, AppError> {
            let mut text: Vec<u16> = reason
                .encode_utf16()
                .filter(|unit| *unit != 0)
                .chain(std::iter::once(0))
                .collect();
            let context = REASON_CONTEXT {
                Version: CONTEXT_VERSION,
                Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                Reason: REASON_CONTEXT_0 {
                    SimpleReasonString: PWSTR(text.as_mut_ptr()),
                },
            };
            // SAFETY: `context` and the NUL-terminated `text` it points to
            // outlive the call, which only reads them.
            #[allow(unsafe_code)]
            let handle = unsafe { PowerCreateRequest(&context) }.map_err(win)?;
            // From here a failure drops `request`, which closes the handle.
            let mut request = Self {
                handle: Some(handle),
                set: false,
            };
            // SAFETY: `handle` is a live power request handle.
            #[allow(unsafe_code)]
            let set = unsafe { PowerSetRequest(handle, PowerRequestSystemRequired) };
            set.map_err(win)?;
            request.set = true;
            tracing::info!("Holding the PC awake for active user work");
            Ok(request)
        }

        #[cfg(test)]
        pub(super) fn is_set(&self) -> bool {
            self.set
        }

        pub(super) fn release(&mut self) -> Result<(), AppError> {
            let Some(handle) = self.handle.take() else {
                return Ok(());
            };
            let mut outcome = Ok(());
            if std::mem::take(&mut self.set) {
                // SAFETY: `handle` is a live power request handle.
                #[allow(unsafe_code)]
                let cleared = unsafe { PowerClearRequest(handle, PowerRequestSystemRequired) };
                outcome = cleared.map_err(win);
            }
            // SAFETY: the handle came from PowerCreateRequest and is closed
            // only here, after which `handle` is None.
            #[allow(unsafe_code)]
            let closed = unsafe { CloseHandle(handle) }.map_err(win);
            outcome.and(closed)
        }
    }

    impl Drop for Request {
        fn drop(&mut self) {
            if let Err(error) = self.release() {
                tracing::warn!(%error, "Could not release the PC-awake request");
            } else {
                tracing::info!("Released the PC-awake request after user work");
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc::{
        rc::StrongPtr,
        runtime::{Class, Object, Sel},
        Message,
    };

    use crate::shared::error::AppError;

    // Public Foundation NSActivityOptions: UserInitiated, which includes
    // NSActivityIdleSystemSleepDisabled (1 << 20). The display-sleep bit is
    // not set, so the screen may still turn off.
    pub(super) const ACTIVITY_OPTIONS: u64 = 0x00FF_FFFF;

    pub(super) struct Activity {
        process: StrongPtr,
        pub(super) token: StrongPtr,
    }

    // SAFETY: NSProcessInfo activities explicitly support beginning before
    // dispatch and ending on the worker queue. Both objects are retained; this
    // guard has one owner, never exposes them, and ends the activity exactly once.
    #[allow(unsafe_code)]
    unsafe impl Send for Activity {}

    impl Activity {
        pub(super) fn begin(reason: &str) -> Result<Self, AppError> {
            let unavailable = |what: &str| AppError::ServiceNotAvailable(what.to_string());
            super::super::with_autorelease_pool(|| {
                // SAFETY: Foundation is linked by the desktop runtime. These
                // public methods take NSUInteger, NSString*, and no other state.
                // Every pointer is checked and retained before the pool drains.
                #[allow(unsafe_code)]
                unsafe {
                    let class = Class::get("NSProcessInfo")
                        .ok_or_else(|| unavailable("NSProcessInfo is unavailable"))?;
                    let process: *mut Object = class
                        .send_message(Sel::register("processInfo"), ())
                        .map_err(|e| AppError::InternalError(e.to_string()))?;
                    let process_ref = process
                        .as_ref()
                        .ok_or_else(|| unavailable("NSProcessInfo returned nil"))?;
                    let string_class = Class::get("NSString")
                        .ok_or_else(|| unavailable("NSString is unavailable"))?;
                    let reason = std::ffi::CString::new(reason)
                        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
                    let text: *mut Object = string_class
                        .send_message(Sel::register("stringWithUTF8String:"), (reason.as_ptr(),))
                        .map_err(|e| AppError::InternalError(e.to_string()))?;
                    if text.is_null() {
                        return Err(AppError::InternalError(
                            "Could not construct activity reason".to_string(),
                        ));
                    }
                    let token: *mut Object = process_ref
                        .send_message(
                            Sel::register("beginActivityWithOptions:reason:"),
                            (ACTIVITY_OPTIONS, text),
                        )
                        .map_err(|e| AppError::InternalError(e.to_string()))?;
                    if token.is_null() {
                        return Err(AppError::InternalError(
                            "Could not begin user activity".to_string(),
                        ));
                    }
                    tracing::info!("Protected active user work from App Nap and idle sleep");
                    Ok(Self {
                        process: StrongPtr::retain(process),
                        token: StrongPtr::retain(token),
                    })
                }
            })
        }
    }

    impl Drop for Activity {
        fn drop(&mut self) {
            super::super::with_autorelease_pool(|| {
                // SAFETY: process/token were retained in begin. Foundation
                // permits ending on another worker thread. This guard cannot be
                // cloned, and its StrongPtrs release after endActivity returns.
                #[allow(unsafe_code)]
                unsafe {
                    if let Some(process) = (*self.process).as_ref() {
                        let ended: Result<(), _> =
                            process.send_message(Sel::register("endActivity:"), (*self.token,));
                        if let Err(error) = ended {
                            tracing::warn!(%error, "Could not end user activity");
                        } else {
                            tracing::info!("Released App Nap protection after user work");
                        }
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn the_guard_can_be_held_across_awaits() {
        assert_send::<UserActivity>();
    }

    #[test]
    fn beginning_and_dropping_never_panics_even_with_an_odd_reason() {
        {
            let _guard = UserActivity::begin("Preparing learning material");
        }
        {
            let _guard = UserActivity::begin("");
        }
        {
            let _guard = UserActivity::begin("embedded\0nul and \u{1F600} non-BMP");
        }
        let _first = UserActivity::begin("first");
        let _second = UserActivity::begin("second");
    }

    #[cfg(windows)]
    #[test]
    fn windows_guard_sets_the_system_request_and_clears_it_on_release() {
        let mut request = windows_power::Request::begin("Testing power request")
            .expect("A power request must be creatable on Windows");
        assert!(request.is_set());
        request
            .release()
            .expect("Clearing a set request must succeed");
        assert!(!request.is_set());
        request.release().expect("Releasing twice is a no-op");
    }

    #[cfg(windows)]
    #[test]
    fn windows_requests_are_independent_per_holder() {
        let mut first = windows_power::Request::begin("first").expect("first request");
        let mut second = windows_power::Request::begin("second").expect("second request");
        first.release().expect("release first");
        assert!(second.is_set());
        second.release().expect("release second");
    }

    #[cfg(windows)]
    #[test]
    fn windows_public_guard_holds_a_power_request() {
        let guard = UserActivity::begin("Testing public guard");
        assert!(
            guard._power.is_some(),
            "The guard must hold a power request"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_options_keep_the_system_awake_but_not_the_display() {
        const IDLE_SYSTEM_SLEEP_DISABLED: u64 = 1 << 20;
        const IDLE_DISPLAY_SLEEP_DISABLED: u64 = 1 << 40;
        let options = macos::ACTIVITY_OPTIONS;
        assert_ne!(options & IDLE_SYSTEM_SLEEP_DISABLED, 0);
        assert_eq!(options & IDLE_DISPLAY_SLEEP_DISABLED, 0);
        assert_ne!(options & 0xFF, 0, "User-initiated bits must stay set");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn native_activity_is_retained_across_awaits_and_released_on_cancellation() {
        let guard = UserActivity::begin("Testing background activity lifetime");
        let activity = guard
            ._activity
            .as_ref()
            .expect("A real Foundation activity must be available on macOS");
        let weak = activity.token.weak();
        let (ready, started) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _guard = guard;
            let _ = ready.send(());
            std::future::pending::<()>().await;
        });
        assert!(started.await.is_ok());
        assert!(!weak.load().is_null());
        task.abort();
        assert!(task.await.is_err());
        assert!(
            weak.load().is_null(),
            "Cancellation must release the native activity token"
        );
    }
}
