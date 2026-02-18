"""Indexing service - orchestrates document indexing pipeline.

This service coordinates:
- Content extraction (via content_extractor module)
- Text chunking (via chunking module)
- Embedding generation (via embedding_generator module)
- Database persistence (File, TextContent, Image, embeddings)
- Event emission for indexed/failed/deleted documents

The service is a THIN ORCHESTRATOR - it does not contain indexing algorithms,
just coordinates calls to domain modules and manages persistence.
"""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime
import logging
from pathlib import Path
from typing import TYPE_CHECKING
from uuid import UUID

from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from src.config.settings import get_settings
from src.models import File, Image, ImageEmbedding, TextContent, TextEmbedding
from src.modules.chunking import ChunkingService
from src.modules.content_extractor import extract_content
from src.modules.embedding_generator import ImageEmbedder, TextEmbedder

if TYPE_CHECKING:
    from src.events import EventBus

logger = logging.getLogger(__name__)


class IndexingService:
    def __init__(self, event_bus: EventBus | None = None):
        self.settings = get_settings()
        self.text_embedder = TextEmbedder()
        self.image_embedder = ImageEmbedder()
        self.chunking_service = ChunkingService(
            chunk_size=self.settings.search_chunk_size,
            chunk_overlap=self.settings.search_chunk_overlap,
            enable_context=self.settings.enable_contextual_retrieval,
            context_template=self.settings.context_prefix_template,
        )
        self.event_bus = event_bus

    async def index_file(self, file_id: UUID, db_session: AsyncSession) -> bool:
        try:
            result = await db_session.execute(select(File).where(File.id == file_id))
            file = result.scalar_one_or_none()

            if not file:
                logger.error(f"File not found: {file_id}")
                return False

            logger.info(f"Indexing file: {file.path}")

            await self._update_file_status(file_id, "processing", None, db_session)

            file_path = Path(file.path)
            if not file_path.exists():
                raise FileNotFoundError(f"File does not exist: {file_path}")

            extracted = await self._extract_content(file_path, file.mime_type)

            if extracted.text and extracted.text.strip():
                await self._process_text_content(
                    file_id=file_id,
                    text=extracted.text,
                    metadata=extracted.metadata,
                    db_session=db_session,
                )

            if file.mime_type.startswith("image/"):
                await self._process_image_content(
                    file_id=file_id,
                    file_path=file_path,
                    metadata=extracted.metadata,
                    db_session=db_session,
                )

            await self._update_file_status(file_id, "indexed", None, db_session)

            logger.info(f"Successfully indexed file: {file.path}")

            # Emit document indexed event
            if self.event_bus:
                from src.events.domain import DocumentIndexedEvent

                await self.event_bus.publish(
                    DocumentIndexedEvent(
                        document_id=int(file_id),
                        file_path=file.path,
                        chunk_count=1,  # Simplified: actual count would require tracking
                        embedding_model=self.text_embedder.TEXT_MODEL_NAME,
                    )
                )

            return True

        except Exception as e:
            logger.error(f"Failed to index file {file_id}: {e}", exc_info=True)
            await self._update_file_status(file_id, "failed", str(e), db_session)

            # Emit document indexing failed event
            if self.event_bus:
                from src.events.domain import DocumentIndexingFailedEvent

                await self.event_bus.publish(
                    DocumentIndexingFailedEvent(
                        file_path=file.path if file else str(file_id),
                        error_type=type(e).__name__,
                        error_message=str(e),
                    )
                )

            raise

    async def _extract_content(self, file_path: Path, mime_type: str):
        try:
            return extract_content(file_path, mime_type=mime_type)
        except Exception as e:
            logger.error(f"Content extraction failed: {e}")
            raise

    async def _process_text_content(
        self,
        file_id: UUID,
        text: str,
        metadata: dict,
        db_session: AsyncSession,
        file_path: Path | None = None,
    ) -> UUID:
        result = await db_session.execute(select(File).where(File.id == file_id))
        file_obj = result.scalar_one_or_none()
        file_path = file_path or (Path(file_obj.path) if file_obj else None)

        text_to_embed = text
        contextualized_metadata = metadata.copy()

        if self.settings.enable_contextual_retrieval and file_path:
            try:
                chunks = self.chunking_service.chunk_text_with_context(
                    text=text, file_path=str(file_path)
                )

                if chunks:
                    text_to_embed = chunks[0].contextualized_text
                    contextualized_metadata["has_contextual_retrieval"] = True
                    contextualized_metadata["document_title"] = chunks[0].metadata.get(
                        "document_title"
                    )
                    contextualized_metadata["topic_hint"] = chunks[0].metadata.get("topic_hint")
                    contextualized_metadata["original_text"] = chunks[0].original_text

                    logger.info(f"Applied contextual retrieval to {file_path.name}")
            except Exception as e:
                logger.warning(f"Failed to apply contextual retrieval: {e}. Using original text.")
                contextualized_metadata["has_contextual_retrieval"] = False

        text_content = TextContent(
            file_id=file_id,
            content=text,
            char_count=len(text),
            language=metadata.get("language", "en"),
        )
        db_session.add(text_content)
        await db_session.flush()

        embedding = await asyncio.get_event_loop().run_in_executor(
            None, self.text_embedder.embed, text_to_embed
        )

        text_embedding = TextEmbedding(
            file_id=file_id,
            embedding=embedding.tolist(),
            model_name=self.text_embedder.TEXT_MODEL_NAME,
        )
        db_session.add(text_embedding)

        logger.info(
            f"Processed text content: {len(text)} chars (contextualized: {contextualized_metadata.get('has_contextual_retrieval', False)})"
        )
        return text_content.id

    async def _process_image_content(
        self, file_id: UUID, file_path: Path, metadata: dict, db_session: AsyncSession
    ) -> UUID:
        image = Image(
            file_id=file_id,
            width=metadata.get("width"),
            height=metadata.get("height"),
            format=metadata.get("format"),
            color_space=metadata.get("color_space"),
        )
        db_session.add(image)
        await db_session.flush()

        embedding = await asyncio.get_event_loop().run_in_executor(
            None, self.image_embedder.embed, str(file_path)
        )

        image_embedding = ImageEmbedding(
            file_id=file_id,
            embedding=embedding.tolist(),
            model_name=self.image_embedder.IMAGE_MODEL_NAME,
        )
        db_session.add(image_embedding)

        logger.info(f"Processed image: {file_path.name}")
        return image.id

    async def _update_file_status(
        self, file_id: UUID, status: str, error: str | None, db_session: AsyncSession
    ) -> None:
        values = {"processing_status": status}
        if status == "indexed":
            values["indexed_at"] = datetime.now(UTC)
        if error:
            values["processing_error"] = error

        await db_session.execute(update(File).where(File.id == file_id).values(**values))

    async def reindex_file(self, file_id: UUID, db_session: AsyncSession) -> bool:
        try:
            await db_session.execute(
                update(TextEmbedding)
                .where(TextEmbedding.file_id == file_id)
                .values(is_deleted=True)
            )
            await db_session.execute(
                update(ImageEmbedding)
                .where(ImageEmbedding.file_id == file_id)
                .values(is_deleted=True)
            )

            return await self.index_file(file_id, db_session)

        except Exception as e:
            logger.error(f"Failed to reindex file {file_id}: {e}")
            return False

    async def delete_file_index(self, file_id: UUID, db_session: AsyncSession) -> bool:
        try:
            # Get file info before deletion for event
            result = await db_session.execute(select(File).where(File.id == file_id))
            file = result.scalar_one_or_none()

            await db_session.execute(
                update(TextEmbedding)
                .where(TextEmbedding.file_id == file_id)
                .values(is_deleted=True)
            )
            await db_session.execute(
                update(ImageEmbedding)
                .where(ImageEmbedding.file_id == file_id)
                .values(is_deleted=True)
            )

            await db_session.execute(
                update(TextContent).where(TextContent.file_id == file_id).values(is_deleted=True)
            )
            await db_session.execute(
                update(Image).where(Image.file_id == file_id).values(is_deleted=True)
            )

            logger.info(f"Deleted index for file: {file_id}")

            # Emit document deleted event
            if self.event_bus and file:
                from src.events.domain import DocumentDeletedEvent

                await self.event_bus.publish(
                    DocumentDeletedEvent(
                        document_id=int(file_id),
                        file_path=file.path,
                        chunk_count=1,  # Simplified: actual count would require tracking
                    )
                )

            return True

        except Exception as e:
            logger.error(f"Failed to delete index for file {file_id}: {e}")
            return False
