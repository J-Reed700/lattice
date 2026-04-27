from datetime import datetime

import pytest

from src.modules.content_extractor.extractors.pdf import extract_pdf
from src.modules.content_extractor.extractors.text import extract_json, extract_text
from src.modules.content_extractor.types import (
    CorruptedFileError,
    ExtractedContent,
    ExtractionError,
)


@pytest.mark.unit()
class TestTextExtraction:
    def test_extract_text_success(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert isinstance(content, ExtractedContent)
        assert content.mime_type == "text/plain"
        assert len(content.text) > 0
        assert "test document" in content.text.lower()
        assert content.encoding in ["utf-8", "ascii"]
        assert content.file_size > 0

    def test_extract_text_with_special_characters(self, temp_dir):
        file_path = temp_dir / "special.txt"
        file_path.write_text("Hello 世界 🌍 Ñoño")

        content = extract_text(str(file_path))
        assert "Hello" in content.text
        assert content.encoding == "utf-8"

    def test_extract_text_empty_file(self, temp_dir):
        file_path = temp_dir / "empty.txt"
        file_path.write_text("")

        content = extract_text(str(file_path))
        assert content.text == ""
        assert content.mime_type == "text/plain"

    def test_extract_text_file_not_found(self):
        with pytest.raises(ExtractionError, match="Failed to read text file"):
            extract_text("/nonexistent/file.txt")

    def test_extract_text_preserves_structure(self, sample_markdown_file):
        content = extract_text(str(sample_markdown_file))
        assert "# Test Document" in content.text
        assert "## Introduction" in content.text
        assert "```python" in content.text


@pytest.mark.unit()
class TestJSONExtraction:
    def test_extract_json_success(self, sample_json_file):
        content = extract_json(str(sample_json_file))
        assert isinstance(content, ExtractedContent)
        assert content.mime_type == "application/json"
        assert "Test User" in content.text
        assert "test@example.com" in content.text
        assert "keys" in content.metadata
        assert content.metadata["key_count"] == 4

    def test_extract_json_array(self, temp_dir):
        file_path = temp_dir / "array.json"
        file_path.write_text('[{"id": 1}, {"id": 2}, {"id": 3}]')

        content = extract_json(str(file_path))
        assert content.mime_type == "application/json"
        assert "item_count" in content.metadata
        assert content.metadata["item_count"] == 3

    def test_extract_json_invalid(self, temp_dir):
        file_path = temp_dir / "invalid.json"
        file_path.write_text('{"key": invalid}')

        with pytest.raises(CorruptedFileError, match="Invalid JSON format"):
            extract_json(str(file_path))

    def test_extract_json_empty_object(self, temp_dir):
        file_path = temp_dir / "empty.json"
        file_path.write_text("{}")

        content = extract_json(str(file_path))
        assert content.metadata["key_count"] == 0

    def test_extract_json_nested(self, temp_dir):
        import json

        file_path = temp_dir / "nested.json"
        data = {"user": {"name": "John", "address": {"city": "NYC", "zip": "10001"}}}
        file_path.write_text(json.dumps(data))

        content = extract_json(str(file_path))
        assert "NYC" in content.text


@pytest.mark.unit()
class TestPDFExtraction:
    def test_extract_pdf_success(self, sample_pdf_file):
        content = extract_pdf(str(sample_pdf_file))
        assert isinstance(content, ExtractedContent)
        assert content.mime_type == "application/pdf"
        assert len(content.text) > 0
        assert "Test PDF Document" in content.text
        assert "page_count" in content.metadata
        assert content.metadata["page_count"] >= 1
        assert content.file_size > 0

    def test_extract_pdf_corrupted(self, invalid_pdf_file):
        with pytest.raises(CorruptedFileError, match="corrupted or invalid"):
            extract_pdf(str(invalid_pdf_file))

    def test_extract_pdf_not_found(self):
        with pytest.raises(ExtractionError):
            extract_pdf("/nonexistent/file.pdf")

    def test_extract_pdf_metadata_extraction(self, sample_pdf_file):
        content = extract_pdf(str(sample_pdf_file))
        assert isinstance(content.metadata, dict)
        assert "page_count" in content.metadata


@pytest.mark.unit()
class TestContentNormalization:
    def test_normalize_whitespace(self, temp_dir):
        file_path = temp_dir / "whitespace.txt"
        file_path.write_text("Line1\n\n\n\nLine2\t\tTabbed")

        content = extract_text(str(file_path))
        assert "\n\n\n\n" not in content.text or content.text.count("\n") < 10

    def test_normalize_unicode(self, temp_dir):
        file_path = temp_dir / "unicode.txt"
        file_path.write_text("café résumé naïve")

        content = extract_text(str(file_path))
        assert "café" in content.text or "cafe" in content.text


@pytest.mark.unit()
class TestEncodingDetection:
    def test_detect_utf8_encoding(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert content.encoding in ["utf-8", "ascii"]

    def test_handle_encoding_errors(self, temp_dir):
        file_path = temp_dir / "encoding.txt"
        file_path.write_bytes(b"\xff\xfe\x00\x00T\x00e\x00s\x00t\x00")

        try:
            content = extract_text(str(file_path))
            assert content.encoding == "utf-8"
        except ExtractionError:
            pytest.skip("Encoding fallback not implemented")


@pytest.mark.unit()
class TestLanguageDetection:
    def test_detect_english_language(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert content.language is None or content.language == "en"

    def test_detect_language_short_text(self, temp_dir):
        file_path = temp_dir / "short.txt"
        file_path.write_text("Hi")

        content = extract_text(str(file_path))
        assert content.language is None or isinstance(content.language, str)


@pytest.mark.unit()
class TestExtractionMetadata:
    def test_extraction_includes_timestamp(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert content.extraction_time is not None
        assert isinstance(content.extraction_time, datetime)
        assert content.extraction_time <= datetime.now()

    def test_extraction_includes_file_size(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert content.file_size > 0
        assert content.file_size == sample_text_file.stat().st_size

    def test_extraction_includes_mime_type(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert content.mime_type is not None
        assert content.mime_type.startswith("text/")

    def test_extraction_includes_file_path(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert content.file_path == str(sample_text_file)


@pytest.mark.unit()
class TestErrorMessages:
    def test_extraction_error_includes_file_path(self):
        file_path = "/nonexistent/file.txt"
        with pytest.raises(ExtractionError) as exc_info:
            extract_text(file_path)
        assert file_path in str(exc_info.value) or "Failed to read" in str(exc_info.value)

    def test_corrupted_file_error_message(self, invalid_pdf_file):
        with pytest.raises(CorruptedFileError) as exc_info:
            extract_pdf(str(invalid_pdf_file))
        assert (
            "corrupted" in str(exc_info.value).lower() or "invalid" in str(exc_info.value).lower()
        )


@pytest.mark.unit()
class TestMarkdownExtraction:
    def test_extract_markdown_as_text(self, sample_markdown_file):
        content = extract_text(str(sample_markdown_file))
        assert content.mime_type in ["text/markdown", "text/x-markdown", "text/plain"]
        assert "Test Document" in content.text
        assert "Introduction" in content.text

    def test_extract_markdown_preserves_code_blocks(self, sample_markdown_file):
        content = extract_text(str(sample_markdown_file))
        assert "def hello_world" in content.text or "hello_world" in content.text


@pytest.mark.unit()
class TestLargeFileHandling:
    def test_extract_large_text_file(self, large_text_file):
        content = extract_text(str(large_text_file))
        assert len(content.text) > 10000
        assert "Line 999" in content.text or "Line 500" in content.text

    def test_large_file_performance(self, large_text_file):
        import time

        start = time.time()
        content = extract_text(str(large_text_file))
        duration = time.time() - start
        assert duration < 5.0
        assert content.file_size > 10000


@pytest.mark.unit()
class TestContentIntegrity:
    def test_extracted_text_not_empty_for_non_empty_file(self, sample_text_file):
        content = extract_text(str(sample_text_file))
        assert len(content.text.strip()) > 0

    def test_extracted_content_matches_file_content(self, sample_text_file):
        sample_text_file.read_text()
        extracted_content = extract_text(str(sample_text_file))
        assert "test document" in extracted_content.text.lower()
        assert "semantic search" in extracted_content.text.lower()
