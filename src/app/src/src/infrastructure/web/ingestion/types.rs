use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDocument {
    pub url: String,
    pub content: WebContent,
    pub metadata: WebMetadata,
    pub fetched_at: DateTime<Utc>,
}

impl WebDocument {
    pub fn new(url: String, content: WebContent, metadata: WebMetadata) -> Self {
        Self {
            url,
            content,
            metadata,
            fetched_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebContent {
    pub title: String,
    pub body: String,
    pub excerpt: Option<String>,
    pub byline: Option<String>,
    pub word_count: usize,
    pub lang: Option<String>,
}

impl WebContent {
    pub fn new(title: String, body: String) -> Self {
        let word_count = body.split_whitespace().count();
        let excerpt = Self::generate_excerpt(&body);

        Self {
            title,
            body,
            excerpt,
            byline: None,
            word_count,
            lang: None,
        }
    }

    pub fn with_byline(mut self, byline: String) -> Self {
        self.byline = Some(byline);
        self
    }

    pub fn with_lang(mut self, lang: String) -> Self {
        self.lang = Some(lang);
        self
    }

    fn generate_excerpt(text: &str) -> Option<String> {
        const EXCERPT_LENGTH: usize = 200;

        if text.len() <= EXCERPT_LENGTH {
            return Some(text.to_string());
        }

        let excerpt = text.chars().take(EXCERPT_LENGTH).collect::<String>();
        let last_space = excerpt.rfind(' ')?;

        Some(format!("{}...", &excerpt[..last_space]))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebMetadata {
    pub site_name: Option<String>,
    pub author: Option<String>,
    pub published_time: Option<DateTime<Utc>>,
    pub modified_time: Option<DateTime<Utc>>,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub image_url: Option<String>,
    pub favicon_url: Option<String>,
    pub canonical_url: Option<String>,
    pub content_type: Option<String>,
}

impl Default for WebMetadata {
    fn default() -> Self {
        Self {
            site_name: None,
            author: None,
            published_time: None,
            modified_time: None,
            description: None,
            keywords: Vec::new(),
            image_url: None,
            favicon_url: None,
            canonical_url: None,
            content_type: None,
        }
    }
}

impl WebMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_site_name(mut self, site_name: String) -> Self {
        self.site_name = Some(site_name);
        self
    }

    pub fn with_author(mut self, author: String) -> Self {
        self.author = Some(author);
        self
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    pub fn with_image_url(mut self, image_url: String) -> Self {
        self.image_url = Some(image_url);
        self
    }

    pub fn with_canonical_url(mut self, canonical_url: String) -> Self {
        self.canonical_url = Some(canonical_url);
        self
    }

    pub fn with_content_type(mut self, content_type: String) -> Self {
        self.content_type = Some(content_type);
        self
    }
}

pub(crate) fn normalize_url(url: &str) -> Result<Url, url::ParseError> {
    let parsed = Url::parse(url)?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(url::ParseError::InvalidDomainCharacter);
    }

    Ok(parsed)
}

/// Check if an IPv4 address is safe for public access
fn is_safe_ipv4(ip: &Ipv4Addr) -> bool {
    // Block loopback (127.0.0.0/8)
    if ip.is_loopback() {
        return false;
    }

    // Block private (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
    if ip.is_private() {
        return false;
    }

    // Block link-local (169.254.0.0/16)
    if ip.is_link_local() {
        return false;
    }

    // Block broadcast
    if ip.is_broadcast() {
        return false;
    }

    // Block unspecified (0.0.0.0)
    if ip.is_unspecified() {
        return false;
    }

    // Block cloud metadata endpoint (169.254.169.254)
    if is_cloud_metadata(ip) {
        return false;
    }

    // Block 0.0.0.0/8 range (current network)
    let octets = ip.octets();
    if octets[0] == 0 {
        return false;
    }

    true
}

/// Check if an IPv6 address is safe for public access
fn is_safe_ipv6(ip: &Ipv6Addr) -> bool {
    // Block loopback (::1)
    if ip.is_loopback() {
        return false;
    }

    // Block unique local (fc00::/7)
    if ip.is_unique_local() {
        return false;
    }

    // Block multicast
    if ip.is_multicast() {
        return false;
    }

    // Block unspecified (::)
    if ip.is_unspecified() {
        return false;
    }

    // Block link-local (fe80::/10)
    // SECURITY: Manual check required since is_unicast_link_local() is unstable
    // fe80::/10 means first 10 bits are 1111 1110 10xx xxxx
    // Checking first segment: 0xfe80 with mask 0xffc0 (first 10 bits)
    let segments = ip.segments();
    if segments[0] & 0xffc0 == 0xfe80 {
        return false;
    }

    // Check for IPv4-mapped IPv6 (::ffff:0:0/96)
    // These addresses map IPv4 addresses into IPv6 space
    if let Some(ipv4) = ip.to_ipv4_mapped() {
        return is_safe_ipv4(&ipv4);
    }

    true
}

/// Check if the IP is a cloud metadata endpoint
fn is_cloud_metadata(ip: &Ipv4Addr) -> bool {
    // AWS/GCP/Azure metadata endpoint: 169.254.169.254
    ip.octets() == [169, 254, 169, 254]
}

/// Validate that an IP address is safe for external requests
pub(crate) fn is_safe_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_safe_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_safe_ipv6(ipv6),
    }
}

/// Check if a URL is safe by validating its host component
///
/// For IP addresses in the URL, this validates them directly.
/// For domain names, DNS resolution must be validated separately (see fetcher.rs)
pub(crate) fn is_safe_url(url: &Url) -> bool {
    // Get the host component
    let host = match url.host() {
        Some(h) => h,
        None => return false,
    };

    // If it's an IP address, validate it directly
    match host {
        url::Host::Ipv4(ip) => is_safe_ipv4(&ip),
        url::Host::Ipv6(ip) => is_safe_ipv6(&ip),
        url::Host::Domain(domain) => {
            // For domains, we can't validate the IP without DNS resolution
            // This will be handled in the fetcher (DNS resolution validation)
            // Block obviously dangerous domain patterns
            let domain_lower = domain.to_lowercase();

            // Block localhost and local domains
            if domain_lower == "localhost"
                || domain_lower.ends_with(".local")
                || domain_lower.ends_with(".localhost")
            {
                return false;
            }

            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_excerpt_generation() {
        let short_text = "This is a short text.";
        let excerpt = WebContent::generate_excerpt(short_text);
        assert_eq!(excerpt, Some(short_text.to_string()));

        let long_text = "a".repeat(250);
        let excerpt = WebContent::generate_excerpt(&long_text);
        assert!(excerpt.is_some());
        assert!(excerpt.unwrap().len() <= 204);
    }

    #[test]
    fn test_safe_url_public() {
        // Public IPs should be safe
        let safe = Url::parse("https://example.com").unwrap();
        assert!(is_safe_url(&safe));

        let safe_ip = Url::parse("http://8.8.8.8").unwrap();
        assert!(is_safe_url(&safe_ip));
    }

    #[test]
    fn test_safe_url_localhost() {
        // Localhost domain should be blocked
        let unsafe_localhost = Url::parse("http://localhost").unwrap();
        assert!(!is_safe_url(&unsafe_localhost));

        // IPv4 loopback should be blocked (127.0.0.0/8)
        let unsafe_loopback = Url::parse("http://127.0.0.1").unwrap();
        assert!(!is_safe_url(&unsafe_loopback));

        let unsafe_loopback2 = Url::parse("http://127.1.2.3").unwrap();
        assert!(!is_safe_url(&unsafe_loopback2));

        // IPv6 loopback should be blocked
        let unsafe_ipv6_loopback = Url::parse("http://[::1]").unwrap();
        assert!(!is_safe_url(&unsafe_ipv6_loopback));
    }

    #[test]
    fn test_safe_url_private_networks() {
        // RFC1918 private networks should be blocked

        // 10.0.0.0/8
        let unsafe_10 = Url::parse("http://10.0.0.1").unwrap();
        assert!(!is_safe_url(&unsafe_10));

        let unsafe_10_2 = Url::parse("http://10.255.255.255").unwrap();
        assert!(!is_safe_url(&unsafe_10_2));

        // 172.16.0.0/12
        let unsafe_172_16 = Url::parse("http://172.16.0.1").unwrap();
        assert!(!is_safe_url(&unsafe_172_16));

        let unsafe_172_31 = Url::parse("http://172.31.255.255").unwrap();
        assert!(!is_safe_url(&unsafe_172_31));

        // 192.168.0.0/16
        let unsafe_192 = Url::parse("http://192.168.1.1").unwrap();
        assert!(!is_safe_url(&unsafe_192));

        let unsafe_192_2 = Url::parse("http://192.168.255.255").unwrap();
        assert!(!is_safe_url(&unsafe_192_2));
    }

    #[test]
    fn test_safe_url_link_local() {
        // Link-local addresses should be blocked (169.254.0.0/16)
        let unsafe_link_local = Url::parse("http://169.254.1.1").unwrap();
        assert!(!is_safe_url(&unsafe_link_local));

        // IPv6 link-local should be blocked (fe80::/10)
        let unsafe_ipv6_link_local = Url::parse("http://[fe80::1]").unwrap();
        assert!(!is_safe_url(&unsafe_ipv6_link_local));
    }

    #[test]
    fn test_safe_url_cloud_metadata() {
        // AWS/GCP/Azure metadata endpoint should be blocked
        let unsafe_metadata = Url::parse("http://169.254.169.254").unwrap();
        assert!(!is_safe_url(&unsafe_metadata));
    }

    #[test]
    fn test_safe_url_ipv6_unique_local() {
        // IPv6 unique local addresses should be blocked (fc00::/7)
        let unsafe_fc00 = Url::parse("http://[fc00::1]").unwrap();
        assert!(!is_safe_url(&unsafe_fc00));

        let unsafe_fd00 = Url::parse("http://[fd00::1]").unwrap();
        assert!(!is_safe_url(&unsafe_fd00));
    }

    #[test]
    fn test_safe_url_ipv4_mapped_ipv6() {
        // IPv4-mapped IPv6 addresses should be validated based on the IPv4 address
        // ::ffff:127.0.0.1 should be blocked (loopback)
        let unsafe_mapped_loopback = Url::parse("http://[::ffff:127.0.0.1]").unwrap();
        assert!(!is_safe_url(&unsafe_mapped_loopback));

        // ::ffff:192.168.1.1 should be blocked (private)
        let unsafe_mapped_private = Url::parse("http://[::ffff:192.168.1.1]").unwrap();
        assert!(!is_safe_url(&unsafe_mapped_private));

        // ::ffff:8.8.8.8 should be safe (public)
        let safe_mapped_public = Url::parse("http://[::ffff:8.8.8.8]").unwrap();
        assert!(is_safe_url(&safe_mapped_public));
    }

    #[test]
    fn test_safe_url_special_addresses() {
        // 0.0.0.0 and 0.0.0.0/8 should be blocked
        let unsafe_unspecified = Url::parse("http://0.0.0.0").unwrap();
        assert!(!is_safe_url(&unsafe_unspecified));

        let unsafe_zero_net = Url::parse("http://0.1.2.3").unwrap();
        assert!(!is_safe_url(&unsafe_zero_net));

        // IPv6 unspecified should be blocked
        let unsafe_ipv6_unspecified = Url::parse("http://[::]").unwrap();
        assert!(!is_safe_url(&unsafe_ipv6_unspecified));

        // Broadcast should be blocked
        let unsafe_broadcast = Url::parse("http://255.255.255.255").unwrap();
        assert!(!is_safe_url(&unsafe_broadcast));
    }

    #[test]
    fn test_safe_url_multicast() {
        // IPv6 multicast should be blocked (ff00::/8)
        let unsafe_multicast = Url::parse("http://[ff02::1]").unwrap();
        assert!(!is_safe_url(&unsafe_multicast));
    }

    #[test]
    fn test_safe_url_local_domains() {
        // .local domains should be blocked
        let unsafe_local = Url::parse("http://myserver.local").unwrap();
        assert!(!is_safe_url(&unsafe_local));

        // .localhost domains should be blocked
        let unsafe_localhost_domain = Url::parse("http://test.localhost").unwrap();
        assert!(!is_safe_url(&unsafe_localhost_domain));
    }

    #[test]
    fn test_safe_ip_function() {
        // Test the is_safe_ip function directly
        use std::net::IpAddr;

        // Public IPv4 should be safe
        let public_ipv4: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(is_safe_ip(&public_ipv4));

        // Private IPv4 should be blocked
        let private_ipv4: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(!is_safe_ip(&private_ipv4));

        // Public IPv6 should be safe
        let public_ipv6: IpAddr = "2001:4860:4860::8888".parse().unwrap();
        assert!(is_safe_ip(&public_ipv6));

        // Loopback IPv6 should be blocked
        let loopback_ipv6: IpAddr = "::1".parse().unwrap();
        assert!(!is_safe_ip(&loopback_ipv6));
    }

    #[test]
    fn test_normalize_url() {
        assert!(normalize_url("https://example.com").is_ok());
        assert!(normalize_url("http://example.com").is_ok());
        assert!(normalize_url("ftp://example.com").is_err());
    }
}
