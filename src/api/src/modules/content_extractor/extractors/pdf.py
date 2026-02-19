from contextlib import contextmanager
from datetime import datetime
from pathlib import Path
from typing import Any

from ..detector import check_file_size, detect_mime_type
from ..types import CorruptedFileError, ExtractedContent, ExtractionError, PasswordProtectedError
from ..utils import detect_language, normalize_text
from . import register_extractor


@contextmanager
def managed_pdf_document(file_path: str):
    """Context manager for PDF documents ensuring proper cleanup.

    Args:
        file_path: Path to PDF file

    Yields:
        PyMuPDF document object

    Ensures:
        Document is always closed, even if exceptions occur
    """
    import fitz

    doc = None
    try:
        doc = fitz.open(file_path)
        yield doc
    finally:
        if doc is not None:
            doc.close()
            del doc


@register_extractor("application/pdf")
def extract_pdf(file_path: str) -> ExtractedContent:
    """Extract content from PDF files using PyMuPDF.

    Extracts text page by page for memory efficiency.
    Handles password protection and corruption detection.
    Extracts metadata including author, title, page count, etc.

    Args:
        file_path: Path to the PDF file

    Returns:
        ExtractedContent with extracted text and PDF metadata

    Raises:
        PasswordProtectedError: If PDF is encrypted/password-protected
        CorruptedFileError: If PDF structure is invalid
        ExtractionError: If extraction fails
        FileSizeLimitError: If file exceeds 500MB limit

    Example:
        >>> content = extract_pdf('/path/to/document.pdf')
        >>> assert content.mime_type == 'application/pdf'
        >>> assert 'page_count' in content.metadata
        >>> assert 'author' in content.metadata
    """
    import gc

    try:
        import fitz
    except ImportError as e:
        raise ExtractionError(
            "PyMuPDF (fitz) library not installed. Install with: pip install pymupdf",
            file_path=file_path,
        ) from e

    try:
        mime_type = detect_mime_type(file_path)
        file_size = check_file_size(file_path, mime_type)

        with managed_pdf_document(file_path) as doc:
            if doc.is_encrypted:
                raise PasswordProtectedError("PDF is password protected", file_path=file_path)

            text_parts = []
            for page_num in range(len(doc)):
                try:
                    page = doc[page_num]
                    page_text = page.get_text()
                    text_parts.append(page_text)

                    del page_text
                    if page_num % 10 == 0:
                        gc.collect()

                except Exception as e:
                    raise CorruptedFileError(
                        f"Failed to extract text from page {page_num + 1}: {e!s}",
                        file_path=file_path,
                    ) from e

            text = "\n\n".join(text_parts)
            text = normalize_text(text)

            metadata = _extract_pdf_metadata(doc)
            metadata["page_count"] = len(doc)
            metadata["file_name"] = Path(file_path).name
            metadata["file_path"] = str(Path(file_path).absolute())

            del text_parts
            gc.collect()

            language = detect_language(text)

            return ExtractedContent(
                text=text,
                mime_type=mime_type,
                file_path=file_path,
                metadata=metadata,
                encoding=None,
                language=language,
                extraction_time=datetime.now(),
                file_size=file_size,
            )

    except fitz.FileDataError as e:
        if "password" in str(e).lower() or "encrypted" in str(e).lower():
            raise PasswordProtectedError("PDF is password protected", file_path=file_path) from e
        raise CorruptedFileError(
            f"PDF file is corrupted or invalid: {e!s}", file_path=file_path
        ) from e
    except (PasswordProtectedError, CorruptedFileError):
        raise
    except Exception as e:
        raise ExtractionError(f"Failed to extract PDF content: {e!s}", file_path=file_path) from e


def _extract_pdf_metadata(doc) -> dict[str, Any]:
    """Extract metadata from PDF document.

    Args:
        doc: PyMuPDF document object

    Returns:
        Dictionary with metadata fields
    """
    metadata = {}

    pdf_metadata = doc.metadata
    if pdf_metadata:
        if pdf_metadata.get("author"):
            metadata["author"] = pdf_metadata["author"]
        if pdf_metadata.get("title"):
            metadata["title"] = pdf_metadata["title"]
        if pdf_metadata.get("subject"):
            metadata["subject"] = pdf_metadata["subject"]
        if pdf_metadata.get("keywords"):
            metadata["keywords"] = pdf_metadata["keywords"]
        if pdf_metadata.get("creator"):
            metadata["creator"] = pdf_metadata["creator"]
        if pdf_metadata.get("producer"):
            metadata["producer"] = pdf_metadata["producer"]
        if pdf_metadata.get("creationDate"):
            metadata["creation_date"] = pdf_metadata["creationDate"]
        if pdf_metadata.get("modDate"):
            metadata["modification_date"] = pdf_metadata["modDate"]

    return metadata
