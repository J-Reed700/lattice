"""
Link Extractor - Type Definitions

Data types for representing bidirectional wikilinks.
"""

from dataclasses import dataclass
from uuid import UUID


@dataclass
class Backlink:
    """Represents a backlink from another document."""

    source_path: str
    source_title: str
    source_document_id: UUID
    line_number: int
    context: str
    link_text: str


@dataclass
class ForwardLink:
    """Represents an outgoing link from a document."""

    target_path: str | None
    target_title: str | None
    target_document_id: UUID | None
    display_text: str | None
    header: str | None
    line_number: int
    context: str
    link_text: str
    is_resolved: bool
