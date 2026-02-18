from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any


class OcrEngine(str, Enum):
    """OCR engine options for image text extraction."""

    VLM = "vlm"
    TESSERACT = "tesseract"
    DISABLED = "disabled"


class VlmModel(str, Enum):
    """VLM model options for OCR."""

    FLORENCE2_BASE = "florence2-base"
    FLORENCE2_LARGE = "florence2-large"
    QWEN2_VL_2B = "qwen2-vl-2b"
    QWEN2_VL_7B = "qwen2-vl-7b"


@dataclass
class OcrConfig:
    """Configuration for OCR extraction from images.

    Attributes:
        enabled: Whether OCR is enabled
        engine: OCR engine to use (vlm, tesseract, or disabled)
        min_confidence: Minimum confidence threshold for extracted text (0.0-1.0)
        detect_text_first: Whether to detect if image contains text before OCR
        max_image_size: Maximum image dimension for processing (pixels)
        model_name: VLM model name for OCR (e.g., 'microsoft/Florence-2-base')
        languages: List of language codes for OCR (e.g., ['eng', 'fra'])
        vlm_model: VLM model selection (florence2-base, qwen2-vl-2b, etc.)
        use_quantization: Enable 4-bit quantization for memory efficiency
        max_new_tokens: Maximum tokens to generate in VLM response
        temperature: Temperature for VLM generation (0.0-1.0)
        device: Device to use for inference ('cuda', 'cpu', 'auto')

    Example:
        >>> config = OcrConfig(enabled=True, engine=OcrEngine.VLM)
        >>> config.min_confidence
        0.5
    """

    enabled: bool = True
    engine: OcrEngine = OcrEngine.VLM
    min_confidence: float = 0.5
    detect_text_first: bool = True
    max_image_size: int = 2048
    model_name: str = "microsoft/Florence-2-base"
    languages: list[str] = field(default_factory=lambda: ["eng"])
    vlm_model: VlmModel = VlmModel.QWEN2_VL_2B
    use_quantization: bool = True
    max_new_tokens: int = 1024
    temperature: float = 0.0
    device: str = "auto"


@dataclass
class ExtractedContent:
    """Extracted content from a file with metadata.

    Attributes:
        text: Extracted text content
        mime_type: MIME type of the file
        file_path: Path to the source file
        metadata: File-specific metadata (author, page_count, etc.)
        encoding: Text encoding detected (for text files)
        language: Detected language code (ISO 639-1), None for MVP
        extraction_time: When extraction occurred
        file_size: Size of original file in bytes

    Example:
        >>> content = ExtractedContent(
        ...     text="Document content here",
        ...     mime_type="text/plain",
        ...     file_path="/path/to/file.txt",
        ...     metadata={},
        ...     encoding="utf-8",
        ...     language=None,
        ...     extraction_time=datetime.now(),
        ...     file_size=1024
        ... )
    """

    text: str
    mime_type: str
    file_path: str
    metadata: dict[str, Any] = field(default_factory=dict)
    encoding: str | None = None
    language: str | None = None
    extraction_time: datetime = field(default_factory=datetime.now)
    file_size: int = 0


class ExtractionError(Exception):
    """Base exception for all extraction errors.

    Raised when content extraction fails for any reason.
    Always includes the file path in the error message.
    """

    def __init__(self, message: str, file_path: str | None = None):
        self.file_path = file_path
        if file_path:
            message = f"{message} (file: {file_path})"
        super().__init__(message)


class UnsupportedFileTypeError(ExtractionError):
    """File type is not supported for extraction.

    Raised when attempting to extract content from a file
    with an unsupported MIME type.
    """


class CorruptedFileError(ExtractionError):
    """File is corrupted or malformed.

    Raised when file structure is invalid or cannot be parsed.
    """


class FileSizeLimitError(ExtractionError):
    """File exceeds maximum size limit.

    Raised when file size is larger than allowed limits.
    Default: 100MB for most files, 500MB for PDFs.
    """


class PasswordProtectedError(ExtractionError):
    """File is password protected.

    Raised when attempting to extract from encrypted/protected files.
    """
