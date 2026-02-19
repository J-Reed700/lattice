from __future__ import annotations

from collections.abc import AsyncGenerator, Generator
from datetime import datetime
import os
from pathlib import Path
import tempfile
from typing import Any
from uuid import UUID, uuid4

from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine
from sqlalchemy.pool import NullPool

TEST_DATABASE_URL = os.getenv(
    "TEST_DATABASE_URL", "postgresql+asyncpg://vault:vault@localhost:5432/vault_test"
)


@pytest.fixture(scope="session")
def event_loop():
    import asyncio

    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()


@pytest.fixture(scope="session")
async def test_engine():
    from src.models.base import Base

    engine = create_async_engine(
        TEST_DATABASE_URL,
        poolclass=NullPool,
        echo=False,
    )

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)
        await conn.run_sync(Base.metadata.create_all)

    yield engine

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)

    await engine.dispose()


@pytest.fixture()
async def db_session(test_engine) -> AsyncGenerator[AsyncSession, None]:
    async_session = async_sessionmaker(
        test_engine,
        class_=AsyncSession,
        expire_on_commit=False,
    )

    async with async_session() as session:
        async with session.begin():
            yield session
            await session.rollback()


@pytest.fixture()
async def client(db_session: AsyncSession) -> AsyncGenerator[AsyncClient, None]:
    from src.api.app import app
    from src.db import get_session

    async def override_get_session():
        yield db_session

    app.dependency_overrides[get_session] = override_get_session

    async with AsyncClient(app=app, base_url="http://test") as test_client:
        yield test_client

    app.dependency_overrides.clear()


@pytest.fixture()
def temp_dir() -> Generator[Path, None, None]:
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


@pytest.fixture()
async def watch_folder(db_session: AsyncSession, temp_dir: Path) -> Any:
    from src.models import WatchFolder

    folder = WatchFolder(
        id=uuid4(),
        path=str(temp_dir),
        name="Test Folder",
        is_active=True,
        recursive=True,
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow(),
    )
    db_session.add(folder)
    await db_session.commit()
    await db_session.refresh(folder)
    return folder


@pytest.fixture()
def sample_text_file(temp_dir: Path) -> Path:
    file_path = temp_dir / "test_document.txt"
    file_path.write_text(
        "This is a test document for indexing.\n"
        "It contains multiple lines of text.\n"
        "The quick brown fox jumps over the lazy dog.\n"
        "Testing semantic search functionality."
    )
    return file_path


@pytest.fixture()
def sample_markdown_file(temp_dir: Path) -> Path:
    file_path = temp_dir / "test_readme.md"
    file_path.write_text(
        "# Test Document\n\n"
        "## Introduction\n\n"
        "This is a markdown document for testing.\n\n"
        "## Features\n\n"
        "- Bullet point 1\n"
        "- Bullet point 2\n"
        "- Bullet point 3\n\n"
        "## Code Example\n\n"
        "```python\n"
        "def hello_world():\n"
        "    print('Hello, World!')\n"
        "```\n"
    )
    return file_path


@pytest.fixture()
def sample_json_file(temp_dir: Path) -> Path:
    import json

    file_path = temp_dir / "test_data.json"
    data = {
        "name": "Test User",
        "email": "test@example.com",
        "age": 30,
        "tags": ["testing", "automation", "search"],
    }
    file_path.write_text(json.dumps(data, indent=2))
    return file_path


@pytest.fixture()
def sample_pdf_file(temp_dir: Path) -> Path:
    try:
        from reportlab.lib.pagesizes import letter
        from reportlab.pdfgen import canvas
    except ImportError:
        pytest.skip("reportlab not installed")

    file_path = temp_dir / "test_document.pdf"
    c = canvas.Canvas(str(file_path), pagesize=letter)
    c.drawString(100, 750, "Test PDF Document")
    c.drawString(100, 730, "This is a sample PDF for testing.")
    c.drawString(100, 710, "It contains extractable text content.")
    c.drawString(100, 690, "Machine learning and artificial intelligence.")
    c.showPage()
    c.save()
    return file_path


@pytest.fixture()
def large_text_file(temp_dir: Path) -> Path:
    file_path = temp_dir / "large_document.txt"
    content = "\n".join([f"Line {i}: This is test content." for i in range(1000)])
    file_path.write_text(content)
    return file_path


@pytest.fixture()
def invalid_pdf_file(temp_dir: Path) -> Path:
    file_path = temp_dir / "corrupted.pdf"
    file_path.write_bytes(b"This is not a valid PDF file")
    return file_path


