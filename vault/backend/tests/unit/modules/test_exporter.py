"""Unit tests for export module.

Tests export service, handlers, and type validation.
"""

import csv
from datetime import datetime
import json
from pathlib import Path
from unittest.mock import AsyncMock
from uuid import uuid4
import zipfile

import pytest

from src.modules.exporter import (
    CSVExportHandler,
    ExportFormat,
    ExportMetadata,
    ExportRequest,
    ExportResult,
    ExportScope,
    ExportService,
    ExportStatus,
    FileExportData,
    JSONExportHandler,
    MarkdownExportHandler,
    ZIPExportHandler,
)


class TestExportTypes:
    """Test export type definitions and validation."""

    def test_export_format_enum(self):
        """Test ExportFormat enum values."""
        assert ExportFormat.JSON == "json"
        assert ExportFormat.CSV == "csv"
        assert ExportFormat.MARKDOWN == "markdown"
        assert ExportFormat.ZIP == "zip"

    def test_export_scope_enum(self):
        """Test ExportScope enum values."""
        assert ExportScope.FULL == "full"
        assert ExportScope.FILTERED == "filtered"
        assert ExportScope.SEARCH_RESULTS == "search_results"
        assert ExportScope.SELECTED == "selected"

    def test_export_request_validation(self):
        """Test ExportRequest model validation."""
        request = ExportRequest(format=ExportFormat.JSON, scope=ExportScope.FULL, compress=True)

        assert request.format == ExportFormat.JSON
        assert request.scope == ExportScope.FULL
        assert request.compress is True
        assert request.include_embeddings is False

    def test_export_result_creation(self):
        """Test ExportResult model creation."""
        export_id = uuid4()
        result = ExportResult(
            export_id=export_id,
            status=ExportStatus.COMPLETED,
            format=ExportFormat.JSON,
            scope=ExportScope.FULL,
            file_count=100,
            total_size_bytes=1024000,
            progress_percent=100.0,
        )

        assert result.export_id == export_id
        assert result.status == ExportStatus.COMPLETED
        assert result.file_count == 100
        assert result.total_size_bytes == 1024000


class TestJSONExportHandler:
    """Test JSON export handler."""

    @pytest.fixture()
    def sample_files(self):
        """Create sample file data for testing."""
        return [
            FileExportData(
                id=uuid4(),
                path="/test/file1.pdf",
                filename="file1.pdf",
                extension="pdf",
                mime_type="application/pdf",
                size_bytes=1024,
                hash_sha256="abc123",
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
                text_content="Sample text content",
                word_count=3,
                char_count=20,
                language="en",
                tags=["test"],
            )
        ]

    @pytest.fixture()
    def metadata(self):
        """Create sample export metadata."""
        return ExportMetadata(
            exported_at=datetime.utcnow(),
            vault_version="1.0.0",
            total_files=1,
            total_size_bytes=1024,
            export_format=ExportFormat.JSON,
        )

    @pytest.mark.asyncio()
    async def test_json_export_uncompressed(self, tmp_path, sample_files, metadata):
        """Test JSON export without compression."""
        output_path = tmp_path / "export.json"
        handler = JSONExportHandler(str(output_path), compress=False)

        result_path = await handler.generate(sample_files, metadata)

        assert Path(result_path).exists()
        assert result_path.endswith(".json")

        with open(result_path) as f:
            data = json.load(f)

        assert "metadata" in data
        assert "files" in data
        assert len(data["files"]) == 1
        assert data["files"][0]["filename"] == "file1.pdf"

    @pytest.mark.asyncio()
    async def test_json_export_compressed(self, tmp_path, sample_files, metadata):
        """Test JSON export with gzip compression."""
        import gzip

        output_path = tmp_path / "export.json"
        handler = JSONExportHandler(str(output_path), compress=True)

        result_path = await handler.generate(sample_files, metadata)

        assert Path(result_path).exists()
        assert result_path.endswith(".json.gz")

        with gzip.open(result_path, "rt", encoding="utf-8") as f:
            data = json.load(f)

        assert "metadata" in data
        assert "files" in data


