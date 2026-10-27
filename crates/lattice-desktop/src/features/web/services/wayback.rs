//! The Internet Archive's Wayback Machine: the page reader's last resort when
//! a page is gone or refuses every reader, and its dated reads (design §3.8).
//!
//! Three endpoints, each with its own breaker in a [`HealthBook`] (§3.6):
//!
//! - availability (`archive.org/wayback/available`): the one capture closest
//!   to a timestamp, or `{"archived_snapshots":{}}`;
//! - CDX (`web.archive.org/cdx/search/cdx`): asked only when availability has
//!   no capture that answered 200, for the 200 capture nearest the timestamp
//!   (`closest=` + `sort=closest`);
//! - replay (`web.archive.org/web/<timestamp>id_/<url>`): the capture's own
//!   bytes, without the Wayback toolbar.
//!
//! Every call carries the declared `Lattice/<version>` agent (§3.9) and goes
//! through the shared per-host pacer. Lattice never asks the archive to save
//! a page: Save Page Now publishes the URL.
//!
//! Neither index documents a rate (Appendix B, read 2026-10-18), and CDX
//! answered a first request with 429 and no `Retry-After`. So each endpoint
//! keeps the pacer's spacing, opens its breaker on the first refusal, and the
//! core breaker derives how long it stays open; a `Retry-After`, when the
//! archive sends one, is honoured in full.

use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::domain::sourcing::{
    unwrap_archive, Admission, Auth, Cost, Destination, FailureCause, FinderDescriptor,
    FinderHealth, FinderId, FinderOutcome, FinderResult, Identity, NeedKind, Privacy, RatePolicy,
    Reach, Role, SkipReason, SourcingPolicy, WaveSlot,
};
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::info;
use url::Url;

use super::finders::{retry_after, Fetched, FinderHttp};
use super::identity;
use super::page_cache::refusal_is_settled;
use super::sourcing::HealthBook;
use super::web::{read_bytes_bounded, MAX_PAGE_BODY_BYTES, MIN_SITE_REQUEST_INTERVAL};

/// The availability API, documented at `archive.org/help/wayback_api.php`.
const AVAILABILITY_API: &str = "https://archive.org/wayback/available";
/// The CDX server, documented in the `wayback-cdx-server` README.
const CDX_API: &str = "https://web.archive.org/cdx/search/cdx";
/// Where captures are replayed: `<root><timestamp>[id_]/<url>`.
const REPLAY_ROOT: &str = "https://web.archive.org/web/";

/// `id_` asks the replay for the capture's original bytes: no toolbar, no
/// rewritten links, so the page extractor reads what the site served.
const RAW_REPLAY_MODIFIER: &str = "id_";

/// The CDX rows asked for: the nearest capture only. A second row would be
/// another date, which the caller did not ask for.
const CDX_ROWS: &str = "1";

/// An index answer is one capture; this bound only stops a misbehaving
/// response, it never cuts an answer short.
const INDEX_BODY_BYTES: usize = 64 * 1024;

/// The capture timestamp format both indexes and the replay URLs use.
const CAPTURE_TIMESTAMP: &str = "%Y%m%d%H%M%S";

const JSON_ACCEPT: &str = "application/json";
const PAGE_ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

pub(crate) const AVAILABILITY_ID: FinderId = FinderId::from_static("wayback_available");
pub(crate) const CDX_ID: FinderId = FinderId::from_static("wayback_cdx");
pub(crate) const REPLAY_ID: FinderId = FinderId::from_static("wayback_replay");

const ARCHIVE_ROLES: &[Role] = &[Role {
    kind: NeedKind::Archive,
    wave: WaveSlot::First,
}];

/// One archive endpoint. No published rate, so the pacer's spacing; a
/// refusal trips at once because a client that keeps asking through 429s
/// is the one the archive blocks.
const fn endpoint(
    id: FinderId,
    name: &'static str,
    host: &'static str,
    source: &'static str,
) -> FinderDescriptor {
    FinderDescriptor {
        id,
        name,
        finds: ARCHIVE_ROLES,
        reaches: Reach::Archive,
        auth: Auth::None,
        privacy: Privacy::QueryLogged {
            by: "the Internet Archive",
        },
        cost: Cost::Free,
        rate: RatePolicy {
            min_interval: MIN_SITE_REQUEST_INTERVAL,
            quota: None,
            honours_retry_after: true,
            trip_on_first_refusal: true,
            source,
        },
        supports_scope: false,
        identity: Identity::DeclaredClient,
        sends_to: Destination::Host(host),
    }
}

