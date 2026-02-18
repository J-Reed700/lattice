"""
Rate Limit Configurations

Define all rate limits in one place for easy management.
"""

from dataclasses import dataclass


@dataclass
class RateLimitConfig:
    """Rate limit configuration"""

    limit: int
    window_seconds: int

    def as_string(self) -> str:
        """Convert to slowapi format (e.g., '5/minute')"""
        if self.window_seconds == 60:
            return f"{self.limit}/minute"
        if self.window_seconds == 3600:
            return f"{self.limit}/hour"
        if self.window_seconds == 86400:
            return f"{self.limit}/day"
        return f"{self.limit}/{self.window_seconds} seconds"


LOGIN_IP_LIMIT = RateLimitConfig(5, 60)
LOGIN_USERNAME_LIMIT = RateLimitConfig(10, 3600)
REGISTER_LIMIT = RateLimitConfig(3, 3600)
PASSWORD_RESET_LIMIT = RateLimitConfig(3, 3600)

FILE_UPLOAD_LIMIT = RateLimitConfig(10, 60)
FILE_DOWNLOAD_LIMIT = RateLimitConfig(100, 60)
FILE_DELETE_LIMIT = RateLimitConfig(20, 60)

SEARCH_LIMIT = RateLimitConfig(60, 60)

EXPORT_LIMIT = RateLimitConfig(5, 3600)

OCR_LIMIT = RateLimitConfig(20, 60)

SUMMARIZE_LIMIT = RateLimitConfig(10, 60)
