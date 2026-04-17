use crate::shared::error::AppError;
use crate::shared::utils::reqwest_client_builder;
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;
use url::Url;

/// Maximum number of redirects to follow
const MAX_REDIRECTS: u8 = 5;

/// Timeout for URL validation (30 seconds)
const VALIDATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Service for validating URLs and preventing SSRF attacks (CWE-918)
pub struct UrlValidationService {
    client: Client,
}

impl UrlValidationService {
    /// Create a new URL validation service
    pub fn new() -> Result<Self, AppError> {
        let client = reqwest_client_builder()
            .timeout(VALIDATION_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none()) // Handle redirects manually for SSRF checks
            .build()
            .map_err(|e| {
                AppError::ValidationFailed(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self { client })
    }

    /// Pre-save validation: SSRF prevention (fast checks before saving to DB)
    ///
    /// Checks:
    /// - Valid URL format
    /// - HTTPS protocol
    /// - Not a private IP or localhost
    /// - Not a cloud metadata endpoint
    pub async fn validate_url_pre_save(&self, url_str: &str) -> Result<Url, AppError> {
        // Parse URL
        let url = Url::parse(url_str)
            .map_err(|e| AppError::ValidationFailed(format!("Invalid URL format: {}", e)))?;

        // Require HTTPS for security
        if url.scheme() != "https" {
            return Err(AppError::ValidationFailed(
                "Only HTTPS URLs are allowed".to_string(),
            ));
        }

        // Get hostname
        let host = url
            .host_str()
            .ok_or_else(|| AppError::ValidationFailed("URL must have a host".to_string()))?;

        // Check if it's a cloud metadata endpoint
        if self.is_cloud_metadata_endpoint(host) {
            return Err(AppError::Security(
                "Access to cloud metadata endpoints is forbidden".to_string(),
            ));
        }

        // Resolve DNS and check IP address
        let ip = self.resolve_host_to_ip(host).await?;

        // Check if IP is private/localhost
        if self.is_private_ip(&ip) {
            return Err(AppError::Security(
                "Access to private IP addresses is forbidden".to_string(),
            ));
        }

        Ok(url)
    }

    /// Deep validation: Verify file signature without downloading entire file
    ///
    /// Downloads first 1MB and checks for valid model file signatures:
    /// - GGUF magic bytes
    /// - ONNX magic bytes
    /// - SafeTensors magic bytes
    /// - PyTorch magic bytes
    pub async fn validate_url_deep(&self, url: &Url) -> Result<(), AppError> {
        // Follow redirects safely (max 5, validate each)
        let final_url = self.follow_redirects_safely(url).await?;

        // Download first 1MB to check file signature
        let response = self
            .client
            .get(final_url.as_str())
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Failed to fetch URL: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        // Read first 1MB
        const MAX_READ_SIZE: usize = 1024 * 1024; // 1MB
        let mut bytes_read = 0;
        let mut buffer = Vec::new();

        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| AppError::Network(format!("Failed to read response: {}", e)))?;
            buffer.extend_from_slice(&chunk);
            bytes_read += chunk.len();

            if bytes_read >= MAX_READ_SIZE {
                break;
            }
        }

        // Verify file signature
        self.verify_file_signature(&buffer)?;

        Ok(())
    }

    /// Follow redirects safely, validating each destination IP
    async fn follow_redirects_safely(&self, url: &Url) -> Result<Url, AppError> {
        let mut current_url = url.clone();
        let mut redirect_count = 0;

        loop {
            // Check redirect limit
            if redirect_count >= MAX_REDIRECTS {
                return Err(AppError::ValidationFailed(format!(
                    "Too many redirects (max {})",
                    MAX_REDIRECTS
                )));
            }

            // Validate current URL
            self.validate_url_pre_save(current_url.as_str()).await?;

            // Try to fetch with HEAD request
            let response = self
                .client
                .head(current_url.as_str())
                .send()
                .await
                .map_err(|e| AppError::Network(format!("Failed to check URL: {}", e)))?;

            // Check for redirect
            if response.status().is_redirection() {
                if let Some(location) = response.headers().get("location") {
                    let location_str = location.to_str().map_err(|e| {
                        AppError::ValidationFailed(format!("Invalid redirect location: {}", e))
                    })?;

                    // Parse redirect URL (may be relative)
                    current_url = current_url.join(location_str).map_err(|e| {
                        AppError::ValidationFailed(format!("Invalid redirect URL: {}", e))
                    })?;

                    redirect_count += 1;
                    continue;
                }
            }

            // No more redirects
            break;
        }

        Ok(current_url)
    }

    /// Resolve hostname to IP address using DNS
    async fn resolve_host_to_ip(&self, host: &str) -> Result<IpAddr, AppError> {
        // Try to parse as IP address first
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(ip);
        }

        // DNS resolution
        let addrs = tokio::net::lookup_host(format!("{}:443", host))
            .await
            .map_err(|e| AppError::Network(format!("DNS resolution failed: {}", e)))?;