class TestCSVExportHandler:
    """Test CSV export handler."""

    @pytest.fixture()
    def sample_files(self):
        """Create sample file data for testing."""
        return [
            FileExportData(
                id=uuid4(),
                path="/test/file1.pdf",
                filename="file1.pdf",
                extension="pdf",
                mime_type="application/pdf",
                size_bytes=1024,
                hash_sha256="abc123",
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
                text_content="Sample text",
                word_count=2,
                char_count=11,
                language="en",
                tags=["test", "sample"],
            ),
            FileExportData(
                id=uuid4(),
                path="/test/file2.txt",
                filename="file2.txt",
                extension="txt",
                mime_type="text/plain",
                size_bytes=512,
                hash_sha256="def456",
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
                text_content="Another text",
                word_count=2,
                char_count=12,
                language="en",
                tags=[],
            ),
        ]

    @pytest.fixture()
    def metadata(self):
        """Create sample export metadata."""
        return ExportMetadata(
            exported_at=datetime.utcnow(),
            vault_version="1.0.0",
            total_files=2,
            total_size_bytes=1536,
            export_format=ExportFormat.CSV,
        )

    @pytest.mark.asyncio()
    async def test_csv_export_uncompressed(self, tmp_path, sample_files, metadata):
        """Test CSV export without compression."""
        output_path = tmp_path / "export.csv"
        handler = CSVExportHandler(str(output_path), compress=False)

        result_path = await handler.generate(sample_files, metadata)

        assert Path(result_path).exists()
        assert result_path.endswith(".csv")

        with open(result_path, newline="") as f:
            reader = csv.DictReader(f)
            rows = list(reader)

        assert len(rows) == 2
        assert rows[0]["filename"] == "file1.pdf"
        assert rows[0]["tags"] == "test,sample"
        assert rows[1]["filename"] == "file2.txt"
        assert rows[1]["tags"] == ""


class TestMarkdownExportHandler:
    """Test Markdown export handler."""

    @pytest.fixture()
    def sample_files(self):
        """Create sample file data for testing."""
        return [
            FileExportData(
                id=uuid4(),
                path="/Users/test/Documents/report.pdf",
                filename="report.pdf",
                extension="pdf",
                mime_type="application/pdf",
                size_bytes=2048,
                hash_sha256="abc123",
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
                text_content="# Report\n\nThis is a test report.",
                word_count=5,
                char_count=30,
                language="en",
                tags=["important", "work"],
            )
        ]

    @pytest.fixture()
    def metadata(self):
        """Create sample export metadata."""
        return ExportMetadata(
            exported_at=datetime.utcnow(),
            vault_version="1.0.0",
            total_files=1,
            total_size_bytes=2048,
            export_format=ExportFormat.MARKDOWN,
        )

    @pytest.mark.asyncio()
    async def test_markdown_export_uncompressed(self, tmp_path, sample_files, metadata):
        """Test Markdown export without compression (directory structure)."""
        output_path = tmp_path / "export"
        handler = MarkdownExportHandler(str(output_path), compress=False)

        result_path = await handler.generate(sample_files, metadata)

        result_dir = Path(result_path)
        assert result_dir.exists()
        assert result_dir.is_dir()

        # Check for INDEX.md
        index_path = result_dir / "INDEX.md"
        assert index_path.exists()

        with open(index_path) as f:
            index_content = f.read()
            assert "Vault Export" in index_content
            assert "report.pdf" in index_content

        # Check for metadata.json
        metadata_path = result_dir / "metadata.json"
        assert metadata_path.exists()

        # Check for document markdown file
        docs_dir = result_dir / "documents"
        assert docs_dir.exists()
        assert len(list(docs_dir.glob("*.md"))) > 0

    @pytest.mark.asyncio()
    async def test_markdown_export_compressed(self, tmp_path, sample_files, metadata):
        """Test Markdown export with compression (ZIP)."""
        output_path = tmp_path / "export"
        handler = MarkdownExportHandler(str(output_path), compress=True)

        result_path = await handler.generate(sample_files, metadata)

        assert Path(result_path).exists()
        assert result_path.endswith(".zip")

        # Verify ZIP contents
        with zipfile.ZipFile(result_path, "r") as zipf:
            names = zipf.namelist()
            assert "INDEX.md" in names
            assert "metadata.json" in names