@pytest.fixture()
async def indexed_file(db_session: AsyncSession, watch_folder: Any, sample_text_file: Path) -> Any:
    from src.models import File

    file_record = File(
        id=uuid4(),
        watch_folder_id=watch_folder.id,
        path=str(sample_text_file),
        filename=sample_text_file.name,
        extension="txt",
        size_bytes=sample_text_file.stat().st_size,
        mime_type="text/plain",
        hash_sha256="abc123def456" + "0" * 48,
        modified_at=datetime.fromtimestamp(sample_text_file.stat().st_mtime),
        indexed_at=datetime.utcnow(),
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow(),
    )
    db_session.add(file_record)
    await db_session.commit()
    await db_session.refresh(file_record)
    return file_record


@pytest.fixture()
async def indexed_file_with_content(db_session: AsyncSession, indexed_file: Any) -> Any:
    from src.models import TextContent

    text_content = TextContent(
        id=uuid4(),
        file_id=indexed_file.id,
        content="This is a test document for indexing. It contains searchable text.",
        language="en",
        char_count=100,
        word_count=15,
        embedding=[0.1] * 384,
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow(),
    )
    db_session.add(text_content)
    await db_session.commit()
    await db_session.refresh(indexed_file)
    return indexed_file


@pytest.fixture()
def mock_embedding_service(mocker):
    from src.services.embeddings.service import EmbeddingService

    mock_service = mocker.Mock(spec=EmbeddingService)
    mock_service.text_generator.generate_from_text.return_value = [0.1] * 384
    mock_service.text_generator.generate_from_chunks.return_value = [[0.1] * 384]
    return mock_service


@pytest.fixture()
def file_factory():
    def create_file(
        watch_folder_id: UUID, path: str, filename: str, extension: str, **kwargs
    ) -> Any:
        from src.models import File

        defaults = {
            "id": uuid4(),
            "size_bytes": 1024,
            "mime_type": "text/plain",
            "hash_sha256": "test_hash_" + "0" * 54,
            "modified_at": datetime.utcnow(),
            "indexed_at": datetime.utcnow(),
            "created_at": datetime.utcnow(),
            "updated_at": datetime.utcnow(),
        }
        defaults.update(kwargs)

        return File(
            watch_folder_id=watch_folder_id,
            path=path,
            filename=filename,
            extension=extension,
            **defaults,
        )

    return create_file


@pytest.fixture()
async def multiple_indexed_files(
    db_session: AsyncSession, watch_folder: Any, temp_dir: Path
) -> list[Any]:
    from src.models import File

    files = []

    for i in range(5):
        file_path = temp_dir / f"test_{i}.txt"
        file_path.write_text(f"Test content {i}")

        file_record = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(file_path),
            filename=file_path.name,
            extension="txt",
            size_bytes=file_path.stat().st_size,
            mime_type="text/plain",
            hash_sha256=f"hash_{i}" + "0" * 58,
            modified_at=datetime.fromtimestamp(file_path.stat().st_mtime),
            indexed_at=datetime.utcnow(),
            created_at=datetime.utcnow(),
            updated_at=datetime.utcnow(),
        )
        db_session.add(file_record)
        files.append(file_record)

    await db_session.commit()

    for file in files:
        await db_session.refresh(file)

    return files


@pytest.fixture()
def mock_content_extractor(mocker):
    from src.modules.content_extractor.types import ExtractedContent

    mock_extractor = mocker.patch("src.modules.content_extractor.extract_content")
    mock_extractor.return_value = ExtractedContent(
        text="Extracted test content",
        mime_type="text/plain",
        file_path="/test/path.txt",
        metadata={},
        encoding="utf-8",
        language="en",
        extraction_time=datetime.utcnow(),
        file_size=1024,
    )
    return mock_extractor


@pytest.fixture()
def sample_image_file(temp_dir: Path) -> Path:
    from PIL import Image

    file_path = temp_dir / "test_image.jpg"
    img = Image.new("RGB", (800, 600), color=(255, 255, 255))
    img.save(file_path, format="JPEG")
    return file_path


@pytest.fixture()
def sample_screenshot_file(temp_dir: Path) -> Path:
    from PIL import Image

    file_path = temp_dir / "screenshot.png"
    img = Image.new("RGB", (1920, 1080), color=(240, 240, 240))
    img.save(file_path, format="PNG")
    return file_path


@pytest.fixture()
def sample_photo_file(temp_dir: Path) -> Path:
    import random

    from PIL import Image

    file_path = temp_dir / "photo.jpg"
    img = Image.new("RGB", (800, 600))
    pixels = img.load()

    for x in range(img.width):
        for y in range(img.height):
            pixels[x, y] = (random.randint(0, 255), random.randint(0, 255), random.randint(0, 255))

    img.save(file_path, format="JPEG")
    return file_path


@pytest.fixture()
def test_settings():
    """Provide test settings for dependency injection."""
    from src.config import Settings

    return Settings(
        database_url=TEST_DATABASE_URL,
        ollama_base_url="http://localhost:11434",
        ollama_default_model="llama2",
        ollama_timeout=120,
        ollama_stream_timeout=300,
    )
