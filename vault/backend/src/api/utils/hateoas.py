"""HATEOAS link generation utilities."""

from uuid import UUID


def generate_file_links(file_id: UUID) -> dict[str, str]:
    """Generate HATEOAS links for a file resource."""
    file_id_str = str(file_id)
    return {
        "self": f"/api/v1/files/{file_id_str}",
        "download": f"/api/v1/files/{file_id_str}/download",
        "content": f"/api/v1/files/{file_id_str}/content",
        "thumbnail": f"/api/v1/files/{file_id_str}/thumbnail",
    }


def generate_search_links(query: str, offset: int, limit: int, total: int) -> dict[str, str]:
    """Generate HATEOAS links for search results."""
    from urllib.parse import quote_plus

    encoded_query = quote_plus(query)
    links = {
        "self": f"/api/v1/search?query={encoded_query}&offset={offset}&limit={limit}",
    }

    # Add next link if there are more results
    if offset + limit < total:
        next_offset = offset + limit
        links["next"] = f"/api/v1/search?query={encoded_query}&offset={next_offset}&limit={limit}"

    # Add previous link if we're not on the first page
    if offset > 0:
        prev_offset = max(0, offset - limit)
        links["prev"] = f"/api/v1/search?query={encoded_query}&offset={prev_offset}&limit={limit}"

    # Add first and last links
    if total > 0:
        links["first"] = f"/api/v1/search?query={encoded_query}&offset=0&limit={limit}"
        last_offset = ((total - 1) // limit) * limit
        links["last"] = f"/api/v1/search?query={encoded_query}&offset={last_offset}&limit={limit}"

    return links


def generate_file_list_links(offset: int, limit: int, total: int, **filters) -> dict[str, str]:
    """Generate HATEOAS links for file list pagination."""
    filter_params = "&".join(f"{k}={v}" for k, v in filters.items() if v is not None)
    base = f"/api/v1/files?{filter_params}&" if filter_params else "/api/v1/files?"

    links = {
        "self": f"{base}offset={offset}&limit={limit}",
    }

    # Add next link if there are more results
    if offset + limit < total:
        next_offset = offset + limit
        links["next"] = f"{base}offset={next_offset}&limit={limit}"

    # Add previous link if we're not on the first page
    if offset > 0:
        prev_offset = max(0, offset - limit)
        links["prev"] = f"{base}offset={prev_offset}&limit={limit}"

    # Add first and last links
    if total > 0:
        links["first"] = f"{base}offset=0&limit={limit}"
        last_offset = ((total - 1) // limit) * limit
        links["last"] = f"{base}offset={last_offset}&limit={limit}"

    return links


def generate_document_links(document_id: str) -> dict[str, str]:
    """Generate HATEOAS links for a document resource."""
    return {
        "self": f"/api/v1/documents/{document_id}",
        "favorite": f"/api/v1/documents/{document_id}/favorite",
        "access": f"/api/v1/documents/{document_id}/access",
    }
