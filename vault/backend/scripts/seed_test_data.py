"""Seed test database with sample data for UUID migration testing."""

import asyncio
import os
import sys
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from sqlalchemy import String, Text, select
from sqlalchemy.ext.asyncio import AsyncSession, create_async_engine
from sqlalchemy.orm import sessionmaker

from config.settings import get_settings
from models import (
    File,
    FileTag,
    Image,
    ImageEmbedding,
    SearchHistory,
    Tag,
    TextContent,
    TextEmbedding,
    WatchFolder,
)


async def seed_data() -> None:
    """Seed test database with sample data."""

    settings = get_settings()
    database_url = os.environ.get("DATABASE_URL", settings.database_url)

    engine = create_async_engine(database_url, echo=False)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    async with async_session() as session:
        print("Creating watch folders...")
        # Create watch folders
        watch_folders = []
        for i in range(5):
            watch_folder = WatchFolder(
                id=str(uuid4()),
                path=f"/test/documents_{i}",
                name=f"Test Documents {i}",
                is_active=True,
                created_at=datetime.now(UTC),
                updated_at=datetime.now(UTC),
            )
            watch_folders.append(watch_folder)
            session.add(watch_folder)

        await session.flush()

        print("Creating files...")
        # Create files
        files = []
        for i in range(100):
            watch_folder = watch_folders[i % len(watch_folders)]
            file = File(
                id=str(uuid4()),
                path=f"/test/documents_{i % 5}/doc_{i}.txt",
                filename=f"doc_{i}.txt",
                extension="txt",
                mime_type="text/plain",
                size_bytes=1024 * (i + 1),
                hash_sha256=f"sha256_{i:064d}",
                watch_folder_id=watch_folder.id,
                created_at=datetime.now(UTC),
                modified_at=datetime.now(UTC),
                indexed_at=datetime.now(UTC),
            )
            files.append(file)
            session.add(file)

        await session.flush()

        print("Creating text content and embeddings...")
        # Create text content and embeddings for first 50 files
        text_contents = []
        for idx, file in enumerate(files[:50]):
            text_content = TextContent(
                id=str(uuid4()),
                file_id=file.id,
                content=f"Sample content for {file.filename}. " * 100,
                language="en",
                word_count=200,
                char_count=2000,
                created_at=datetime.now(UTC),
            )
            text_contents.append(text_content)
            session.add(text_content)

        await session.flush()

        # Create embeddings (10 per text_content)
        embeddings_count = 0
        for text_content in text_contents:
            for chunk_idx in range(10):
                embedding = TextEmbedding(
                    id=str(uuid4()),
                    text_content_id=text_content.id,
                    chunk_index=chunk_idx,
                    chunk_text=f"Chunk {chunk_idx} of document",
                    embedding=[0.1] * 768,  # Dummy 768-dim embedding
                    model_name="test-model-v1",
                    dimension=768,
                    created_at=datetime.now(UTC),
                )
                session.add(embedding)
                embeddings_count += 1

        await session.flush()

        print("Creating images and image embeddings...")
        # Create images for 20 files
        images = []
        for file in files[:20]:
            image = Image(
                id=str(uuid4()),
                file_id=file.id,
                width=1920,
                height=1080,
                format="png",
                color_space="RGB",
                has_alpha=False,
                created_at=datetime.now(UTC),
            )
            images.append(image)
            session.add(image)

        await session.flush()

        # Create image embeddings (5 per image)
        image_embeddings_count = 0
        for image in images:
            for chunk_idx in range(5):
                img_embedding = ImageEmbedding(
                    id=str(uuid4()),
                    image_id=image.id,
                    embedding=[0.2] * 512,  # Dummy 512-dim image embedding
                    model_name="clip-vit-base-patch32",
                    dimension=512,
                    region_x=chunk_idx * 100,
                    region_y=0,
                    region_width=100,
                    region_height=100,
                    created_at=datetime.now(UTC),
                )
                session.add(img_embedding)
                image_embeddings_count += 1

        await session.flush()

        print("Creating tags...")
        # Create tags
        tags = []
        for i in range(20):
            tag = Tag(
                id=str(uuid4()),
                name=f"tag_{i}",
                color=f"#{i:06x}",
                description=f"Test tag {i}",
                created_at=datetime.now(UTC),
                updated_at=datetime.now(UTC),
            )
            tags.append(tag)
            session.add(tag)

        await session.flush()

        print("Creating file-tag associations...")
        # Tag files (each file gets 3 random tags)
        file_tags_count = 0
        for file_idx, file in enumerate(files[:30]):
            for tag_idx in range(3):
                tag = tags[(file_idx + tag_idx) % len(tags)]
                file_tag = FileTag(
                    id=str(uuid4()),
                    file_id=file.id,
                    tag_id=tag.id,
                    created_at=datetime.now(UTC),
                )
                session.add(file_tag)
                file_tags_count += 1

        await session.flush()

        print("Creating search history...")
        # Create search history
        search_history_count = 0
        for i in range(50):
            search = SearchHistory(
                id=str(uuid4()),
                query=f"test query {i}",
                results_count=i % 20,
                search_type="semantic",
                created_at=datetime.now(UTC),
            )
            session.add(search)
            search_history_count += 1

        await session.flush()

        # Commit all changes
        await session.commit()

        print()
        print("✅ Seeded test data:")
        print(f"   - {len(watch_folders)} watch folders")
        print(f"   - {len(files)} files")
        print(f"   - {len(text_contents)} text contents")
        print(f"   - {embeddings_count} text embeddings")
        print(f"   - {len(images)} images")
        print(f"   - {image_embeddings_count} image embeddings")
        print(f"   - {len(tags)} tags")
        print(f"   - {file_tags_count} file-tag associations")
        print(f"   - {search_history_count} search history entries")
        print()
        print(f"Total records: {len(watch_folders) + len(files) + len(text_contents) + embeddings_count + len(images) + image_embeddings_count + len(tags) + file_tags_count + search_history_count}")

    await engine.dispose()


if __name__ == "__main__":
    asyncio.run(seed_data())
