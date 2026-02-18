"""Data export module for Recall/Vault.

This module provides comprehensive data export functionality allowing users
to export their knowledge base in multiple formats with various scopes.

Public Interface:
    - ExportService: Main service for coordinating exports
    - ExportFormat: Enum of supported export formats
    - ExportScope: Enum of export scope types
    - ExportRequest: Request model for exports
    - ExportResult: Result model with export metadata

Example:
    >>> from modules.exporter import ExportService, ExportFormat, ExportScope
    >>> service = ExportService(session)
    >>> result = await service.export_data(
    ...     format=ExportFormat.JSON,
    ...     scope=ExportScope.FULL
    ... )
"""

from .handlers import CSVExportHandler, JSONExportHandler, MarkdownExportHandler, ZIPExportHandler
from .service import ExportService
from .types import ExportFormat, ExportJob, ExportRequest, ExportResult, ExportScope, ExportStatus

__all__ = [
    "CSVExportHandler",
    "ExportFormat",
    "ExportJob",
    "ExportRequest",
    "ExportResult",
    "ExportScope",
    "ExportService",
    "ExportStatus",
    "JSONExportHandler",
    "MarkdownExportHandler",
    "ZIPExportHandler",
]