pub(crate) const AVAILABILITY: FinderDescriptor = endpoint(
    AVAILABILITY_ID,
    "Internet Archive (availability)",
    "archive.org",
    "https://archive.org/help/wayback_api.php",
);
pub(crate) const CDX: FinderDescriptor = endpoint(
    CDX_ID,
    "Internet Archive (CDX)",
    "web.archive.org",
    "https://github.com/internetarchive/wayback/tree/master/wayback-cdx-server",
);
pub(crate) const REPLAY: FinderDescriptor = endpoint(
    REPLAY_ID,
    "Internet Archive (replay)",
    "web.archive.org",
    "https://archive.org/help/wayback_api.php",
);

/// Where the three endpoints live; fixture servers in tests.
#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    pub availability: String,
    pub cdx: String,
    /// Ends with `/`; a capture is `<replay><timestamp>/<url>`.
    pub replay: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            availability: AVAILABILITY_API.to_string(),
            cdx: CDX_API.to_string(),
            replay: REPLAY_ROOT.to_string(),
        }
    }
}

/// A capture the archive holds that answered 200 when it was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Capture {
    /// `YYYYMMDDhhmmss`, as the archive names it.
    pub timestamp: String,
    pub captured_at: DateTime<Utc>,
}

impl Capture {
    fn from_timestamp(timestamp: &str) -> Result<Self, FailureCause> {
        NaiveDateTime::parse_from_str(timestamp, CAPTURE_TIMESTAMP)
            .map(|at| Self {
                timestamp: timestamp.to_string(),
                captured_at: at.and_utc(),
            })
            .map_err(|error| {
                FailureCause::Parse(format!("unreadable capture timestamp {timestamp}: {error}"))
            })
    }
}

/// What the indexes said about a URL.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Lookup {
    Found(Capture),
    /// Both indexes answered (or the user turned one off) and neither holds
    /// a capture that answered 200.
    NothingArchived,
    /// An index failed, refused or is paused, so a copy may exist. The note
    /// names each endpoint's outcome.
    Unanswered(String),
}

/// A capture's bytes, as replayed.
#[derive(Debug)]
pub(crate) struct Replayed {
    /// The capture as the archive stores it: a page's markup, or a PDF's or
    /// text file's own bytes, read the way a live read reads them.
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    /// The capture actually served: the replay may redirect to a neighbour.
    pub captured_at: DateTime<Utc>,
    /// The capture's page as a person opens it, with the archive's banner.
    pub snapshot_url: String,
}

/// Why a replay produced nothing.
#[derive(Debug, PartialEq)]
pub(crate) enum ReplayMiss {
    /// The archive answered that this capture cannot be served, with a status
    /// the page cache treats as settled: no readable copy behind the entry.
    Gone(String),
    /// The replay failed or is paused; the copy may still be readable.
    Unanswered(String),
}

/// The Wayback client: the endpoints, the user's archive switch and one
/// breaker per endpoint.
pub(crate) struct Wayback {
    /// The client and the user's switches; Settings → Sources swaps it
    /// (`apply`), and a lookup holds the one it started with.
    config: parking_lot::RwLock<Arc<Config>>,
    endpoints: Endpoints,
    health: HealthBook,
}

/// The finders' HTTP (its contact is the user's) and the sourcing settings
/// the archive switch is read from.
struct Config {
    http: FinderHttp,
    settings: SourcingSettingsDto,
}

/// One endpoint call: its outcome and, when it answered, its value.
struct Call<T> {
    outcome: FinderOutcome,
    value: Option<T>,
}

impl Wayback {
    pub(crate) fn new(http: FinderHttp, settings: SourcingSettingsDto) -> Self {
        Self::with_endpoints(http, settings, Endpoints::default())
    }

