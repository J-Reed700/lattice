"""Pydantic schemas for document operations."""

from __future__ import annotations

from pydantic import BaseModel, Field


class DocumentBase(BaseModel):
    """Base document information."""

    document_id: str
    file_path: str
    filename: str
    extension: str
    mime_type: str | None = None
    size_bytes: int | None = None
    file_modified_at: int | None = None


class DocumentAccessResponse(DocumentBase):
    """Response for document access record."""

    access_id: str
    user_id: str
    accessed_at: int
    access_count: int
    is_favorite: bool


class FavoriteResponse(DocumentBase):
    """Response for favorite document."""

    favorite_id: str
    user_id: str
    favorited_at: int
    note: str | None = None
    access_count: int | None = 0
    last_accessed_at: int | None = None


class RecentDocumentsResponse(BaseModel):
    """Response containing recent documents."""

    documents: list[DocumentAccessResponse]
    total: int


class FavoritesResponse(BaseModel):
    """Response containing favorite documents."""

    documents: list[FavoriteResponse]
    total: int


class FavoriteRequest(BaseModel):
    """Request to favorite a document."""

    note: str | None = Field(None, max_length=500)
