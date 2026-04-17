#[path = "modules/alignment.rs"]
pub mod alignment;
#[path = "modules/atomic_fs.rs"]
pub mod atomic_fs;
#[path = "modules/config.rs"]
pub mod config;
#[path = "modules/error_handler.rs"]
pub mod error_handler;
#[path = "modules/http_client.rs"]
pub mod http_client;
#[path = "modules/logger.rs"]
pub mod logger;
#[path = "modules/path.rs"]
pub mod path;
pub mod patterns;
#[path = "modules/progress_emitter.rs"]
pub mod progress_emitter;
#[path = "modules/retry.rs"]
pub mod retry;
#[path = "modules/stealth.rs"]
pub mod stealth;

pub use alignment::{
    bytes_to_f32_safe, bytes_to_f32_slice, bytes_to_f32_vec, bytes_to_slice, validate_alignment,
    AlignedF32Data,
};
pub use atomic_fs::AtomicFs;
pub use error_handler::{map_error_to_user_message, sanitize_error, SafeError};
pub use http_client::{reqwest_client_builder, should_disable_system_proxy};
pub use path::{path_to_string, validate_path};
pub use progress_emitter::ProgressEmitter;
pub use retry::{retry_async, retry_with_backoff, retry_with_jitter, RetryConfig};
pub use stealth::{
    browser_headers, flaresolverr, random_delay, random_profile, search_headers,
    stealth_client_builder, BrowserProfile,
};