class TestZIPExportHandler:
    """Test ZIP export handler."""

    @pytest.fixture()
    def sample_files(self, tmp_path):
        """Create sample file data with actual files."""
        # Create a temporary file to include in export
        test_file = tmp_path / "test.txt"
        test_file.write_text("Test content")

        return [
            FileExportData(
                id=uuid4(),
                path=str(test_file),
                filename="test.txt",
                extension="txt",
                mime_type="text/plain",
                size_bytes=12,
                hash_sha256="abc123",
                modified_at=datetime.utcnow(),
                indexed_at=datetime.utcnow(),
                text_content="Test content",
                word_count=2,
                char_count=12,
                language="en",
                tags=[],
            )
        ]

    @pytest.fixture()
    def metadata(self):
        """Create sample export metadata."""
        return ExportMetadata(
            exported_at=datetime.utcnow(),
            vault_version="1.0.0",
            total_files=1,
            total_size_bytes=12,
            export_format=ExportFormat.ZIP,
        )

    @pytest.mark.asyncio()
    async def test_zip_export_with_original_files(self, tmp_path, sample_files, metadata):
        """Test ZIP export including original files."""
        output_path = tmp_path / "export.zip"
        handler = ZIPExportHandler(str(output_path), compress=False)

        result_path = await handler.generate(sample_files, metadata, include_original_files=True)

        assert Path(result_path).exists()
        assert result_path.endswith(".zip")

        # Verify ZIP contents
        with zipfile.ZipFile(result_path, "r") as zipf:
            names = zipf.namelist()
            assert "metadata.json" in names
            assert "index.json" in names
            assert any("extracted_text" in name for name in names)
            assert any("files/test.txt" in name for name in names)

    @pytest.mark.asyncio()
    async def test_zip_export_without_original_files(self, tmp_path, sample_files, metadata):
        """Test ZIP export without original files."""
        output_path = tmp_path / "export.zip"
        handler = ZIPExportHandler(str(output_path), compress=False)

        result_path = await handler.generate(sample_files, metadata, include_original_files=False)

        assert Path(result_path).exists()

        with zipfile.ZipFile(result_path, "r") as zipf:
            names = zipf.namelist()
            assert "metadata.json" in names
            assert "index.json" in names
            # Should still have extracted text
            assert any("extracted_text" in name for name in names)
            # Should NOT have original files
            assert not any("files/" in name for name in names)


class TestExportService:
    """Test export service."""

    @pytest.fixture()
    def mock_session(self):
        """Create mock database session."""
        session = AsyncMock()
        return session

    @pytest.mark.asyncio()
    async def test_create_export_full(self, mock_session, tmp_path):
        """Test creating a full export."""
        service = ExportService(mock_session, export_dir=str(tmp_path))

        request = ExportRequest(format=ExportFormat.JSON, scope=ExportScope.FULL, compress=True)

        result = await service.create_export(request)

        assert result.export_id is not None
        assert result.status == ExportStatus.PENDING
        assert result.format == ExportFormat.JSON
        assert result.scope == ExportScope.FULL

    @pytest.mark.asyncio()
    async def test_create_export_selected_validation(self, mock_session):
        """Test that selected scope requires file_ids."""
        service = ExportService(mock_session)

        request = ExportRequest(
            format=ExportFormat.JSON,
            scope=ExportScope.SELECTED,
            file_ids=None,  # Missing file_ids
        )

        with pytest.raises(ValueError, match="file_ids required"):
            await service.create_export(request)

    @pytest.mark.asyncio()
    async def test_get_export_status(self, mock_session):
        """Test getting export status."""
        service = ExportService(mock_session)

        # Create export first
        request = ExportRequest(format=ExportFormat.JSON, scope=ExportScope.FULL)
        result = await service.create_export(request)

        # Get status
        status = await service.get_export_status(result.export_id)

        assert status is not None
        assert status.export_id == result.export_id

    @pytest.mark.asyncio()
    async def test_cancel_export(self, mock_session):
        """Test cancelling an export."""
        service = ExportService(mock_session)

        # Create export
        request = ExportRequest(format=ExportFormat.JSON, scope=ExportScope.FULL)
        result = await service.create_export(request)

        # Cancel it
        cancelled = await service.cancel_export(result.export_id)

        assert cancelled is True

        # Check status
        status = await service.get_export_status(result.export_id)
        assert status.status == ExportStatus.CANCELLED


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
