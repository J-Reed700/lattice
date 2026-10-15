//! Private-address guard (CWE-918).
//!
//! A page read must never reach this machine or the user's network. Three
//! places enforce it: the URL check before the first request, the redirect
//! policy for every hop (so a public page's 302 to a router is refused before
//! the GET), and the client's resolver, which refuses a name that resolves
//! privately and hands the connector exactly the addresses it checked, so a
//! second lookup cannot rebind to a private one.
use super::parse_fetchable_url;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use tracing::{debug, warn};
use url::{Host, Url};

/// Redirect hops followed before giving up, as reqwest's default does.
const MAX_REDIRECTS: usize = 10;

/// Whether `ip` is loopback, private, link-local, shared, reserved or
/// otherwise not on the public internet.
pub(crate) fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local() // 169.254/16, cloud metadata included
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0 // 0.0.0.0/8: "this network", which reaches localhost on some stacks
        || (a == 100 && (b & 0xc0) == 64) // 100.64/10: carrier-grade NAT, Tailscale
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    // `::ffff:192.168.1.1` is the IPv4 address wearing an IPv6 costume, and
    // the NAT64 prefix `64:ff9b::/96` is translated to its embedded IPv4.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_ipv4(v4);
    }
    let s = ip.segments();
    if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return is_blocked_ipv4(v4);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || s[0] & 0xfe00 == 0xfc00 // fc00::/7, unique local (cloud metadata fd00:ec2::254 included)
        || s[0] & 0xffc0 == 0xfe80 // fe80::/10, link-local
}

/// Refuse what the URL alone shows to be local: the localhost names and a
/// literal address in a blocked range. Returns the domain still to be
/// resolved, or `None` when the host was a literal and is already cleared.
pub(super) fn check_url_host(url: &Url) -> Result<Option<String>> {
    match url.host() {
        None => Err(AppError::InvalidUrl("URL must have a host".to_string())),
        Some(Host::Ipv4(ip)) => refuse_ip(IpAddr::V4(ip)).map(|()| None),
        Some(Host::Ipv6(ip)) => refuse_ip(IpAddr::V6(ip)).map(|()| None),
        Some(Host::Domain(domain)) => {
            let name = domain.trim_end_matches('.').to_ascii_lowercase();
            if name == "localhost" || name.ends_with(".localhost") {
                return Err(AppError::InvalidUrl(format!(
                    "Access to localhost blocked: {domain}"
                )));
            }
            Ok(Some(name))
        }
    }
}

fn refuse_ip(ip: IpAddr) -> Result<()> {
    if is_blocked_ip(ip) {
        return Err(AppError::InvalidUrl(format!(
            "Access to private/reserved IP blocked: {ip}"
        )));
    }
    Ok(())
}

pub(super) fn refuse_blocked_addresses(host: &str, resolved: &[SocketAddr]) -> Result<()> {
    if resolved.is_empty() {
        return Err(AppError::InvalidUrl(format!(
            "No IP addresses resolved for hostname: {host}"
        )));
    }
    let blocked: Vec<String> = resolved
        .iter()
        .filter(|addr| is_blocked_ip(addr.ip()))
        .map(|addr| addr.ip().to_string())
        .collect();
    if !blocked.is_empty() {
        return Err(AppError::InvalidUrl(format!(
            "Hostname {host} resolves to blocked IP(s): {}. Access to private networks is not allowed.",
            blocked.join(", ")
        )));
    }
    Ok(())
}

/// Resolve `host` without blocking the runtime and refuse it if any address
/// it resolves to is blocked. Any, not all: a name that answers with one
/// public and one private address can be steered to the private one.
async fn resolve_public(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| AppError::InvalidUrl(format!("Failed to resolve hostname {host}: {e}")))?
        .collect();
    refuse_blocked_addresses(host, &resolved)?;
    Ok(resolved)
}

/// The full check for a URL this service is about to request: scheme, host
/// and every address the host resolves to.
pub(crate) async fn validate_url_resolved(url: &str) -> Result<()> {
    let parsed = parse_fetchable_url(url)?;
    if let Some(domain) = check_url_host(&parsed)? {
        let port = parsed.port_or_known_default().unwrap_or(443);
        resolve_public(&domain, port).await?;
    }
    debug!("URL validation passed: {}", url);
    Ok(())
}

/// Follow a redirect only to an http(s) URL whose host is not local. A hop
/// to a name is resolved, and refused if private, by [`PublicOnlyResolver`]
/// before anything connects.
pub(crate) fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error(format!("too many redirects (over {MAX_REDIRECTS})"));
        }
        let checked =
            parse_fetchable_url(attempt.url().as_str()).and_then(|url| check_url_host(&url));
        match checked {
            Ok(_) => attempt.follow(),
            Err(e) => {
                warn!(target_url = %attempt.url(), error = %e, "Redirect refused");
                attempt.error(e.to_string())
            }
        }
    })
}

/// The web client's DNS resolver: an address in a blocked range fails the
/// lookup, so neither the first request nor any redirect hop can connect to
/// one, and the connector uses exactly the addresses checked here.
pub(crate) struct PublicOnlyResolver {
    /// Configured proxies, which may legitimately live on this machine.
    exempt_hosts: HashSet<String>,
}

impl PublicOnlyResolver {
    pub(crate) fn new(exempt_hosts: Vec<String>) -> Self {
        Self {
            exempt_hosts: exempt_hosts.into_iter().collect(),
        }
    }
}

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().trim_end_matches('.').to_ascii_lowercase();
        let exempt = self.exempt_hosts.contains(&host);
        Box::pin(async move {
            // The connector sets the port itself; 0 is a placeholder.
            let addrs: Vec<SocketAddr> = if exempt {
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect()
            } else {
                if host == "localhost" || host.ends_with(".localhost") {
                    return Err(format!("Access to localhost blocked: {host}").into());
                }
                resolve_public(&host, 0).await.map_err(|e| e.to_string())?
            };
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}
