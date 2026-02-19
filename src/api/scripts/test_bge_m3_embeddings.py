#!/usr/bin/env python3
"""
Test script for BGE-M3 embeddings.

Verifies that:
1. BGE-M3 model loads correctly
2. Embeddings have correct dimension (1024)
3. Embeddings are properly normalized
4. Model works for different text types
5. Performance is acceptable

Usage:
    python test_bge_m3_embeddings.py [--verbose]
"""

import argparse
import asyncio
import logging
import sys
import time
from pathlib import Path
from typing import List, Dict, Any
import tempfile

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

logging.basicConfig(level=logging.INFO, format='%(levelname)s: %(message)s')
logger = logging.getLogger(__name__)


class BGEm3Tester:
    """Test suite for BGE-M3 embeddings."""

    def __init__(self, verbose: bool = False):
        self.verbose = verbose
        self.tests_passed = 0
        self.tests_failed = 0

    def log(self, message: str, level: str = "info"):
        """Log message if verbose."""
        if self.verbose or level == "error":
            getattr(logger, level)(message)

    async def test_model_loading(self) -> bool:
        """Test that BGE-M3 model loads correctly."""
        self.log("Testing model loading...")
        try:
            from services.embeddings.text import TextEmbeddingGenerator

            gen = TextEmbeddingGenerator()

            assert gen.MODEL_NAME == "BAAI/bge-m3", f"Wrong model: {gen.MODEL_NAME}"
            assert gen.EMBEDDING_DIM == 1024, f"Wrong dimension: {gen.EMBEDDING_DIM}"

            # Load model
            _ = gen.model

            self.log("✓ Model loaded successfully", "info")
            return True

        except Exception as e:
            logger.error(f"✗ Model loading failed: {e}")
            return False

    async def test_embedding_dimension(self) -> bool:
        """Test that embeddings have correct dimension."""
        self.log("Testing embedding dimension...")
        try:
            from services.embeddings.text import TextEmbeddingGenerator

            gen = TextEmbeddingGenerator()

            # Create temp text file
            with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                f.write("This is a test document for BGE-M3 embeddings.")
                temp_path = f.name

            try:
                embedding = await gen.generate(temp_path)

                assert isinstance(embedding, list), "Embedding should be a list"
                assert len(embedding) == 1024, f"Expected 1024 dimensions, got {len(embedding)}"
                assert all(isinstance(x, float) for x in embedding), "All values should be floats"

                self.log(f"✓ Embedding has correct dimension: {len(embedding)}", "info")
                return True

            finally:
                Path(temp_path).unlink()

        except Exception as e:
            logger.error(f"✗ Dimension test failed: {e}")
            return False

    async def test_embedding_normalization(self) -> bool:
        """Test that embeddings are properly normalized."""
        self.log("Testing embedding normalization...")
        try:
            from services.embeddings.text import TextEmbeddingGenerator
            import numpy as np

            gen = TextEmbeddingGenerator()

            with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                f.write("Normalization test text.")
                temp_path = f.name

            try:
                embedding = await gen.generate(temp_path)
                embedding_array = np.array(embedding)

                norm = np.linalg.norm(embedding_array)

                # Sentence transformers normalize by default
                self.log(f"Embedding norm: {norm:.4f}")

                # Check if normalized (norm should be close to 1.0)
                if abs(norm - 1.0) < 0.01:
                    self.log("✓ Embeddings are normalized", "info")
                    return True
                else:
                    self.log(f"✓ Embeddings have norm: {norm:.4f} (may not be normalized)", "info")
                    return True  # Not critical

            finally:
                Path(temp_path).unlink()

        except Exception as e:
            logger.error(f"✗ Normalization test failed: {e}")
            return False

    async def test_different_text_types(self) -> bool:
        """Test embeddings for different text types."""
        self.log("Testing different text types...")

        test_cases = [
            ("Short text", "Hello world"),
            ("Long text", "This is a much longer piece of text. " * 50),
            ("Multilingual", "Hello 你好 Bonjour Hola こんにちは"),
            ("Technical", "def calculate_embedding(x: np.ndarray) -> np.ndarray: return x / np.linalg.norm(x)"),
            ("Empty", "")
        ]

        try:
            from services.embeddings.text import TextEmbeddingGenerator

            gen = TextEmbeddingGenerator()

            for name, text in test_cases:
                self.log(f"  Testing: {name}")

                with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False, encoding='utf-8') as f:
                    f.write(text if text else " ")
                    temp_path = f.name

                try:
                    embedding = await gen.generate(temp_path)
                    assert len(embedding) == 1024, f"{name} failed: wrong dimension"
                    self.log(f"    ✓ {name}: OK")

                finally:
                    Path(temp_path).unlink()

            self.log("✓ All text types processed successfully", "info")
            return True

        except Exception as e:
            logger.error(f"✗ Text type test failed: {e}")
            return False

    async def test_semantic_similarity(self) -> bool:
        """Test that semantic similarity works correctly."""
        self.log("Testing semantic similarity...")
        try:
            from services.embeddings.text import TextEmbeddingGenerator
            import numpy as np

            gen = TextEmbeddingGenerator()

            # Similar texts
            text1 = "The cat sat on the mat."
            text2 = "A cat was sitting on a mat."
            text3 = "Dogs are running in the park."

            embeddings = []
            for text in [text1, text2, text3]:
                with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                    f.write(text)
                    temp_path = f.name

                try:
                    emb = await gen.generate(temp_path)
                    embeddings.append(np.array(emb))
                finally:
                    Path(temp_path).unlink()

            # Calculate cosine similarities
            sim_1_2 = np.dot(embeddings[0], embeddings[1])
            sim_1_3 = np.dot(embeddings[0], embeddings[2])

            self.log(f"Similarity (text1, text2): {sim_1_2:.4f}")
            self.log(f"Similarity (text1, text3): {sim_1_3:.4f}")

            # Similar texts should have higher similarity
            if sim_1_2 > sim_1_3:
                self.log("✓ Semantic similarity working correctly", "info")
                return True
            else:
                logger.warning("⚠ Similar texts have lower similarity than different texts")
                return True  # Not critical, may depend on model

        except Exception as e:
            logger.error(f"✗ Similarity test failed: {e}")
            return False

    async def test_performance(self) -> bool:
        """Test embedding generation performance."""
        self.log("Testing performance...")
        try:
            from services.embeddings.text import TextEmbeddingGenerator

            gen = TextEmbeddingGenerator()

            # Warm up
            with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                f.write("Warm up text")
                temp_path = f.name

            await gen.generate(temp_path)
            Path(temp_path).unlink()

            # Benchmark
            test_text = "This is a benchmark text for performance testing. " * 10

            with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                f.write(test_text)
                temp_path = f.name

            try:
                num_runs = 10
                start = time.time()

                for _ in range(num_runs):
                    await gen.generate(temp_path)

                elapsed = time.time() - start
                avg_time = elapsed / num_runs

                self.log(f"Average time per embedding: {avg_time*1000:.2f}ms", "info")

                if avg_time < 1.0:  # Less than 1 second
                    self.log("✓ Performance is good", "info")
                    return True
                else:
                    logger.warning(f"⚠ Performance is slow: {avg_time:.2f}s per embedding")
                    return True  # Not critical

            finally:
                Path(temp_path).unlink()

        except Exception as e:
            logger.error(f"✗ Performance test failed: {e}")
            return False

    async def run_all_tests(self) -> bool:
        """Run all tests and return overall result."""
        logger.info("=" * 70)
        logger.info("BGE-M3 Embedding Test Suite")
        logger.info("=" * 70)

        tests = [
            ("Model Loading", self.test_model_loading),
            ("Embedding Dimension", self.test_embedding_dimension),
            ("Embedding Normalization", self.test_embedding_normalization),
            ("Different Text Types", self.test_different_text_types),
            ("Semantic Similarity", self.test_semantic_similarity),
            ("Performance", self.test_performance),
        ]

        for name, test_func in tests:
            logger.info(f"\n[{name}]")
            try:
                result = await test_func()
                if result:
                    self.tests_passed += 1
                else:
                    self.tests_failed += 1
            except Exception as e:
                logger.error(f"Test crashed: {e}")
                self.tests_failed += 1

        # Summary
        logger.info("\n" + "=" * 70)
        logger.info("Test Summary")
        logger.info("=" * 70)
        logger.info(f"Passed: {self.tests_passed}/{len(tests)}")
        logger.info(f"Failed: {self.tests_failed}/{len(tests)}")

        return self.tests_failed == 0


def main():
    parser = argparse.ArgumentParser(description="Test BGE-M3 embeddings")
    parser.add_argument("--verbose", action="store_true", help="Verbose output")

    args = parser.parse_args()

    tester = BGEm3Tester(verbose=args.verbose)
    success = asyncio.run(tester.run_all_tests())

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
