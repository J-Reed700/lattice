"""API utilities."""

from .hateoas import (
    generate_document_links,
    generate_file_links,
    generate_file_list_links,
    generate_search_links,
)

__all__ = [
    "generate_document_links",
    "generate_file_links",
    "generate_file_list_links",
    "generate_search_links",
]