    pub(crate) fn with_endpoints(
        http: FinderHttp,
        settings: SourcingSettingsDto,
        endpoints: Endpoints,
    ) -> Self {
        Self {
            config: parking_lot::RwLock::new(Arc::new(Config { http, settings })),
            endpoints,
            health: HealthBook::new(SourcingPolicy::default()),
        }
    }

    /// Takes the user's new sourcing settings (the archive switch, the
    /// endpoints' own switches) and the finders' HTTP carrying their contact.
    /// The endpoints' breakers carry over.
    pub(crate) fn apply(&self, http: FinderHttp, settings: SourcingSettingsDto) {
        *self.config.write() = Arc::new(Config { http, settings });
    }

    fn config(&self) -> Arc<Config> {
        Arc::clone(&self.config.read())
    }

    /// The three endpoints, as the settings panel lists them.
    pub(crate) fn descriptors() -> [FinderDescriptor; 3] {
        [AVAILABILITY, CDX, REPLAY]
    }

    /// Each endpoint's breaker health, gate aside.
    pub(crate) fn breaker_health(&self) -> Vec<FinderHealth> {
        let descriptors = Self::descriptors();
        let descriptors: Vec<&FinderDescriptor> = descriptors.iter().collect();
        self.health.breaker_health(&descriptors)
    }

    /// The user turned the archive fallback off (decision 4: on by default).
    pub(crate) fn is_off(&self) -> bool {
        !self.config().settings.archive_fallback
    }

    /// When `id`'s breaker lets it be called again, while it is open.
    #[cfg(test)]
    pub(crate) fn paused_until(&self, id: &FinderId) -> Option<DateTime<Utc>> {
        self.health.next_allowed_at(id)
    }

    /// The capture of `url` nearest `as_of`, or the latest when `None`:
    /// availability first (one request, no documented limit), CDX only when
    /// availability holds no capture that answered 200.
    pub(crate) async fn lookup(&self, url: &Url, as_of: Option<NaiveDate>) -> Lookup {
        let available = self
            .call(&AVAILABILITY, || self.ask_availability(url, as_of))
            .await;
        if let Some(capture) = available.value {
            return found(url, capture, &AVAILABILITY_ID);
        }
        let near = target_stamp(as_of, Utc::now());
        let cdx = self.call(&CDX, || self.ask_cdx(url, &near)).await;
        if let Some(capture) = cdx.value {
            return found(url, capture, &CDX_ID);
        }
        if settled_empty(&available.outcome) && settled_empty(&cdx.outcome) {
            Lookup::NothingArchived
        } else {
            Lookup::Unanswered(describe(&[available.outcome, cdx.outcome]))
        }
    }

    /// Read `capture` of `url` as the site served it.
    pub(crate) async fn replay(
        &self,
        url: &Url,
        capture: &Capture,
    ) -> Result<Replayed, ReplayMiss> {
        let raw = format!(
            "{}{}{RAW_REPLAY_MODIFIER}/{url}",
            self.endpoints.replay, capture.timestamp
        );
        let call = self.call(&REPLAY, || self.fetch_replay(&raw)).await;
        match call.value {
            Some((final_url, content_type, body)) => {
                // A replay redirects to the nearest capture it can serve; the
                // label names that one.
                let captured_at = Url::parse(&final_url)
                    .ok()
                    .and_then(|landed| unwrap_archive(&landed))
                    .and_then(|copy| copy.captured_at)
                    .unwrap_or(capture.captured_at);
                let served = captured_at.format(CAPTURE_TIMESTAMP);
                Ok(Replayed {
                    body,
                    content_type,
                    captured_at,
                    snapshot_url: format!("{}{served}/{url}", self.endpoints.replay),
                })
            }
            None => {
                let note = describe(std::slice::from_ref(&call.outcome));
                match call.outcome.result {
                    FinderResult::Failed(FailureCause::Status { code, .. })
                        if StatusCode::from_u16(code).is_ok_and(refusal_is_settled) =>
                    {
                        Err(ReplayMiss::Gone(note))
                    }
                    _ => Err(ReplayMiss::Unanswered(note)),
                }
            }
        }
    }

