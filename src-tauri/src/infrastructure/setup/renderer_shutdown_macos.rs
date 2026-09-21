//! Tao 0.34 does not implement applicationShouldTerminate:. Its native Quit
//! menu and Dock Quit call AppKit's terminate: directly, bypassing Tauri's
//! ExitRequested event. Route that request back through the existing save gate.
//! https://github.com/tauri-apps/tauri/issues/12978
//!
//! Keep Tao's delegate and all of its existing methods. Add only the missing
//! public AppKit delegate method, and fail startup if an upgraded runtime
//! already owns it rather than silently overriding another implementation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use objc::runtime::{class_addMethod, Class, Imp, Object, Sel, NO, YES};
use objc::Message;

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
static PENDING: AtomicBool = AtomicBool::new(false);

// NSUInteger on both supported 64-bit macOS architectures. Returning
// NSTerminateLater (2) keeps AppKit from destroying the webview. AppHandle::exit
// queues Tauri's ExitRequested; after the renderer acknowledges its saves,
// the native request is answered after graceful shutdown. This also allows
// logout/system shutdown to wait for saves instead of always cancelling it.
extern "C" fn should_terminate(_: &Object, _: Sel, _: *mut Object) -> usize {
    // No Rust unwind may cross the Objective-C callback boundary.
    let _ = std::panic::catch_unwind(|| {
        if let Some(app) = APP.get() {
            PENDING.store(true, Ordering::SeqCst);
            app.exit(0);
        }
    });
    2
}

/// Finish a deferred AppKit request, or cancel it when saving/cancellation fails.
pub(super) fn reply(app: &tauri::AppHandle, saved: bool) {
    if !PENDING.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Err(error) = app.run_on_main_thread(move || {
        // SAFETY: this closure runs on the AppKit main thread. NSApplication is
        // process-owned; the public reply method takes one Objective-C BOOL.
        #[allow(unsafe_code)]
        unsafe {
            if let Some(class) = Class::get("NSApplication") {
                let application: Result<*mut Object, _> =
                    class.send_message(Sel::register("sharedApplication"), ());
                if let Ok(application) = application {
                    if let Some(application) = application.as_ref() {
                        let result: Result<(), _> = application.send_message(
                            Sel::register("replyToApplicationShouldTerminate:"),
                            (if saved { YES } else { NO },),
                        );
                        if let Err(error) = result {
                            tracing::error!(%error, "Could not reply to AppKit termination");
                        }
                    }
                }
            }
        }
    }) {
        tracing::error!(%error, "Could not schedule AppKit termination reply");
    }
}

/// Called from Tauri's setup hook on the AppKit main thread.
pub(super) fn install(app: &tauri::AppHandle) -> Result<(), String> {
    APP.set(app.clone())
        .map_err(|_| "Native quit handler was already installed".to_string())?;

    // SAFETY: setup runs on the main thread after NSApplication and Tao's
    // delegate exist. Both objects are runtime-owned for the application's
    // lifetime. We neither replace nor release them. The callback's ABI and
    // encoding are NSUInteger (self: id, selector: SEL, sender: NSApplication*).
    #[allow(unsafe_code)]
    unsafe {
        let application_class = Class::get("NSApplication")
            .ok_or("NSApplication is unavailable; cannot install safe quit")?;
        let application: *mut Object = application_class
            .send_message(Sel::register("sharedApplication"), ())
            .map_err(|error| error.to_string())?;
        let application = application
            .as_ref()
            .ok_or("NSApplication is not initialized; cannot install safe quit")?;
        let delegate: *mut Object = application
            .send_message(Sel::register("delegate"), ())
            .map_err(|error| error.to_string())?;
        let Some(delegate) = delegate.as_ref() else {
            return Err("AppKit delegate is unavailable; cannot install safe quit".into());
        };
        let class = delegate.class();
        let selector = Sel::register("applicationShouldTerminate:");
        if class.instance_method(selector).is_some() {
            return Err(
                "AppKit delegate already handles termination; review the native save integration"
                    .into(),
            );
        }
        let implementation = std::mem::transmute::<
            extern "C" fn(&Object, Sel, *mut Object) -> usize,
            Imp,
        >(should_terminate);
        if class_addMethod(
            class as *const Class as *mut Class,
            selector,
            implementation,
            c"Q@:@".as_ptr(),
        ) == NO
        {
            return Err("Could not install the native quit save handler".into());
        }
    }
    Ok(())
}
