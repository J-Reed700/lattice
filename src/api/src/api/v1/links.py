"""
Links API endpoints for bidirectional wikilinks support.
"""

from __future__ import annotations

from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel, Field
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.modules.link_extractor import LinksService

router = APIRouter(prefix="/links", tags=["links"])


class WikiLinkResponse(BaseModel):
    """Response model for wikilink."""

    id: UUID
    source_document_id: UUID
    target_document_id: UUID | None
    target_text: str
    display_text: str | None
    header: str | None
    line_number: int
    context: str
    is_resolved: bool


class BacklinkResponse(BaseModel):
    """Response model for backlink."""

    source_path: str
    source_title: str
    source_document_id: UUID
    line_number: int
    context: str
    link_text: str


class ForwardLinkResponse(BaseModel):
    """Response model for forward link."""

    target_path: str | None
    target_title: str | None
    target_document_id: UUID | None
    display_text: str | None
    header: str | None
    line_number: int
    context: str
    link_text: str
    is_resolved: bool


class UnlinkedMentionResponse(BaseModel):
    """Response model for unlinked mention."""

    file_path: str
    line_number: int
    context: str
    matched_text: str


class GraphNodeResponse(BaseModel):
    """Response model for graph node."""

    id: UUID
    title: str
    path: str
    depth: int


class GraphEdgeResponse(BaseModel):
    """Response model for graph edge."""

    source: UUID
    target: UUID
    type: str


class LinkGraphResponse(BaseModel):
    """Response model for link graph."""

    nodes: list[GraphNodeResponse]
    edges: list[GraphEdgeResponse]


class ExtractLinksRequest(BaseModel):
    """Request model for extracting links."""

    document_id: UUID = Field(..., description="Document ID to extract links from")
    content: str = Field(..., description="Document content")
    file_path: str = Field(..., description="File path")


class ExtractLinksResponse(BaseModel):
    """Response model for link extraction."""

    status: str
    links_extracted: int


@router.post("/extract", response_model=ExtractLinksResponse)
async def extract_links(
    request: ExtractLinksRequest,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
):
    """Extract and store wikilinks from document content.

    This endpoint parses the document content for [[wikilinks]] and stores them
    in the database. Links are resolved to actual documents when possible.

    Args:
        request: Link extraction request
        current_user: Current authenticated user
        db: Database session

    Returns:
        Number of links extracted
    """
    try:
        links_service = LinksService(db, str(current_user.id))
        link_count = await links_service.extract_links(
            document_id=request.document_id, content=request.content, file_path=request.file_path
        )

        return ExtractLinksResponse(status="success", links_extracted=link_count)

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{document_id}/backlinks", response_model=list[BacklinkResponse])
async def get_backlinks(
    document_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get all documents that link to this document.

    Returns backlinks with context showing where the link appears.

    Args:
        document_id: Target document ID
        current_user: Current authenticated user
        db: Database session

    Returns:
        List of backlinks
    """
    try:
        links_service = LinksService(db, str(current_user.id))
        backlinks = await links_service.get_backlinks(document_id)

        return [
            BacklinkResponse(
                source_path=bl.source_path,
                source_title=bl.source_title,
                source_document_id=bl.source_document_id,
                line_number=bl.line_number,
                context=bl.context,
                link_text=bl.link_text,
            )
            for bl in backlinks
        ]

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{document_id}/forward-links", response_model=list[ForwardLinkResponse])
async def get_forward_links(
    document_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get all links from this document to other documents.

    Returns forward links showing what this document links to.

    Args:
        document_id: Source document ID
        current_user: Current authenticated user
        db: Database session

    Returns:
        List of forward links
    """
    try:
        links_service = LinksService(db, str(current_user.id))
        forward_links = await links_service.get_forward_links(document_id)

        return [
            ForwardLinkResponse(
                target_path=link.target_path,
                target_title=link.target_title,
                target_document_id=link.target_document_id,
                display_text=link.display_text,
                header=link.header,
                line_number=link.line_number,
                context=link.context,
                link_text=link.link_text,
                is_resolved=link.is_resolved,
            )
            for link in forward_links
        ]

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{document_id}/unlinked-mentions", response_model=list[UnlinkedMentionResponse])
async def get_unlinked_mentions(
    document_id: UUID,
    limit: int = Query(50, ge=1, le=200, description="Maximum mentions to return"),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Find mentions of document title that aren't wikilinks.

    Searches for occurrences of the document's title in other documents
    where the mention is NOT already a [[wikilink]].

    Args:
        document_id: Document to search for mentions of
        limit: Maximum number of mentions to return
        current_user: Current authenticated user
        db: Database session

    Returns:
        List of unlinked mentions
    """
    try:
        links_service = LinksService(db, str(current_user.id))
        mentions = await links_service.get_unlinked_mentions(document_id=document_id, limit=limit)

        return [
            UnlinkedMentionResponse(
                file_path=m["file_path"],
                line_number=m["line_number"],
                context=m["context"],
                matched_text=m["matched_text"],
            )
            for m in mentions
        ]

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{document_id}/graph", response_model=LinkGraphResponse)
async def get_link_graph(
    document_id: UUID,
    depth: int = Query(2, ge=1, le=3, description="Traversal depth for graph"),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get link graph for visualization.

    Traverses links up to specified depth and returns a graph structure
    suitable for visualization with D3.js or similar libraries.

    Args:
        document_id: Center document ID
        depth: How many levels to traverse (1-3)
        current_user: Current authenticated user
        db: Database session

    Returns:
        Graph with nodes and edges
    """
    try:
        links_service = LinksService(db, str(current_user.id))
        graph = await links_service.get_link_graph(document_id=document_id, depth=depth)

        return LinkGraphResponse(
            nodes=[
                GraphNodeResponse(
                    id=node["id"], title=node["title"], path=node["path"], depth=node["depth"]
                )
                for node in graph["nodes"]
            ],
            edges=[
                GraphEdgeResponse(source=edge["source"], target=edge["target"], type=edge["type"])
                for edge in graph["edges"]
            ],
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/update-unresolved")
async def update_unresolved_links(
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
):
    """Re-resolve links that had no target when created.

    This should be called after indexing new documents to resolve
    previously unresolved links.

    Args:
        current_user: Current authenticated user
        db: Database session

    Returns:
        Number of links resolved
    """
    try:
        links_service = LinksService(db, str(current_user.id))
        resolved_count = await links_service.update_unresolved_links()

        return {"status": "success", "resolved_count": resolved_count}

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