    /// Run one endpoint call under the user's switch and its breaker, and
    /// record and log what happened (the `Source finder outcome` line).
    async fn call<T, F, Fut>(&self, descriptor: &FinderDescriptor, request: F) -> Call<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<T>, FailureCause>>,
    {
        let id = descriptor.id.clone();
        let skipped = |reason: SkipReason| {
            self.health.record_skip(&id);
            Call {
                outcome: logged(FinderOutcome::skipped(id.clone(), reason)),
                value: None,
            }
        };
        if let Some(reason) = self.config().settings.gate(descriptor, &BTreeSet::new()) {
            return skipped(reason);
        }
        if let Admission::Refuse(reason) = self.health.admit(descriptor, Utc::now()) {
            return skipped(reason);
        }
        let started = Instant::now();
        let (result, value) = match request().await {
            Ok(Some(value)) => (FinderResult::Answered, Some(value)),
            Ok(None) => (FinderResult::Empty, None),
            Err(cause) => (FinderResult::Failed(cause), None),
        };
        let outcome = FinderOutcome {
            finder: id,
            leads: Vec::new(),
            result,
            elapsed: started.elapsed(),
        };
        self.health.record(descriptor, &outcome, Utc::now());
        Call {
            outcome: logged(outcome),
            value,
        }
    }

    async fn ask_availability(
        &self,
        url: &Url,
        as_of: Option<NaiveDate>,
    ) -> Result<Option<Capture>, FailureCause> {
        let mut params = vec![("url", url.to_string())];
        if let Some(day) = as_of {
            params.push(("timestamp", day_stamp(day)));
        }
        let request = endpoint_url(&self.endpoints.availability, &params)?;
        let body = self.index_body(&request).await?;
        capture_from_availability(&body)
    }

    async fn ask_cdx(&self, url: &Url, near: &str) -> Result<Option<Capture>, FailureCause> {
        let request = endpoint_url(
            &self.endpoints.cdx,
            &[
                ("url", url.to_string()),
                ("output", "json".to_string()),
                ("fl", "timestamp,statuscode".to_string()),
                ("filter", "statuscode:200".to_string()),
                ("closest", near.to_string()),
                ("sort", "closest".to_string()),
                ("limit", CDX_ROWS.to_string()),
            ],
        )?;
        let body = self.index_body(&request).await?;
        capture_from_cdx(&body)
    }

    async fn index_body(&self, request: &str) -> Result<String, FailureCause> {
        let config = self.config();
        let headers =
            identity::declared_client_headers(config.http.contact.as_deref(), JSON_ACCEPT);
        match config.http.get(request, headers, INDEX_BODY_BYTES).await {
            Fetched::Body(body) => Ok(body),
            Fetched::Status { code, retry_after } => {
                Err(FailureCause::Status { code, retry_after })
            }
            Fetched::Transport(detail) => Err(FailureCause::Transport(detail)),
        }
    }

    /// One paced replay GET: where it landed, its content type and its text.
    async fn fetch_replay(
        &self,
        request: &str,
    ) -> Result<Option<(String, Option<String>, Vec<u8>)>, FailureCause> {
        let config = self.config();
        let _site_slot = config.http.pacer.acquire(request).await;
        let headers =
            identity::declared_client_headers(config.http.contact.as_deref(), PAGE_ACCEPT);
        let response = config
            .http
            .client
            .get(request)
            .headers(headers)
            .send()
            .await
            .map_err(|error| FailureCause::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(FailureCause::Status {
                code: status.as_u16(),
                retry_after: retry_after(response.headers(), Utc::now()),
            });
        }
        let final_url = response.url().to_string();
        let content_type = content_type(response.headers());
        // Bytes, not text: an archived PDF decoded as text is no longer one.
        let body = read_bytes_bounded(response, MAX_PAGE_BODY_BYTES)
            .await
            .map_err(FailureCause::Transport)?;
        Ok((!body.iter().all(u8::is_ascii_whitespace)).then_some((final_url, content_type, body)))
    }
}

fn found(url: &Url, capture: Capture, by: &FinderId) -> Lookup {
    info!(
        url = url.as_str(),
        captured_at = %capture.captured_at,
        by = %by,
        "Archive capture found"
    );
    Lookup::Found(capture)
}

