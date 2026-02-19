from dataclasses import dataclass, replace
from datetime import datetime
from uuid import UUID as PyUUID


@dataclass(frozen=True)
class SearchResult:
    id: PyUUID
    file_path: str
    filename: str
    mime_type: str
    size_bytes: int
    modified_at: datetime
    score: float
    snippet: str | None = None
    thumbnail_url: str | None = None
    extension: str | None = None

    def with_score(self, new_score: float) -> "SearchResult":
        return replace(self, score=new_score)

    def with_snippet(self, new_snippet: str | None) -> "SearchResult":
        return replace(self, snippet=new_snippet)

    @classmethod
    def from_file(
        cls,
        file_id: PyUUID,
        file_path: str,
        filename: str,
        mime_type: str,
        size_bytes: int,
        modified_at: datetime,
        score: float,
        snippet: str | None = None,
        thumbnail_url: str | None = None,
        extension: str | None = None,
    ) -> "SearchResult":
        return cls(
            id=file_id,
            file_path=file_path,
            filename=filename,
            mime_type=mime_type,
            size_bytes=size_bytes,
            modified_at=modified_at,
            score=score,
            snippet=snippet,
            thumbnail_url=thumbnail_url,
            extension=extension,
        )


@dataclass
class SearchResults:
    results: list[SearchResult]
    total: int
    page: int
    page_size: int
    has_more: bool

    @classmethod
    def from_results(
        cls,
        results: list[SearchResult],
        total: int,
        offset: int,
        limit: int,
    ) -> "SearchResults":
        page = (offset // limit) + 1 if limit > 0 else 1
        has_more = (offset + len(results)) < total

        return cls(
            results=results,
            total=total,
            page=page,
            page_size=limit,
            has_more=has_more,
        )

    def sort_by_score(self, descending: bool = True) -> "SearchResults":
        sorted_results = sorted(self.results, key=lambda r: r.score, reverse=descending)
        return SearchResults(
            results=sorted_results,
            total=self.total,
            page=self.page,
            page_size=self.page_size,
            has_more=self.has_more,
        )

    def sort_by_date(self, descending: bool = True) -> "SearchResults":
        sorted_results = sorted(self.results, key=lambda r: r.modified_at, reverse=descending)
        return SearchResults(
            results=sorted_results,
            total=self.total,
            page=self.page,
            page_size=self.page_size,
            has_more=self.has_more,
        )
