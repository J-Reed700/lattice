//! Why a link the user attached could not be saved, in the claim card's
//! words, and what to do next (§5.4 "Fetch failures"). Every case is read
//! from the web service's existing errors or from the page it returned; the
//! page is never saved when it reads as a wall.

use lattice_core::application::contracts::web::FetchUrlContentOutput;
use lattice_core::shared::error::AppError;
use serde::{Deserialize, Serialize};

/// The start of the web service's thin-page refusal (`fetch_reference_content`).
const THIN_PAGE_REFUSAL: &str = "This page did not provide enough static reference text";
/// The workflow's refusal of a page with no title or no text.
const UNUSABLE_PAGE_REFUSAL: &str = "The page did not contain usable article text";
/// The browser reader's refusal when no browser can run (`browser_reader::unavailable`).
const NO_BROWSER: &str = "no browser is available";
/// The HTTP statuses a site answers when it refuses a reader rather than
/// failing: authentication, forbidden, rate limited.
const REFUSING_STATUSES: [u16; 3] = [401, 403, 429];
/// The most words a sign-in or subscription wall gives: a headline and a
/// teaser paragraph. A page with more is an article, even with a
/// "subscribe" footer.
const WALL_MAX_WORDS: usize = 250;
/// What a sign-in or subscription wall says where the article would be.
const WALL_PHRASES: [&str; 14] = [
    "subscribe to continue",
    "subscribe to read",
    "subscribers only",
    "for subscribers",
    "already a subscriber",
    "sign in to continue",
    "sign in to read",
    "log in to continue",
    "log in to read",
    "login to continue",
    "create a free account",
    "become a member to",
    "members only",
    "register to continue",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AttachFailureKindDto {
    /// HTTP 401, 403 or 429.
    Refused,
    /// A sign-in or subscription wall.
    Paywall,
    /// Too little static text, or none.
    Thin,
    BrowserUnavailable,
    /// A private or local address (SSRF guard).
    Blocked,
    /// Any other failure to read the page.
    Unreachable,
}

/// What the card offers after a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum AttachNextStepDto {
    /// "Open it in your browser, then paste the relevant text"; the pasted
    /// text input is prefilled with `origin` as its source.
    PasteText {
        origin: Option<String>,
    },
    /// "Save it as PDF and add the file."
    AddFile,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachFailureDto {
    pub kind: AttachFailureKindDto,
    pub message: String,
    pub next_steps: Vec<AttachNextStepDto>,
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
        .unwrap_or_else(|| url.to_owned())
}

/// The status of an `HTTP <status>…` network error, as the page reader and
/// its refusal cache word them.
fn http_status(message: &str) -> Option<u16> {
    let rest = message.strip_prefix("HTTP ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

impl AttachFailureDto {
    fn new(kind: AttachFailureKindDto, message: String, url: &str) -> Self {
        let paste = AttachNextStepDto::PasteText {
            origin: Some(url.to_owned()),
        };
        let next_steps = match kind {
            AttachFailureKindDto::Refused
            | AttachFailureKindDto::Paywall
            | AttachFailureKindDto::Thin => vec![paste, AttachNextStepDto::AddFile],
            AttachFailureKindDto::BrowserUnavailable | AttachFailureKindDto::Unreachable => {
                vec![AttachNextStepDto::Retry, paste, AttachNextStepDto::AddFile]
            }
            AttachFailureKindDto::Blocked => vec![AttachNextStepDto::AddFile, paste],
        };
        Self {
            kind,
            message,
            next_steps,
        }
    }

    /// The card's account of a failed read of `url`.
    pub fn from_fetch_error(url: &str, error: &AppError) -> Self {
        let host = host_of(url);
        let (kind, message) = match error {
            AppError::Network(message) => match http_status(message) {
                Some(status) if REFUSING_STATUSES.contains(&status) => (
                    AttachFailureKindDto::Refused,
                    format!("{host} refused the request (HTTP {status})."),
                ),
                Some(status) => (
                    AttachFailureKindDto::Unreachable,
                    format!("{host} answered HTTP {status}; the page could not be read."),
                ),
                None => (
                    AttachFailureKindDto::Unreachable,
                    format!("{host} could not be read: {message}"),
                ),
            },
            AppError::InvalidInput(message)
                if message.starts_with(THIN_PAGE_REFUSAL)
                    || message.starts_with(UNUSABLE_PAGE_REFUSAL) =>
            {
                (AttachFailureKindDto::Thin, message.clone())
            }
            AppError::ServiceNotAvailable(message) if message.contains(NO_BROWSER) => (
                AttachFailureKindDto::BrowserUnavailable,
                "The page needs a browser to render and the in-app browser is not available right now.".into(),
            ),
            AppError::InvalidUrl(message) if message.contains("blocked") => (
                AttachFailureKindDto::Blocked,
                "That address is private or local and cannot be fetched.".into(),
            ),
            other => (
                AttachFailureKindDto::Unreachable,
                format!("{host} could not be read: {other}"),
            ),
        };
        Self::new(kind, message, url)
    }

    /// A sign-in or subscription wall in place of the article: little text
    /// that asks the reader to sign in or subscribe. `None` for an article.
    pub fn from_wall(url: &str, page: &FetchUrlContentOutput) -> Option<Self> {
        let words = page.content.split_whitespace().count();
        if words > WALL_MAX_WORDS {
            return None;
        }
        let text = page.content.to_lowercase();
        WALL_PHRASES
            .iter()
            .any(|phrase| text.contains(phrase))
            .then(|| {
                Self::new(
                    AttachFailureKindDto::Paywall,
                    format!(
                        "The page needs a login or subscription; only {words} words of text could be read."
                    ),
                    url,
                )
            })
    }
}
