"""Command-line interface for Vault Backend.

This module provides a CLI for managing the knowledge base, including:
- Initializing the database
- Indexing documents
- Searching and Q&A
- Watching directories for changes
- System status and configuration
"""

from __future__ import annotations

from src.cli.main import app, main

__all__ = ["app", "main"]
