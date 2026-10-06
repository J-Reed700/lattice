//! Keep explicit user work eligible to run while its macOS window is inactive.
//! The assertion lasts only for active work, not queueing or retry backoff, and
//! allows normal display and system sleep. Other platforms need no assertion.

pub struct UserActivity {
    #[cfg(target_os = "macos")]
    _activity: Option<macos::Activity>,
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
        #[cfg(not(target_os = "macos"))]
        {
            let _ = reason;
            Self {}
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

    // Public Foundation NSActivityOptions. UserInitiated with the idle-system-
    // sleep bit removed: prevent App Nap, without overriding sleep preferences.
    const USER_INITIATED_ALLOWING_IDLE_SYSTEM_SLEEP: u64 = 0x00FF_FFFF & !(1 << 20);

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
        pub(super) fn begin(reason: &str) -> Result<Self, String> {
            super::super::with_autorelease_pool(|| {
                // SAFETY: Foundation is linked by the desktop runtime. These
                // public methods take NSUInteger, NSString*, and no other state.
                // Every pointer is checked and retained before the pool drains.
                #[allow(unsafe_code)]
                unsafe {
                    let class =
                        Class::get("NSProcessInfo").ok_or("NSProcessInfo is unavailable")?;
                    let process: *mut Object = class
                        .send_message(Sel::register("processInfo"), ())
                        .map_err(|e| e.to_string())?;
                    let process_ref = process.as_ref().ok_or("NSProcessInfo returned nil")?;
                    let string_class = Class::get("NSString").ok_or("NSString is unavailable")?;
                    let reason = std::ffi::CString::new(reason).map_err(|e| e.to_string())?;
                    let text: *mut Object = string_class
                        .send_message(Sel::register("stringWithUTF8String:"), (reason.as_ptr(),))
                        .map_err(|e| e.to_string())?;
                    if text.is_null() {
                        return Err("Could not construct activity reason".into());
                    }
                    let token: *mut Object = process_ref
                        .send_message(
                            Sel::register("beginActivityWithOptions:reason:"),
                            (USER_INITIATED_ALLOWING_IDLE_SYSTEM_SLEEP, text),
                        )
                        .map_err(|e| e.to_string())?;
                    if token.is_null() {
                        return Err("Could not begin user activity".into());
                    }
                    tracing::info!("Protected active user work from App Nap");
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::expect_used)]
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