        // Get first IP
        let ip = addrs
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Network("No IP address found for host".to_string()))?
            .ip();

        Ok(ip)
    }

    /// Check if IP address is private (SSRF prevention)
    ///
    /// Blocks:
    /// - Private IPv4: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
    /// - Loopback: 127.0.0.0/8, ::1
    /// - Link-local: 169.254.0.0/16, fe80::/10
    fn is_private_ip(&self, ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => {
                // Loopback: 127.0.0.0/8
                if ipv4.is_loopback() {
                    return true;
                }

                // Private: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                if ipv4.is_private() {
                    return true;
                }

                // Link-local: 169.254.0.0/16
                if ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254 {
                    return true;
                }

                false
            }
            IpAddr::V6(ipv6) => {
                // Loopback: ::1
                if ipv6.is_loopback() {
                    return true;
                }

                // Link-local: fe80::/10
                if (ipv6.segments()[0] & 0xffc0) == 0xfe80 {
                    return true;
                }

                // Unique local: fc00::/7
                if (ipv6.segments()[0] & 0xfe00) == 0xfc00 {
                    return true;
                }

                false
            }
        }
    }

    /// Check if hostname is a cloud metadata endpoint
    ///
    /// Blocks:
    /// - AWS: 169.254.169.254
    /// - GCP: metadata.google.internal, metadata.google.com
    /// - Azure: 169.254.169.254
    fn is_cloud_metadata_endpoint(&self, host: &str) -> bool {
        let host_lower = host.to_lowercase();

        // AWS/Azure metadata IP
        if host_lower == "169.254.169.254" {
            return true;
        }

        // GCP metadata hostnames
        if host_lower.contains("metadata.google") {
            return true;
        }

        false
    }

    /// Verify file signature (magic bytes) to ensure it's a valid model file
    fn verify_file_signature(&self, bytes: &[u8]) -> Result<(), AppError> {
        if bytes.len() < 4 {
            return Err(AppError::ValidationFailed(
                "File too small to verify signature".to_string(),
            ));
        }

        // GGUF magic: "GGUF" (0x47 0x47 0x55 0x46)
        if bytes.len() >= 4 {
            if let Some(slice) = bytes.get(0..4) {
                if slice == b"GGUF" {
                    return Ok(());
                }
            }
        }

        // ONNX magic: Protocol Buffers (0x08 followed by field number)
        // ONNX files typically start with 0x08 0x03 or 0x08 0x07
        if bytes.len() >= 2 {
            if let (Some(&byte0), Some(&byte1)) = (bytes.first(), bytes.get(1)) {
                if byte0 == 0x08 && (byte1 == 0x03 || byte1 == 0x07 || byte1 == 0x01) {
                    return Ok(());
                }
            }
        }

        // SafeTensors magic: starts with 8-byte little-endian header size
        // We'll accept it if first 8 bytes look like a reasonable header size (< 10MB)
        if bytes.len() >= 8 {
            if let Some(header_bytes) = bytes.get(0..8) {
                if let Ok(header_array) = <[u8; 8]>::try_from(header_bytes) {
                    let header_size = u64::from_le_bytes(header_array);
                    if header_size > 0 && header_size < 10_000_000 {
                        // Likely SafeTensors
                        return Ok(());
                    }
                }
            }
        }

        // PyTorch magic: ZIP archive (0x50 0x4B 0x03 0x04) - .pt files are ZIP files
        if bytes.len() >= 4 {
            if let Some(slice) = bytes.get(0..4) {
                if slice == b"PK\x03\x04" {
                    return Ok(());
                }
            }
        }

        // If none match, reject
        Err(AppError::ValidationFailed(
            "File does not appear to be a valid model format (GGUF, ONNX, SafeTensors, or PyTorch)"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_private_ip_ipv4() {
        let service = UrlValidationService::new().unwrap();

        // Loopback
        assert!(service.is_private_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));

        // Private ranges
        assert!(service.is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(service.is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(service.is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));

        // Link-local
        assert!(service.is_private_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));

        // Public IP (should not be private)
        assert!(!service.is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn test_is_private_ip_ipv6() {
        let service = UrlValidationService::new().unwrap();

        // Loopback
        assert!(service.is_private_ip(&IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))));

        // Link-local (fe80::/10)
        assert!(service.is_private_ip(&IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1))));

        // Public IP (should not be private)
        assert!(!service.is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
        ))));
    }

    #[test]
    fn test_is_cloud_metadata_endpoint() {
        let service = UrlValidationService::new().unwrap();

        // AWS/Azure metadata IP
        assert!(service.is_cloud_metadata_endpoint("169.254.169.254"));

        // GCP metadata hostnames
        assert!(service.is_cloud_metadata_endpoint("metadata.google.internal"));
        assert!(service.is_cloud_metadata_endpoint("metadata.google.com"));

        // Normal hostname (should not be blocked)
        assert!(!service.is_cloud_metadata_endpoint("example.com"));
    }

    #[test]
    fn test_verify_file_signature_gguf() {
        let service = UrlValidationService::new().unwrap();

        // GGUF magic bytes
        let gguf_bytes = b"GGUF\x00\x00\x00\x01";
        assert!(service.verify_file_signature(gguf_bytes).is_ok());
    }

    #[test]
    fn test_verify_file_signature_pytorch() {
        let service = UrlValidationService::new().unwrap();

        // PyTorch (ZIP) magic bytes
        let pytorch_bytes = b"PK\x03\x04\x00\x00\x00\x00";
        assert!(service.verify_file_signature(pytorch_bytes).is_ok());
    }

    #[test]
    fn test_verify_file_signature_invalid() {
        let service = UrlValidationService::new().unwrap();

        // Invalid file
        let invalid_bytes = b"INVALID FILE";
        assert!(service.verify_file_signature(invalid_bytes).is_err());
    }
}
