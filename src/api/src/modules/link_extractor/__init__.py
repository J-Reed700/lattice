"""
Module: Link Extractor

A self-contained module for extracting and managing bidirectional wikilinks
in markdown documents.

Features:
- Parse [[wikilinks]] from markdown content
- Resolve links to actual documents
- Track backlinks (who links to this document)
- Track forward links (what this document links to)
- Find unlinked mentions
- Generate link graphs for visualization

Basic Usage:
    >>> from modules.link_extractor import LinksService
    >>> service = LinksService(db_session, user_id)
    >>> link_count = await service.extract_links(doc_id, content, file_path)
    >>> backlinks = await service.get_backlinks(doc_id)
    >>> forward_links = await service.get_forward_links(doc_id)

Parser Usage (without database):
    >>> from modules.link_extractor import LinkParser
    >>> parser = LinkParser()
    >>> links = parser.parse_document(content, file_path)
    >>> title = parser.extract_title(content)

Domain Models:
    >>> from modules.link_extractor import Backlink, ForwardLink
"""

from .parser import LinkParser
from .service import LinksService
from .types import Backlink, ForwardLink

__all__ = [
    "Backlink",
    "ForwardLink",
    "LinkParser",
    "LinksService",
]
