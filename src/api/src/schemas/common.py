from __future__ import annotations

from typing import Any, Generic, TypeVar

from pydantic import BaseModel, Field


class ErrorResponse(BaseModel):
    error: str = Field(..., description="Error message")
    detail: str | None = Field(None, description="Detailed error information")
    code: str | None = Field(None, description="Error code")

    class Config:
        json_schema_extra = {
            "example": {
                "error": "File not found",
                "detail": "The requested file with ID abc123 does not exist",
                "code": "FILE_NOT_FOUND",
            }
        }


class PaginationParams(BaseModel):
    limit: int = Field(default=20, ge=1, le=100, description="Maximum number of items to return")
    offset: int = Field(default=0, ge=0, description="Number of items to skip")

    class Config:
        json_schema_extra = {"example": {"limit": 20, "offset": 0}}


T = TypeVar("T")


class PaginatedResponse(BaseModel, Generic[T]):
    items: list[T] = Field(..., description="List of items")
    total: int = Field(..., description="Total number of items available")
    limit: int = Field(..., description="Maximum items returned")
    offset: int = Field(..., description="Number of items skipped")
    has_more: bool = Field(..., description="Whether more items are available")

    @classmethod
    def create(cls, items: list[T], total: int, limit: int, offset: int):
        return cls(
            items=items,
            total=total,
            limit=limit,
            offset=offset,
            has_more=(offset + len(items)) < total,
        )


class SuccessResponse(BaseModel):
    success: bool = Field(default=True, description="Operation success status")
    message: str | None = Field(None, description="Success message")
    data: Any | None = Field(None, description="Additional data")

    class Config:
        json_schema_extra = {
            "example": {
                "success": True,
                "message": "File indexed successfully",
                "data": {"file_id": "abc123"},
            }
        }
