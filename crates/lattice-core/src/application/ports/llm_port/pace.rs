//! How fast a backend reads and writes, as its own reports on the app's
//! requests show it. Nothing here asks a server anything: a backend that
//! reports no timing leaves the pace unknown.

use serde::{Deserialize, Serialize};

/// What one request cost the backend, as it reported it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ServedTiming {
    /// Prompt tokens the backend evaluated for this request. Tokens it served
    /// from its cache are left out when it says how many those were: they
    /// cost no reading time.
    pub prompt_tokens: u64,
    /// Seconds it spent evaluating them.
    pub prompt_seconds: f64,
    /// Tokens it generated, reasoning included.
    pub output_tokens: u64,
    /// Seconds it spent generating them.
    pub output_seconds: f64,
}

/// A backend's reading and writing speed, each a moving average over the
/// requests whose timing it reported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    pub prompt_tokens_per_second: f64,
    pub output_tokens_per_second: f64,
}
