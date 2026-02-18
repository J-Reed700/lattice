"""
Link Extractor - Service

Service for managing bidirectional wikilinks in markdown documents.
"""

from pathlib import Path
from typing import Any
from uuid import UUID

from sqlalchemy import delete, text
from sqlalchemy.ext.asyncio import AsyncSession

from .parser import LinkParser
from .types import Backlink, ForwardLink


class LinksService:
    """Service for managing bidirectional wikilinks."""

    def __init__(self, db: AsyncSession, user_id: str):
        """Initialize the links service.

        Args:
            db: Database session
            user_id: Current user ID
        """
        self.db = db
        self.user_id = user_id
        self.parser = LinkParser()

    async def extract_links(self, document_id: UUID, content: str, file_path: str) -> int:
        """Extract and store links from a document.

        Args:
            document_id: ID of the source document
            content: Document content
            file_path: Path to the document

        Returns:
            Number of links extracted
        """
        links = self.parser.parse_document(content, file_path)

        if not links:
            return 0

        await self.db.execute(
            delete(text("links")).where(text("source_document_id = :doc_id")),
            {"doc_id": document_id},
        )

        all_docs = await self._get_all_document_paths()

        for link in links:
            target_path = self._resolve_link(link["target"], file_path, all_docs)

            target_id = None
            if target_path:
                target_id = await self._get_document_id_by_path(target_path)

            await self.db.execute(
                text(
                    """
                    INSERT INTO links (
                        user_id,
                        source_document_id,
                        target_document_id,
                        target_text,
                        display_text,
                        header,
                        line_number,
                        context
                    ) VALUES (
                        :user_id,
                        :source_id,
                        :target_id,
                        :target_text,
                        :display_text,
                        :header,
                        :line_number,
                        :context
                    )
                """
                ),
                {
                    "user_id": self.user_id,
                    "source_id": document_id,
                    "target_id": target_id,
                    "target_text": link["target"],
                    "display_text": link["display_text"],
                    "header": link["header"],
                    "line_number": link["line_number"],
                    "context": link["context"],
                },
            )

        await self.db.commit()
        return len(links)

    async def get_backlinks(self, document_id: UUID) -> list[Backlink]:
        """Get all documents that link to this document.

        Args:
            document_id: Target document ID

        Returns:
            List of backlinks
        """
        result = await self.db.execute(
            text(
                """
                SELECT
                    f.path as source_path,
                    tc.content as source_content,
                    l.line_number,
                    l.context,
                    l.target_text as link_text,
                    f.id as source_document_id
                FROM links l
                JOIN files f ON l.source_document_id = f.id
                LEFT JOIN text_content tc ON f.id = tc.file_id
                WHERE l.target_document_id = :doc_id
                AND l.user_id = :user_id
                ORDER BY f.modified_at DESC
            """
            ),
            {"doc_id": document_id, "user_id": self.user_id},
        )

        rows = result.fetchall()

        backlinks = []
        for row in rows:
            title = (
                self.parser.extract_title(row.source_content or "") or Path(row.source_path).stem
            )

            backlinks.append(
                Backlink(
                    source_path=row.source_path,
                    source_title=title,
                    source_document_id=row.source_document_id,
                    line_number=row.line_number,
                    context=row.context,
                    link_text=row.link_text,
                )
            )

        return backlinks

    async def get_forward_links(self, document_id: UUID) -> list[ForwardLink]:
        """Get all links FROM this document.

        Args:
            document_id: Source document ID

        Returns:
            List of forward links
        """
        result = await self.db.execute(
            text(
                """
                SELECT
                    l.target_text,
                    l.display_text,
                    l.header,
                    l.line_number,
                    l.context,
                    l.target_document_id,
                    f.path as target_path,
                    tc.content as target_content
                FROM links l
                LEFT JOIN files f ON l.target_document_id = f.id
                LEFT JOIN text_content tc ON f.id = tc.file_id
                WHERE l.source_document_id = :doc_id
                AND l.user_id = :user_id
                ORDER BY l.line_number
            """
            ),
            {"doc_id": document_id, "user_id": self.user_id},
        )

        rows = result.fetchall()

        forward_links = []
        for row in rows:
            is_resolved = row.target_document_id is not None
            target_title = None

            if is_resolved and row.target_content:
                target_title = (
                    self.parser.extract_title(row.target_content) or Path(row.target_path).stem
                )

            forward_links.append(
                ForwardLink(
                    target_path=row.target_path,
                    target_title=target_title,
                    target_document_id=row.target_document_id,
                    display_text=row.display_text,
                    header=row.header,
                    line_number=row.line_number,
                    context=row.context,
                    link_text=row.target_text,
                    is_resolved=is_resolved,
                )
            )

        return forward_links

    async def get_unlinked_mentions(
        self, document_id: UUID, limit: int = 50
    ) -> list[dict[str, Any]]:
        """Find mentions of document title that aren't wikilinks.

        Args:
            document_id: Document to search for mentions of
            limit: Maximum number of mentions to return

        Returns:
            List of mention dicts
        """
        doc_result = await self.db.execute(
            text(
                """
                SELECT tc.content
                FROM text_content tc
                WHERE tc.file_id = :doc_id
            """
            ),
            {"doc_id": document_id},
        )

        doc_row = doc_result.fetchone()
        if not doc_row:
            return []

        title = self.parser.extract_title(doc_row.content)
        if not title or len(title) < 3:
            return []

        title_lower = title.lower()

        result = await self.db.execute(
            text(
                """
                SELECT f.id, f.path, tc.content
                FROM files f
                JOIN text_content tc ON f.id = tc.file_id
                WHERE f.id != :doc_id
                AND LOWER(tc.content) LIKE :title_pattern
                AND f.electric_user_id = :user_id
                LIMIT :limit
            """
            ),
            {
                "doc_id": document_id,
                "title_pattern": f"%{title_lower}%",
                "user_id": self.user_id,
                "limit": limit * 2,
            },
        )

        rows = result.fetchall()

        mentions = []
        for row in rows:
            already_linked = await self._has_link_to_document(row.id, document_id)
            if already_linked:
                continue

            content = row.content or ""
            lines = content.split("\n")

            for line_num, line in enumerate(lines, 1):
                if title_lower in line.lower():
                    if f"[[{title}]]" in line or f"[[{title.lower()}]]" in line.lower():
                        continue

                    start = max(0, line_num - 2)
                    end = min(len(lines), line_num + 1)
                    context_lines = lines[start:end]
                    context = "\n".join(context_lines)

                    mentions.append(
                        {
                            "file_path": row.path,
                            "line_number": line_num,
                            "context": context,
                            "matched_text": title,
                        }
                    )

                    if len(mentions) >= limit:
                        break

            if len(mentions) >= limit:
                break

        return mentions

    async def get_link_graph(self, document_id: UUID, depth: int = 2) -> dict[str, Any]:
        """Get link graph for visualization.

        Args:
            document_id: Center document ID
            depth: How many levels to traverse (1-3)

        Returns:
            Dict with 'nodes' and 'edges'
        """
        if depth < 1 or depth > 3:
            depth = 2

        nodes = {}
        edges = []
        visited = set()

        await self._traverse_links(document_id, depth, nodes, edges, visited)

        return {"nodes": list(nodes.values()), "edges": edges}

    async def update_unresolved_links(self) -> int:
        """Re-resolve links that had no target when created.

        Returns:
            Number of links resolved
        """
        result = await self.db.execute(
            text(
                """
                SELECT l.id, l.source_document_id, l.target_text, f.path as source_path
                FROM links l
                JOIN files f ON l.source_document_id = f.id
                WHERE l.target_document_id IS NULL
                AND l.user_id = :user_id
            """
            ),
            {"user_id": self.user_id},
        )

        unresolved = result.fetchall()

        if not unresolved:
            return 0

        all_docs = await self._get_all_document_paths()
        resolved_count = 0

        for link in unresolved:
            target_path = self._resolve_link(link.target_text, link.source_path, all_docs)

            if target_path:
                target_id = await self._get_document_id_by_path(target_path)
                if target_id:
                    await self.db.execute(
                        text(
                            """
                            UPDATE links
                            SET target_document_id = :target_id
                            WHERE id = :link_id
                        """
                        ),
                        {"target_id": target_id, "link_id": link.id},
                    )
                    resolved_count += 1

        await self.db.commit()
        return resolved_count

    async def _get_all_document_paths(self) -> list[dict[str, Any]]:
        """Get all document paths and titles for link resolution."""
        result = await self.db.execute(
            text(
                """
                SELECT f.path as file_path, tc.content
                FROM files f
                LEFT JOIN text_content tc ON f.id = tc.file_id
                WHERE f.electric_user_id = :user_id
            """
            ),
            {"user_id": self.user_id},
        )

        rows = result.fetchall()

        docs = []
        for row in rows:
            title = self.parser.extract_title(row.content or "")
            docs.append({"file_path": row.file_path, "title": title})

        return docs

    async def _get_document_id_by_path(self, file_path: str) -> UUID | None:
        """Get document ID by file path."""
        result = await self.db.execute(
            text(
                """
                SELECT id FROM files
                WHERE path = :path
                AND electric_user_id = :user_id
            """
            ),
            {"path": file_path, "user_id": self.user_id},
        )

        row = result.fetchone()
        return row.id if row else None

    def _resolve_link(
        self, target: str, source_path: str, all_documents: list[dict[str, Any]]
    ) -> str | None:
        """Resolve a link target to actual file path."""
        if not all_documents:
            return None

        target_lower = target.lower()
        source_dir = str(Path(source_path).parent)

        for doc in all_documents:
            file_path = doc["file_path"]
            file_path_lower = file_path.lower()

            if file_path_lower == target_lower:
                return file_path

            if file_path_lower == f"{target_lower}.md":
                return file_path

            if file_path_lower.endswith(f"/{target_lower}") or file_path_lower.endswith(
                f"/{target_lower}.md"
            ):
                return file_path

            relative_path = str(Path(source_dir) / target)
            if (
                file_path_lower == relative_path.lower()
                or file_path_lower == f"{relative_path}.md".lower()
            ):
                return file_path

        for doc in all_documents:
            title = doc.get("title", "")
            if title and title.lower() == target_lower:
                return doc["file_path"]

        for doc in all_documents:
            file_path = doc["file_path"]
            file_name = Path(file_path).stem.lower()

            if target_lower in file_name or file_name in target_lower:
                return file_path

        return None

    async def _has_link_to_document(
        self, source_document_id: UUID, target_document_id: UUID
    ) -> bool:
        """Check if source document has a link to target document."""
        result = await self.db.execute(
            text(
                """
                SELECT COUNT(*) as count
                FROM links
                WHERE source_document_id = :source_id
                AND target_document_id = :target_id
            """
            ),
            {"source_id": source_document_id, "target_id": target_document_id},
        )

        row = result.fetchone()
        return row.count > 0

    async def _traverse_links(
        self,
        document_id: UUID,
        depth: int,
        nodes: dict[UUID, dict],
        edges: list[dict],
        visited: set,
        current_depth: int = 0,
    ) -> None:
        """Recursively traverse links to build graph."""
        if current_depth >= depth or document_id in visited:
            return

        visited.add(document_id)

        doc_result = await self.db.execute(
            text(
                """
                SELECT f.path, tc.content
                FROM files f
                LEFT JOIN text_content tc ON f.id = tc.file_id
                WHERE f.id = :doc_id
            """
            ),
            {"doc_id": document_id},
        )

        doc_row = doc_result.fetchone()
        if not doc_row:
            return

        title = self.parser.extract_title(doc_row.content or "") or Path(doc_row.path).stem

        nodes[document_id] = {
            "id": document_id,
            "title": title,
            "path": doc_row.path,
            "depth": current_depth,
        }

        outgoing_result = await self.db.execute(
            text(
                """
                SELECT target_document_id
                FROM links
                WHERE source_document_id = :doc_id
                AND target_document_id IS NOT NULL
                AND user_id = :user_id
            """
            ),
            {"doc_id": document_id, "user_id": self.user_id},
        )

        outgoing = outgoing_result.fetchall()

        for row in outgoing:
            target_id = row.target_document_id
            edges.append({"source": document_id, "target": target_id, "type": "outgoing"})
            await self._traverse_links(target_id, depth, nodes, edges, visited, current_depth + 1)

        incoming_result = await self.db.execute(
            text(
                """
                SELECT source_document_id
                FROM links
                WHERE target_document_id = :doc_id
                AND user_id = :user_id
            """
            ),
            {"doc_id": document_id, "user_id": self.user_id},
        )

        incoming = incoming_result.fetchall()

        for row in incoming:
            source_id = row.source_document_id
            if source_id not in visited:
                edges.append({"source": source_id, "target": document_id, "type": "incoming"})
                await self._traverse_links(
                    source_id, depth, nodes, edges, visited, current_depth + 1
                )
