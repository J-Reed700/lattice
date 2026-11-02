//! The microphone in the app's own window.
//!
//! Left alone, WebView2 asks with a browser prompt and keeps a "Block" in its
//! profile, with nowhere in the app to undo it. Pressing the mic is the asking
//! here; Windows' privacy setting still has the last word.

use url::Url;

/// Whether a permission request comes from the app's own pages rather than a
/// page shown inside them. Debug builds load from the dev server.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn is_own_page(uri: &str, dev_server: bool) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return false;
    };
    match url.host_str() {
        Some("tauri.localhost") => true,
        Some("localhost") => dev_server && url.scheme() == "http",
        _ => false,
    }
}

/// Lets the main window's own pages open the microphone.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn allow_in_main_window(app: &tauri::App) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    };
    use webview2_com::{take_pwstr, PermissionRequestedEventHandler};

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let reached = window.with_webview(|webview| {
        let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            // SAFETY: `args` is the live event argument for the duration of this callback.
            unsafe { args.PermissionKind(&mut kind)? };
            if kind != COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                return Ok(());
            }
            let mut uri = Default::default();
            // SAFETY: as above, and `take_pwstr` frees the string it is given.
            let uri = unsafe {
                args.Uri(&mut uri)?;
                take_pwstr(uri)
            };
            if is_own_page(&uri, cfg!(debug_assertions)) {
                // SAFETY: as above.
                unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)? };
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: the controller and its webview live as long as the window.
        let added = unsafe {
            webview
                .controller()
                .CoreWebView2()
                .and_then(|core| core.add_PermissionRequested(&handler, &mut token))
        };
        if let Err(error) = added {
            tracing::warn!(%error, "could not set up the microphone permission");
        }
    });
    if let Err(error) = reached {
        tracing::warn!(%error, "could not set up the microphone permission");
    }
}

#[cfg(test)]
mod tests {
    use super::is_own_page;

    #[test]
    fn the_packaged_app_is_its_own_page() {
        assert!(is_own_page("http://tauri.localhost/", false));
        assert!(is_own_page(
            "http://tauri.localhost/chat?id=1#message",
            false
        ));
    }

    #[test]
    fn the_dev_server_counts_only_in_a_debug_build() {
        assert!(is_own_page("http://localhost:5173/", true));
        assert!(!is_own_page("http://localhost:5173/", false));
    }

    #[test]
    fn nothing_else_gets_the_microphone() {
        for uri in [
            "http://lattice-snapshot.localhost/page",
            "https://tauri.localhost.example.com/",
            "https://example.com/",
            "file:///C:/page.html",
            "not a url",
        ] {
            assert!(!is_own_page(uri, true), "{uri}");
        }
    }
}