/// An index that answered with nothing, or that the user turned off: either
/// way the archive's answer is settled.
fn settled_empty(outcome: &FinderOutcome) -> bool {
    matches!(
        outcome.result,
        FinderResult::Empty | FinderResult::Skipped(SkipReason::Disabled)
    )
}

/// The endpoints' outcomes as the error and the step line read them.
fn describe(outcomes: &[FinderOutcome]) -> String {
    let lines: Vec<String> = outcomes
        .iter()
        .map(|outcome| match outcome.result.detail() {
            Some(detail) => format!("{}: {} ({detail})", outcome.finder, outcome.result.as_str()),
            None => format!("{}: {}", outcome.finder, outcome.result.as_str()),
        })
        .collect();
    format!("Internet Archive: {}", lines.join("; "))
}

fn logged(outcome: FinderOutcome) -> FinderOutcome {
    info!(
        finder = %outcome.finder,
        kind = NeedKind::Archive.as_str(),
        result = outcome.result.as_str(),
        leads = 0,
        ms = u64::try_from(outcome.elapsed.as_millis()).unwrap_or(u64::MAX),
        cause = outcome.result.detail().unwrap_or_default(),
        "Source finder outcome"
    );
    outcome
}

fn endpoint_url(base: &str, params: &[(&str, String)]) -> Result<String, FailureCause> {
    Url::parse_with_params(base, params)
        .map(String::from)
        .map_err(|error| FailureCause::Parse(format!("archive endpoint {base}: {error}")))
}

fn content_type(headers: &HeaderMap) -> Option<String> {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// The day asked for, in the archive's 1–14 digit form (`YYYYMMDD`).
fn day_stamp(day: NaiveDate) -> String {
    day.format("%Y%m%d").to_string()
}

/// What "nearest" means: the day asked for, or now for the latest copy.
fn target_stamp(as_of: Option<NaiveDate>, now: DateTime<Utc>) -> String {
    match as_of {
        Some(day) => day_stamp(day),
        None => now.format(CAPTURE_TIMESTAMP).to_string(),
    }
}

/// `{"archived_snapshots":{"closest":{"available":true,"status":"200",
/// "timestamp":"…","url":"…"}}}`, or `{"archived_snapshots":{}}` for none. A
/// closest capture that did not answer 200 (a redirect, a 404 the crawler
/// saw) is not a readable copy.
fn capture_from_availability(body: &str) -> Result<Option<Capture>, FailureCause> {
    let payload: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| FailureCause::Parse(format!("availability answer: {error}")))?;
    let snapshots = payload
        .get("archived_snapshots")
        .ok_or_else(|| FailureCause::Parse("no archived_snapshots field".to_string()))?;
    let Some(closest) = snapshots.get("closest") else {
        return Ok(None);
    };
    let available = closest
        .get("available")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let status = closest.get("status").and_then(serde_json::Value::as_str);
    if !available || status != Some("200") {
        return Ok(None);
    }
    let timestamp = closest
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| FailureCause::Parse("closest capture has no timestamp".to_string()))?;
    Capture::from_timestamp(timestamp).map(Some)
}

/// `output=json`: a header row naming the fields, then one row per capture;
/// `[]` (or an empty body) when nothing matched.
fn capture_from_cdx(body: &str) -> Result<Option<Capture>, FailureCause> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    let rows: Vec<Vec<String>> = serde_json::from_str(body)
        .map_err(|error| FailureCause::Parse(format!("CDX answer: {error}")))?;
    let mut rows = rows.into_iter();
    let Some(header) = rows.next() else {
        return Ok(None);
    };
    let column = header
        .iter()
        .position(|field| field == "timestamp")
        .ok_or_else(|| FailureCause::Parse("CDX answer has no timestamp column".to_string()))?;
    let Some(row) = rows.next() else {
        return Ok(None);
    };
    let timestamp = row
        .get(column)
        .ok_or_else(|| FailureCause::Parse("CDX row has no timestamp".to_string()))?;
    Capture::from_timestamp(timestamp).map(Some)
}

#[cfg(test)]
#[path = "wayback_tests.rs"]
mod tests;
