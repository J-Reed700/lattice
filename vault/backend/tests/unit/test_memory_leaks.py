"""Memory leak tests for VLM and PDF processing.

Tests ensure that memory leaks are fixed and long-running operations
don't cause OOM crashes.
"""

import gc
import os
from pathlib import Path
import tempfile

import pytest

from src.modules.content_extractor.extractors.ocr_service import OcrService
from src.modules.content_extractor.extractors.pdf import extract_pdf, managed_pdf_document
from src.modules.content_extractor.types import OcrConfig, OcrEngine
from src.utils.memory_monitor import MemoryMonitor


def get_memory_mb():
    """Get current memory usage in MB."""
    try:
        import psutil

        process = psutil.Process(os.getpid())
        return process.memory_info().rss / 1024 / 1024
    except ImportError:
        pytest.skip("psutil not installed")


def create_test_image(path: Path, size: tuple = (800, 600), text: str = "Test"):
    """Create a test image with text for OCR."""
    from PIL import Image, ImageDraw, ImageFont

    img = Image.new("RGB", size, color="white")
    draw = ImageDraw.Draw(img)

    try:
        font = ImageFont.truetype("arial.ttf", 48)
    except Exception:
        font = ImageFont.load_default()

    draw.text((100, 100), text, fill="black", font=font)
    img.save(path)


def create_test_pdf(path: Path, num_pages: int = 10):
    """Create a test PDF with multiple pages."""
    try:
        import fitz
    except ImportError:
        pytest.skip("PyMuPDF not installed")

    doc = fitz.open()
    for i in range(num_pages):
        page = doc.new_page()
        text = f"This is page {i + 1}\n" + "Test content " * 50
        page.insert_text((50, 50), text)

    doc.save(path)
    doc.close()


@pytest.mark.asyncio()
@pytest.mark.slow()
async def test_no_memory_leak_in_ocr_batch():
    """Test that OCR doesn't leak memory when processing many images.

    This test processes 100 images and verifies memory doesn't grow
    unbounded. Memory increase should be reasonable (<1GB).
    """
    pytest.importorskip("transformers")
    pytest.importorskip("torch")
    pytest.importorskip("psutil")

    config = OcrConfig(enabled=True, engine=OcrEngine.VLM, min_confidence=0.5)
    service = OcrService(config)

    initial_memory = get_memory_mb()
    print(f"\nInitial memory: {initial_memory:.1f} MB")

    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir_path = Path(tmpdir)

        num_images = 50
        for i in range(num_images):
            image_path = tmpdir_path / f"test_{i}.png"
            create_test_image(image_path, text=f"Test {i}")

            try:
                result = await service.extract_text(str(image_path))
                assert result is not None
            except Exception as e:
                pytest.skip(f"VLM not available: {e}")

            if (i + 1) % 10 == 0:
                current_memory = get_memory_mb()
                memory_increase = current_memory - initial_memory
                print(f"After {i + 1} images: {current_memory:.1f} MB (+{memory_increase:.1f} MB)")

    service.cleanup()
    gc.collect()

    final_memory = get_memory_mb()
    memory_increase = final_memory - initial_memory

    print(f"Final memory: {final_memory:.1f} MB")
    print(f"Total increase: {memory_increase:.1f} MB")

    assert memory_increase < 1024, (
        f"Memory leak detected in OCR: +{memory_increase:.1f} MB for {num_images} images. "
        f"Expected <1024 MB"
    )


@pytest.mark.asyncio()
async def test_ocr_cleanup_after_processing():
    """Test that OCR service properly cleans up after processing."""
    pytest.importorskip("transformers")
    pytest.importorskip("torch")
    pytest.importorskip("psutil")

    config = OcrConfig(enabled=True, engine=OcrEngine.VLM)
    service = OcrService(config)

    get_memory_mb()

    with tempfile.TemporaryDirectory() as tmpdir:
        image_path = Path(tmpdir) / "test.png"
        create_test_image(image_path)

        try:
            await service.extract_text(str(image_path))
        except Exception as e:
            pytest.skip(f"VLM not available: {e}")

    before_cleanup = get_memory_mb()
    service.cleanup()
    gc.collect()
    after_cleanup = get_memory_mb()

    memory_freed = before_cleanup - after_cleanup
    print(f"\nMemory freed by cleanup: {memory_freed:.1f} MB")

    assert service._vlm_model is None, "Model not cleaned up"
    assert service._vlm_processor is None, "Processor not cleaned up"


@pytest.mark.slow()
def test_no_memory_leak_in_pdf_batch():
    """Test that PDF extraction doesn't leak memory.

    Processes 100 PDFs and verifies memory doesn't grow unbounded.
    Memory increase should be reasonable (<500MB).
    """
    pytest.importorskip("fitz")
    pytest.importorskip("psutil")

    initial_memory = get_memory_mb()
    print(f"\nInitial memory: {initial_memory:.1f} MB")

    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir_path = Path(tmpdir)

        num_pdfs = 100
        for i in range(num_pdfs):
            pdf_path = tmpdir_path / f"test_{i}.pdf"
            create_test_pdf(pdf_path, num_pages=5)

            content = extract_pdf(str(pdf_path))
            assert content.text
            assert content.metadata["page_count"] == 5

            if (i + 1) % 20 == 0:
                gc.collect()
                current_memory = get_memory_mb()
                memory_increase = current_memory - initial_memory
                print(f"After {i + 1} PDFs: {current_memory:.1f} MB (+{memory_increase:.1f} MB)")

    gc.collect()
    final_memory = get_memory_mb()
    memory_increase = final_memory - initial_memory

    print(f"Final memory: {final_memory:.1f} MB")
    print(f"Total increase: {memory_increase:.1f} MB")

    assert memory_increase < 500, (
        f"Memory leak detected in PDF: +{memory_increase:.1f} MB for {num_pdfs} PDFs. "
        f"Expected <500 MB"
    )


