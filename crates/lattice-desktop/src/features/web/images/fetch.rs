//! Fetch and validate one picture (§7.4): `https` only, through the web
//! feature's guarded client and site pacer, body bounded by
//! [`MAX_IMAGE_BYTES`], format sniffed from the bytes (the `Content-Type`
//! header is never trusted), dimensions read from the header alone and
//! bounded by [`MAX_IMAGE_PIXELS`].

use std::time::Instant;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};

use super::{extension_for_mime, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};
use crate::features::web::services::stealth;
use crate::features::web::services::web::{BoundedBody, BoundedFetchError, WebService};
use crate::features::web::snapshots::store::sha256_hex;

/// Identifies the app to Wikimedia, whose API policy asks clients for an
/// identifying agent. The Wikipedia tool sends the same value.
pub const COMMONS_USER_AGENT: &str =
    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// The network the picture service reads through. Production is the web
/// feature's guarded [`WebService`]; tests serve canned bodies.
#[async_trait]
pub trait ImageNetwork: Send + Sync {
    /// GET `url` with `headers`, reading at most `max_bytes`, stopping at
    /// `deadline` when one is given.
    async fn get(
        &self,
        url: &str,
        headers: HeaderMap,
        max_bytes: usize,
        deadline: Option<Instant>,
    ) -> Result<BoundedBody, BoundedFetchError>;
}

#[async_trait]
impl ImageNetwork for WebService {
    async fn get(
        &self,
        url: &str,
        headers: HeaderMap,
        max_bytes: usize,
        deadline: Option<Instant>,
    ) -> Result<BoundedBody, BoundedFetchError> {
        self.fetch_bytes_bounded(url, headers, max_bytes, deadline)
            .await
    }
}

/// Headers for a Wikimedia request: the identifying agent and `accept`.
pub fn commons_headers(accept: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(COMMONS_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static(accept));
    headers
}

/// Headers for any other site: the same browser profile page reads send,
/// with `referer` when the request is for something a page shows.
pub fn browser_headers(referer: Option<&str>, accept: Option<&'static str>) -> HeaderMap {
    let mut headers = stealth::browser_headers(stealth::random_profile(), referer);
    if let Some(accept) = accept {
        headers.insert(ACCEPT, HeaderValue::from_static(accept));
    }
    headers
}

/// What an image request asks for: the four stored formats.
pub const IMAGE_ACCEPT: &str = "image/webp,image/png,image/jpeg,image/gif;q=0.9";

/// Why a candidate picture was not stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageRejected {
    /// Not an `https` URL, or a redirect left `https`.
    Scheme,
    /// The address guard refused the host; nothing was sent.
    Blocked(String),
    Network(String),
    Status(u16),
    /// Larger than [`MAX_IMAGE_BYTES`].
    TooLarge,
    /// The turn's deadline passed.
    Deadline,
    /// Not one of the stored formats; carries the sniffed type, if any.
    Format(Option<String>),
    /// The header names no readable width and height.
    Unreadable,
    /// More than [`MAX_IMAGE_PIXELS`].
    TooManyPixels {
        width: u32,
        height: u32,
    },
}

impl std::fmt::Display for ImageRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scheme => f.write_str("not an https address"),
            Self::Blocked(reason) => write!(f, "address refused ({reason})"),
            Self::Network(reason) => write!(f, "could not download ({reason})"),
            Self::Status(status) => write!(f, "the server answered HTTP {status}"),
            Self::TooLarge => f.write_str("the file is too large to show safely"),
            Self::Deadline => f.write_str("the turn ran out of time"),
            Self::Format(Some(mime)) => write!(f, "{mime} is not a picture format shown here"),
            Self::Format(None) => f.write_str("the file is not a recognised picture"),
            Self::Unreadable => f.write_str("the picture's size could not be read"),
            Self::TooManyPixels { width, height } => {
                write!(f, "{width}×{height} is too many pixels to show safely")
            }
        }
    }
}

impl From<BoundedFetchError> for ImageRejected {
    fn from(error: BoundedFetchError) -> Self {
        match error {
            BoundedFetchError::Refused(reason) => Self::Blocked(reason),
            BoundedFetchError::Network(reason) => Self::Network(reason),
            BoundedFetchError::Status(status) => Self::Status(status),
            BoundedFetchError::TooLarge => Self::TooLarge,
            BoundedFetchError::Deadline => Self::Deadline,
        }
    }
}

/// Bytes that passed every check, with what was learned from them.
#[derive(Debug, Clone)]
pub struct ValidatedImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub ext: &'static str,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

/// A fetched, validated picture and where the request ended.
#[derive(Debug, Clone)]
pub struct FetchedImage {
    pub image: ValidatedImage,
    pub final_url: String,
}

/// Checks downloaded bytes: within [`MAX_IMAGE_BYTES`], one of the stored
/// formats by magic bytes, and a readable header within [`MAX_IMAGE_PIXELS`].
/// Nothing is decoded.
pub fn validate_image(bytes: Vec<u8>) -> Result<ValidatedImage, ImageRejected> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageRejected::TooLarge);
    }
    let sniffed = infer::get(&bytes).map(|kind| kind.mime_type());
    let Some((mime, ext)) =
        sniffed.and_then(|mime| extension_for_mime(mime).map(|ext| (mime, ext)))
    else {
        return Err(ImageRejected::Format(sniffed.map(str::to_string)));
    };
    let size = imagesize::blob_size(&bytes).map_err(|_| ImageRejected::Unreadable)?;
    let (Ok(width), Ok(height)) = (u32::try_from(size.width), u32::try_from(size.height)) else {
        return Err(ImageRejected::Unreadable);
    };
    if width == 0 || height == 0 {
        return Err(ImageRejected::Unreadable);
    }
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(ImageRejected::TooManyPixels { width, height });
    }
    let sha256 = sha256_hex(&bytes);
    Ok(ValidatedImage {
        bytes,
        mime,
        ext,
        width,
        height,
        sha256,
    })
}

fn is_https(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https")
}

/// Fetches `url` through `network` and validates the bytes. `https` only,
/// on the first hop and after redirects; `deadline` is the turn's.
pub async fn fetch_image(
    network: &dyn ImageNetwork,
    url: &str,
    headers: HeaderMap,
    deadline: Option<Instant>,
) -> Result<FetchedImage, ImageRejected> {
    if !is_https(url) {
        return Err(ImageRejected::Scheme);
    }
    let body = network.get(url, headers, MAX_IMAGE_BYTES, deadline).await?;
    if !is_https(&body.final_url) {
        return Err(ImageRejected::Scheme);
    }
    Ok(FetchedImage {
        image: validate_image(body.bytes)?,
        final_url: body.final_url,
    })
}
