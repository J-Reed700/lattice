//! Source finding, executed: the `SourceFinding` service over the registered
//! finders (design 2026-10-18 §3.3–§3.7).
//!
//! - `service`: the port implementation; plans a need with the core router,
//!   runs the plan, fuses the leads and reports every finder's outcome.
//! - `runner`: waves, pages, the provider-page cache, waits for a host's
//!   `Retry-After`, and transport retries, all bounded by the caller's budget.
//! - `health`: one breaker and its counters per finder.
//! - `cache`: provider pages, so a later page of a search is a slice of the
//!   pool rather than a re-fetch of every provider's first page.

mod cache;
mod health;
mod runner;
mod service;

pub(crate) use service::{Search, SourcingService};

#[cfg(test)]
mod tests;