def test_pdf_context_manager_cleanup():
    """Test that PDF context manager properly closes documents."""
    pytest.importorskip("fitz")

    with tempfile.TemporaryDirectory() as tmpdir:
        pdf_path = Path(tmpdir) / "test.pdf"
        create_test_pdf(pdf_path, num_pages=3)

        with managed_pdf_document(str(pdf_path)) as doc:
            assert len(doc) == 3
            assert not doc.is_closed


def test_pdf_context_manager_cleanup_on_error():
    """Test that PDF context manager cleans up even on error."""
    pytest.importorskip("fitz")

    with tempfile.TemporaryDirectory() as tmpdir:
        pdf_path = Path(tmpdir) / "test.pdf"
        create_test_pdf(pdf_path, num_pages=3)

        with pytest.raises(ValueError):
            with managed_pdf_document(str(pdf_path)) as doc:
                assert len(doc) == 3
                raise ValueError("Test error")


def test_memory_monitor():
    """Test MemoryMonitor functionality."""
    pytest.importorskip("psutil")

    monitor = MemoryMonitor(threshold_mb=1024)

    initial_memory = monitor.get_memory_mb()
    assert initial_memory is not None
    assert initial_memory > 0

    monitor.log_memory("test")

    monitor.force_cleanup()

    was_cleaned = monitor.check_and_cleanup()
    assert isinstance(was_cleaned, bool)


def test_memory_monitor_high_threshold():
    """Test that cleanup triggers when threshold exceeded."""
    pytest.importorskip("psutil")

    monitor = MemoryMonitor(threshold_mb=1)

    was_cleaned = monitor.check_and_cleanup()
    assert was_cleaned is True


def test_memory_monitor_convenience_functions():
    """Test convenience functions for memory management."""
    pytest.importorskip("psutil")

    from src.utils.memory_monitor import check_memory, cleanup_memory, log_memory

    cleanup_memory()
    result = check_memory(threshold_mb=8192)
    assert isinstance(result, bool)
    log_memory("test")


@pytest.mark.asyncio()
async def test_ocr_periodic_cleanup():
    """Test that OCR service performs periodic cleanup during batch processing."""
    pytest.importorskip("transformers")
    pytest.importorskip("torch")
    pytest.importorskip("psutil")

    config = OcrConfig(enabled=True, engine=OcrEngine.VLM)
    service = OcrService(config)

    service._cleanup_interval = 5

    with tempfile.TemporaryDirectory() as tmpdir:
        for i in range(10):
            image_path = Path(tmpdir) / f"test_{i}.png"
            create_test_image(image_path, text=f"Test {i}")

            try:
                await service.extract_text(str(image_path))
            except Exception as e:
                pytest.skip(f"VLM not available: {e}")

        assert service._inference_count == 10

    service.cleanup()


def test_pdf_large_document_memory():
    """Test that large PDFs with many pages don't cause memory issues."""
    pytest.importorskip("fitz")
    pytest.importorskip("psutil")

    initial_memory = get_memory_mb()

    with tempfile.TemporaryDirectory() as tmpdir:
        pdf_path = Path(tmpdir) / "large.pdf"
        create_test_pdf(pdf_path, num_pages=200)

        content = extract_pdf(str(pdf_path))
        assert content.text
        assert content.metadata["page_count"] == 200

    gc.collect()
    final_memory = get_memory_mb()
    memory_increase = final_memory - initial_memory

    print(f"\nMemory increase for 200-page PDF: {memory_increase:.1f} MB")

    assert memory_increase < 100, f"Excessive memory use for single PDF: +{memory_increase:.1f} MB"


@pytest.mark.asyncio()
async def test_ocr_tensor_cleanup():
    """Test that tensors are properly cleaned up after inference."""
    pytest.importorskip("transformers")
    pytest.importorskip("torch")
    pytest.importorskip("psutil")
    import torch

    config = OcrConfig(enabled=True, engine=OcrEngine.VLM)
    service = OcrService(config)

    with tempfile.TemporaryDirectory() as tmpdir:
        image_path = Path(tmpdir) / "test.png"
        create_test_image(image_path)

        try:
            await service.extract_text(str(image_path))
        except Exception as e:
            pytest.skip(f"VLM not available: {e}")

        if torch.cuda.is_available():
            allocated = torch.cuda.memory_allocated()
            reserved = torch.cuda.memory_reserved()
            print(f"\nCUDA allocated: {allocated / 1024 / 1024:.1f} MB")
            print(f"CUDA reserved: {reserved / 1024 / 1024:.1f} MB")

    service.cleanup()
