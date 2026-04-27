from datetime import datetime
from pathlib import Path
from unittest.mock import AsyncMock
from uuid import uuid4

import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.models import File, TextContent, WatchFolder
from src.modules.reranker import Reranker


@pytest.fixture()
async def mock_storage_service(mocker):
    mock_storage = mocker.Mock()
    mock_storage.store_file = mocker.AsyncMock(
        return_value=("http://storage.url/file", "http://storage.url/thumb")
    )
    mock_storage.delete_file = mocker.AsyncMock(return_value=True)
    mock_storage.get_file_url = mocker.AsyncMock(return_value="http://storage.url/file")
    mock_storage.cleanup_orphaned_files = mocker.AsyncMock(return_value={"deleted": 5, "failed": 0})
    return mock_storage


@pytest.fixture()
async def mock_vector_store(mocker):
    mock_store = mocker.Mock()
    mock_store.add_embedding = mocker.AsyncMock(return_value=str(uuid4()))
    mock_store.search = mocker.AsyncMock(
        return_value=[{"id": str(uuid4()), "score": 0.95}, {"id": str(uuid4()), "score": 0.85}]
    )
    mock_store.delete_embedding = mocker.AsyncMock(return_value=True)
    return mock_store


@pytest.fixture()
async def mock_reranker(mocker):
    mock = mocker.Mock(spec=Reranker)
    mock.rerank = mocker.AsyncMock(
        side_effect=lambda results, query: [
            {**r, "score": r.get("score", 0.5) * 1.2} for r in results
        ]
    )
    return mock


@pytest.fixture()
def sample_pdf_with_text(temp_dir: Path) -> Path:
    try:
        from reportlab.lib.pagesizes import letter
        from reportlab.pdfgen import canvas
    except ImportError:
        pytest.skip("reportlab not installed")

    file_path = temp_dir / "searchable.pdf"
    c = canvas.Canvas(str(file_path), pagesize=letter)
    c.drawString(100, 750, "Machine Learning Research Paper")
    c.drawString(100, 730, "Abstract: This paper discusses neural networks.")
    c.drawString(100, 710, "Deep learning has revolutionized AI.")
    c.showPage()
    c.save()
    return file_path


@pytest.fixture()
async def indexed_files_batch(
    db_session: AsyncSession, watch_folder: WatchFolder, temp_dir: Path
) -> list[File]:
    files = []

    test_data = [
        ("python_tutorial.txt", "Python is a high-level programming language.", "text/plain"),
        ("javascript_guide.md", "# JavaScript Guide\n\nJavaScript is versatile.", "text/markdown"),
        (
            "data_science.json",
            '{"topic": "data science", "tools": ["pandas", "numpy"]}',
            "application/json",
        ),
    ]

    for filename, content, mime_type in test_data:
        file_path = temp_dir / filename
        file_path.write_text(content)

        file_record = File(
            id=uuid4(),
            watch_folder_id=watch_folder.id,
            path=str(file_path),
            filename=filename,
            extension=filename.split(".")[-1],
            size_bytes=file_path.stat().st_size,
            mime_type=mime_type,
            hash_sha256=f"hash_{filename}" + "0" * (64 - len(filename) - 5),
            modified_at=datetime.utcnow(),
            indexed_at=datetime.utcnow(),
            created_at=datetime.utcnow(),
            updated_at=datetime.utcnow(),
        )
        db_session.add(file_record)
        files.append(file_record)

    await db_session.commit()

    for file in files:
        await db_session.refresh(file)

    for file in files:
        text_content = TextContent(
            id=uuid4(),
            file_id=file.id,
            content=f"Content for {file.filename}",
            language="en",
            char_count=100,
            word_count=15,
            embedding=[0.1] * 384,
            created_at=datetime.utcnow(),
            updated_at=datetime.utcnow(),
        )
        db_session.add(text_content)

    await db_session.commit()

    return files


@pytest.fixture()
def concurrent_test_files(temp_dir: Path) -> list[Path]:
    files = []
    for i in range(10):
        file_path = temp_dir / f"concurrent_{i}.txt"
        file_path.write_text(f"Concurrent file content {i}")
        files.append(file_path)
    return files


@pytest.fixture()
async def orphaned_storage_files(mock_storage_service):
    mock_storage_service.list_files = AsyncMock(
        return_value=["orphan_1.txt", "orphan_2.pdf", "orphan_3.jpg", "valid_1.txt", "valid_2.pdf"]
    )
    return mock_storage_service


@pytest.fixture()
def sample_csv_file(temp_dir: Path) -> Path:
    file_path = temp_dir / "data.csv"
    content = """name,email,age
John Doe,john@example.com,30
Jane Smith,jane@example.com,25
Bob Johnson,bob@example.com,35
"""
    file_path.write_text(content)
    return file_path


@pytest.fixture()
def sample_xml_file(temp_dir: Path) -> Path:
    file_path = temp_dir / "data.xml"
    content = """<?xml version="1.0"?>
<catalog>
    <book id="1">
        <title>Machine Learning Basics</title>
        <author>John Doe</author>
    </book>
    <book id="2">
        <title>Deep Learning Advanced</title>
        <author>Jane Smith</author>
    </book>
</catalog>
"""
    file_path.write_text(content)
    return file_path


@pytest.fixture()
async def indexed_pdf_file(
    db_session: AsyncSession, watch_folder: WatchFolder, sample_pdf_with_text: Path
) -> File:
    file_record = File(
        id=uuid4(),
        watch_folder_id=watch_folder.id,
        path=str(sample_pdf_with_text),
        filename=sample_pdf_with_text.name,
        extension="pdf",
        size_bytes=sample_pdf_with_text.stat().st_size,
        mime_type="application/pdf",
        hash_sha256="pdf_hash" + "0" * 56,
        modified_at=datetime.utcnow(),
        indexed_at=datetime.utcnow(),
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow(),
    )
    db_session.add(file_record)
    await db_session.commit()
    await db_session.refresh(file_record)

    text_content = TextContent(
        id=uuid4(),
        file_id=file_record.id,
        content="Machine Learning Research Paper. Abstract: This paper discusses neural networks.",
        language="en",
        char_count=150,
        word_count=20,
        embedding=[0.2] * 384,
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow(),
    )
    db_session.add(text_content)
    await db_session.commit()

    return file_record


@pytest.fixture()
def mock_bm25_engine(mocker):
    mock_engine = mocker.Mock()
    mock_engine.search = mocker.AsyncMock(
        return_value=[
            {
                "document_id": str(uuid4()),
                "file_path": "/test/file1.txt",
                "filename": "file1.txt",
                "bm25_score": 2.5,
            },
            {
                "document_id": str(uuid4()),
                "file_path": "/test/file2.txt",
                "filename": "file2.txt",
                "bm25_score": 1.8,
            },
        ]
    )
    return mock_engine


@pytest.fixture()
def mock_hybrid_engine(mocker):
    mock_engine = mocker.Mock()
    mock_engine.search = mocker.AsyncMock(
        return_value=[
            {
                "file_id": str(uuid4()),
                "file_path": "/test/file1.txt",
                "filename": "file1.txt",
                "score": 0.92,
            },
            {
                "file_id": str(uuid4()),
                "file_path": "/test/file2.txt",
                "filename": "file2.txt",
                "score": 0.87,
            },
        ]
    )
    return mock_engine
