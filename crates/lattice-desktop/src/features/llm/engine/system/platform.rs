//! Platform detection and OS-specific utilities.

use serde::{Deserialize, Serialize};

/// Supported operating system platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
}

impl Platform {
    /// Detect the current platform.
    pub fn detect() -> Self {
        #[cfg(target_os = "macos")]
        return Platform::MacOS;

        #[cfg(target_os = "linux")]
        return Platform::Linux;

        #[cfg(target_os = "windows")]
        return Platform::Windows;
    }

    /// Check if the platform supports Metal (Apple).
    pub fn supports_metal(&self) -> bool {
        matches!(self, Platform::MacOS)
    }

    /// Check if the platform typically supports CUDA (NVIDIA).
    pub fn can_support_cuda(&self) -> bool {
        matches!(self, Platform::Linux | Platform::Windows)
    }

    /// Get platform name as string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::MacOS => "macos",
            Platform::Linux => "linux",
            Platform::Windows => "windows",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::detect();
        #[cfg(target_os = "macos")]
        assert_eq!(platform, Platform::MacOS);

        #[cfg(target_os = "linux")]
        assert_eq!(platform, Platform::Linux);

        #[cfg(target_os = "windows")]
        assert_eq!(platform, Platform::Windows);
    }

    #[test]
    fn test_metal_support() {
        #[cfg(target_os = "macos")]
        assert!(Platform::MacOS.supports_metal());

        #[cfg(not(target_os = "macos"))]
        assert!(!Platform::detect().supports_metal());
    }
}
