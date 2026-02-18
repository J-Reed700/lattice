"""Event subscribers for cross-cutting concerns.

This module contains event subscribers that handle:
- Cache invalidation
- Audit logging
- Metrics tracking
"""

from __future__ import annotations

from .audit_log import setup_audit_logging
from .cache_invalidation import setup_cache_invalidation
from .metrics import setup_metrics

__all__ = [
    "setup_cache_invalidation",
    "setup_audit_logging",
    "setup_metrics",
]
