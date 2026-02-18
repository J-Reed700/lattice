import asyncio
from datetime import datetime
from typing import Any

from ..detector import check_file_size, detect_mime_type
from ..types import CorruptedFileError, ExtractedContent, ExtractionError, OcrConfig
from . import register_extractor
from .ocr_service import OcrResult, get_ocr_service


@register_extractor("image/jpeg", "image/png", "image/gif", "image/bmp", "image/webp", "image/tiff")
async def extract_image(file_path: str, ocr_config: OcrConfig | None = None) -> ExtractedContent:
    """Extract metadata and text from image files using Pillow and OCR.

    Extracts EXIF data, image properties, and optionally performs OCR
    text extraction using Vision Language Models or Tesseract.

    Args:
        file_path: Path to the image file
        ocr_config: OCR configuration (uses default if None)

    Returns:
        ExtractedContent with extracted text and EXIF metadata

    Raises:
        CorruptedFileError: If image file is corrupted
        ExtractionError: If extraction fails
        FileSizeLimitError: If file exceeds size limit

    Example:
        >>> content = extract_image('/path/to/document.jpg')
        >>> assert 'width' in content.metadata
        >>> assert 'height' in content.metadata

        >>> config = OcrConfig(enabled=True)
        >>> content = extract_image('/path/to/screenshot.png', config)
        >>> print(content.text)
    """
    try:
        from PIL import Image
    except ImportError as e:
        raise ExtractionError(
            "Pillow library not installed. Install with: pip install Pillow", file_path=file_path
        ) from e

    try:
        mime_type = detect_mime_type(file_path)
        file_size = check_file_size(file_path, mime_type)

        try:
            with Image.open(file_path) as img:
                metadata = {
                    "width": img.width,
                    "height": img.height,
                    "format": img.format,
                    "mode": img.mode,
                }

                exif_data = _extract_exif(img)
                if exif_data:
                    metadata["exif"] = exif_data
        except Exception as e:
            raise CorruptedFileError(
                f"Failed to open image file: {e!s}", file_path=file_path
            ) from e

        extracted_text = ""
        ocr_result: OcrResult | None = None

        if ocr_config is None:
            ocr_config = OcrConfig()

        if ocr_config.enabled:
            try:
                ocr_service = get_ocr_service(ocr_config)
                try:
                    asyncio.get_running_loop()
                    ocr_result = await ocr_service.extract_text(file_path)
                except RuntimeError:
                    ocr_result = asyncio.run(ocr_service.extract_text(file_path))
                extracted_text = ocr_result.text

                metadata["ocr"] = {
                    "has_text": ocr_result.has_text,
                    "confidence": ocr_result.confidence,
                    "engine": ocr_result.engine_used,
                    "word_count": ocr_result.word_count,
                    "language": ocr_result.language,
                }
            except ExtractionError:
                metadata["ocr"] = {
                    "has_text": False,
                    "confidence": 0.0,
                    "engine": "failed",
                    "word_count": 0,
                    "error": "OCR extraction failed, falling back to metadata only",
                }

        return ExtractedContent(
            text=extracted_text,
            mime_type=mime_type,
            file_path=file_path,
            metadata=metadata,
            encoding=None,
            language=ocr_result.language if ocr_result else None,
            extraction_time=datetime.now(),
            file_size=file_size,
        )

    except (CorruptedFileError, ExtractionError):
        raise
    except Exception as e:
        raise ExtractionError(
            f"Failed to extract image metadata: {e!s}", file_path=file_path
        ) from e


def _extract_exif(img) -> dict[str, Any]:
    """Extract EXIF data from image.

    Args:
        img: PIL Image object

    Returns:
        Dictionary with EXIF data
    """
    exif_data = {}

    try:
        exif = img.getexif()
        if not exif:
            return exif_data

        from PIL.ExifTags import TAGS

        for tag_id, value in exif.items():
            tag = TAGS.get(tag_id, tag_id)

            if isinstance(value, bytes):
                try:
                    value = value.decode("utf-8", errors="ignore")
                except Exception:
                    continue

            if isinstance(tag, str):
                exif_data[tag] = value

    except Exception:
        pass

    return exif_data
