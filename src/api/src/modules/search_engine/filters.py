"""
Search Filters

Filter criteria for search results with SQL and post-filter support.
"""

from dataclasses import dataclass
from datetime import datetime
from typing import Any


@dataclass
class SearchFilters:
    """Filter criteria for search results.

    Attributes:
        file_types: List of file extensions to include (e.g., ['.txt', '.pdf'])
        date_from: Minimum modification date
        date_to: Maximum modification date
        min_size: Minimum file size in bytes
        max_size: Maximum file size in bytes
        tags: List of required tags
        folders: List of folder paths to search within

    Example:
        >>> filters = SearchFilters(
        ...     file_types=['.txt', '.md'],
        ...     date_from=datetime(2024, 1, 1),
        ...     min_size=1024
        ... )
    """

    file_types: list[str] | None = None
    date_from: datetime | None = None
    date_to: datetime | None = None
    min_size: int | None = None
    max_size: int | None = None
    tags: list[str] | None = None
    folders: list[str] | None = None

    def to_sql_filters(self) -> dict[str, Any]:
        """Convert to SQLAlchemy WHERE clause filters.

        Returns:
            Dictionary of field names to filter values for SQL queries

        Example:
            >>> filters.to_sql_filters()
            {
                "file_type": ['.txt', '.md'],
                "modified_date >= ": datetime(2024, 1, 1),
                "size >= ": 1024
            }
        """
        sql_filters = {}

        if self.file_types:
            sql_filters["file_type"] = self.file_types

        if self.date_from:
            sql_filters["modified_date >= "] = self.date_from

        if self.date_to:
            sql_filters["modified_date <= "] = self.date_to

        if self.min_size is not None:
            sql_filters["size >= "] = self.min_size

        if self.max_size is not None:
            sql_filters["size <= "] = self.max_size

        if self.folders:
            sql_filters["folder_path"] = self.folders

        return sql_filters

    def apply(self, results: list[Any]) -> list[Any]:
        """Post-filter results that couldn't be filtered at query time.

        Args:
            results: List of SearchResult objects

        Returns:
            Filtered list of SearchResult objects

        Example:
            >>> filtered = filters.apply(search_results)
        """
        filtered = results

        if self.file_types:
            filtered = [
                r for r in filtered if any(r.file_path.endswith(ft) for ft in self.file_types)
            ]

        if self.date_from:
            filtered = [
                r
                for r in filtered
                if r.metadata.get("modified_date") and r.metadata["modified_date"] >= self.date_from
            ]

        if self.date_to:
            filtered = [
                r
                for r in filtered
                if r.metadata.get("modified_date") and r.metadata["modified_date"] <= self.date_to
            ]

        if self.min_size is not None:
            filtered = [r for r in filtered if r.metadata.get("size", 0) >= self.min_size]

        if self.max_size is not None:
            filtered = [r for r in filtered if r.metadata.get("size", 0) <= self.max_size]

        if self.tags:
            filtered = [
                r for r in filtered if all(tag in r.metadata.get("tags", []) for tag in self.tags)
            ]

        if self.folders:
            filtered = [
                r
                for r in filtered
                if any(r.file_path.startswith(folder) for folder in self.folders)
            ]

        return filtered
