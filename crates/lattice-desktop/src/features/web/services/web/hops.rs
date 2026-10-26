//! The redirects an address answers with, read one hop at a time, for
//! following a search scope to where its site lives now (`SiteHomes`).
//!
//! The client follows no redirect itself, so that each hop's status can be
//! read: whether a site moved is a question of 301 against 302. Every hop
//! clears the same check the redirect policy gives a page read's hops before
//! it is requested, the client's resolver refuses a private address as it
//! does for every read, and each request takes the site's pacing slot. Only
//! the status and `Location` are read; no body is.

use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::domain::sourcing::Hop;
use reqwest::header::LOCATION;
use reqwest::{Client, StatusCode};
use url::Url;

use super::address_guard::{check_url_host, MAX_REDIRECTS};
use super::page::parse_fetchable_url;
use super::SitePacer;
use crate::features::web::services::identity;
use crate::features::web::services::sourcing::HopReader;

/// Reads redirects over the public, non-following client.
pub(crate) struct HttpHops {
    client: Client,
    pacer: Arc<SitePacer>,
    /// The address check every hop clears before it is requested.
    clear: fn(&str) -> Result<Url, String>,
}

impl HttpHops {
    /// Reads through `client`, which must follow no redirect
    /// ([`super::build_hop_client`]), paced by the shared `pacer`.
    pub(crate) fn new(client: Client, pacer: Arc<SitePacer>) -> Self {
        Self {
            client,
            pacer,
            clear: cleared,
        }
    }
}

#[async_trait]
impl HopReader for HttpHops {
    async fn hops(&self, from: &Url) -> Result<Vec<Hop>, String> {
        let mut at = (self.clear)(from.as_str())?;
        let mut hops = Vec::new();
        loop {
            let response = {
                let _site_slot = self.pacer.acquire(at.as_str()).await;
                self.client
                    .get(at.clone())
                    .headers(identity::browser_headers(identity::random_profile(), None))
                    .send()
                    .await
                    .map_err(|error| error.to_string())?
            };
            let status = response.status();
            if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                return Err(format!("HTTP {status}"));
            }
            let location = status
                .is_redirection()
                .then(|| response.headers().get(LOCATION))
                .flatten()
                .and_then(|value| value.to_str().ok());
            // A hop past the redirect limit, or one the guard refuses, is not
            // followed: the hops read so far are what the site said.
            let Some(next) = location
                .filter(|_| hops.len() < MAX_REDIRECTS)
                .and_then(|location| at.join(location).ok())
                .and_then(|next| (self.clear)(next.as_str()).ok())
            else {
                return Ok(hops);
            };
            hops.push(Hop {
                status: status.as_u16(),
                to: next.clone(),
            });
            at = next;
        }
    }
}

/// `address` when this service would request it: http(s), and not a host
/// the URL alone shows to be local.
fn cleared(address: &str) -> Result<Url, String> {
    let url = parse_fetchable_url(address).map_err(|error| error.to_string())?;
    check_url_host(&url).map_err(|error| error.to_string())?;
    Ok(url)
}

#[cfg(test)]
#[path = "hops_tests.rs"]
mod tests;
