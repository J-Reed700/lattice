use thiserror::Error;

#[derive(Error, Debug)]
pub enum WebIngestionError {
    #[error("Invalid URL: {url}. {reason}")]
    InvalidUrl { url: String, reason: String },

    #[error("URL blocked for security reasons: {url}. Cannot fetch content from private networks.")]
    BlockedUrl { url: String },

    #[error("Failed to fetch web page: {url}. {reason}")]
    FetchError { url: String, reason: String },

    #[error("Network timeout while fetching: {url}. The server took too long to respond.")]
    Timeout { url: String },

    #[error("Too many redirects while fetching: {url}. Maximum {max} redirects allowed.")]
    TooManyRedirects { url: String, max: usize },

    #[error("Unsupported content type: {content_type}. Expected HTML or plain text.")]
    UnsupportedContentType { content_type: String },

    #[error("Failed to parse HTML from: {url}. {reason}")]
    ParseError { url: String, reason: String },

    #[error("No readable content found on page: {url}. The page may be empty or mostly generated content.")]
    NoContentFound { url: String },

    #[error("HTTP error {status}: {message} while fetching {url}")]
    HttpError {
        url: String,
        status: u16,
        message: String,
    },

    #[error("Failed to extract metadata from: {url}. {reason}")]
    MetadataError { url: String, reason: String },

    #[error("Request body too large: {size} bytes. Maximum allowed is {max_size} bytes.")]
    ContentTooLarge { size: usize, max_size: usize },

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Internal error during web ingestion: {0}")]
    InternalError(String),
}

impl WebIngestionError {
    pub fn invalid_url(url: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidUrl {
            url: url.into(),
            reason: reason.into(),
        }
    }

    pub fn blocked_url(url: impl Into<String>) -> Self {
        Self::BlockedUrl { url: url.into() }
    }

    pub fn fetch_error(url: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::FetchError {
            url: url.into(),
            reason: reason.into(),
        }
    }

    pub fn timeout(url: impl Into<String>) -> Self {
        Self::Timeout { url: url.into() }
    }

    pub fn too_many_redirects(url: impl Into<String>, max: usize) -> Self {
        Self::TooManyRedirects {
            url: url.into(),
            max,
        }
    }

    pub fn unsupported_content_type(content_type: impl Into<String>) -> Self {
        Self::UnsupportedContentType {
            content_type: content_type.into(),
        }
    }

    pub fn parse_error(url: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ParseError {
            url: url.into(),
            reason: reason.into(),
        }
    }

    pub fn no_content_found(url: impl Into<String>) -> Self {
        Self::NoContentFound { url: url.into() }
    }

    pub fn http_error(url: impl Into<String>, status: u16, message: impl Into<String>) -> Self {
        Self::HttpError {
            url: url.into(),
            status,
            message: message.into(),
        }
    }

    pub fn metadata_error(url: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::MetadataError {
            url: url.into(),
            reason: reason.into(),
        }
    }

    pub fn content_too_large(size: usize, max_size: usize) -> Self {
        Self::ContentTooLarge { size, max_size }
    }

    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError(message.into())
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::InternalError(message.into())
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout { .. } | Self::FetchError { .. } | Self::HttpError { status, .. } if *status >= 500
        )
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::InvalidUrl { url, .. } => {
                format!("The URL '{}' is not valid. Please check the URL and try again.", url)
            }
            Self::BlockedUrl { url } => {
                format!("Cannot access '{}' for security reasons. This appears to be a private network address.", url)
            }
            Self::Timeout { url } => {
                format!("The request to '{}' timed out. Please try again later.", url)
            }
            Self::HttpError { status: 404, url, .. } => {
                format!("The page at '{}' was not found.", url)
            }
            Self::HttpError { status: 403, url, .. } => {
                format!("Access to '{}' was denied. The website may require authentication.", url)
            }
            Self::HttpError { status, url, .. } if *status >= 500 => {
                format!("The server at '{}' is experiencing issues. Please try again later.", url)
            }
            Self::NoContentFound { url } => {
                format!("Could not extract readable content from '{}'. The page may be empty or dynamically generated.", url)
            }
            Self::UnsupportedContentType { content_type } => {
                format!("This type of content ({}) is not supported. Only HTML and text pages can be processed.", content_type)
            }
            _ => self.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, WebIngestionError>;

impl From<reqwest::Error> for WebIngestionError {
    fn from(err: reqwest::Error) -> Self {
        let url = err.url().map(|u| u.to_string()).unwrap_or_else(|| "unknown".to_string());

        if err.is_timeout() {
            Self::timeout(url)
        } else if err.is_redirect() {
            Self::too_many_redirects(url, 10)
        } else if err.is_status() {
            if let Some(status) = err.status() {
                Self::http_error(url, status.as_u16(), status.canonical_reason().unwrap_or("Unknown error"))
            } else {
                Self::fetch_error(url, err.to_string())
            }
        } else {
            Self::fetch_error(url, err.to_string())
        }
    }
}

impl From<url::ParseError> for WebIngestionError {
    fn from(err: url::ParseError) -> Self {
        Self::invalid_url("", err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_messages() {
        let err = WebIngestionError::timeout("https://example.com");
        assert!(err.to_string().contains("timeout"));
        assert!(err.user_message().contains("timed out"));
    }

    #[test]
    fn test_retryable() {
        assert!(WebIngestionError::timeout("https://example.com").is_retryable());
        assert!(WebIngestionError::http_error("https://example.com", 503, "Service Unavailable").is_retryable());
        assert!(!WebIngestionError::http_error("https://example.com", 404, "Not Found").is_retryable());
    }
}
