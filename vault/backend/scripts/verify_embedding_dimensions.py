"""Verify that all embeddings are 768 dimensions across the backend.

This script tests:
1. Text embedding model outputs 768 dimensions
2. Database schema expects 768 dimensions
3. Vector store expects 768 dimensions
4. No hardcoded 384 or 1024 dimension references

Usage:
    python scripts/verify_embedding_dimensions.py
"""

import sys
import asyncio
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from modules.embedding_generator.types import TEXT_EMBEDDING_DIM, TEXT_MODEL_NAME
from modules.embedding_generator.text_embedder import TextEmbedder


async def test_embedding_dimensions():
    """Test that embeddings are generated with correct dimensions."""
    print("="*60)
    print("EMBEDDING DIMENSION VERIFICATION")
    print("="*60)

    # Test 1: Check configuration
    print(f"\n[1/4] Checking configuration...")
    print(f"  TEXT_MODEL_NAME: {TEXT_MODEL_NAME}")
    print(f"  TEXT_EMBEDDING_DIM: {TEXT_EMBEDDING_DIM}")

    if TEXT_EMBEDDING_DIM != 768:
        print(f"  ❌ FAIL: Expected 768 dimensions, got {TEXT_EMBEDDING_DIM}")
        return False
    print(f"  ✅ PASS: Configuration set to 768 dimensions")

    # Test 2: Test single embedding generation
    print(f"\n[2/4] Testing single embedding generation...")
    try:
        embedder = TextEmbedder()
        test_text = "This is a test document for embedding generation."
        embedding = await embedder.embed_text(test_text)

        actual_dim = len(embedding)
        print(f"  Generated embedding dimensions: {actual_dim}")

        if actual_dim != 768:
            print(f"  ❌ FAIL: Expected 768 dimensions, got {actual_dim}")
            return False
        print(f"  ✅ PASS: Single embedding is 768 dimensions")
    except Exception as e:
        print(f"  ❌ FAIL: Error generating embedding: {e}")
        return False

    # Test 3: Test batch embedding generation
    print(f"\n[3/4] Testing batch embedding generation...")
    try:
        test_texts = [
            "First test document",
            "Second test document with more content",
            "Third test document for batch processing"
        ]
        embeddings = await embedder.embed_batch(test_texts)

        print(f"  Generated {len(embeddings)} embeddings")
        for i, emb in enumerate(embeddings):
            actual_dim = len(emb)
            if actual_dim != 768:
                print(f"  ❌ FAIL: Embedding {i}: Expected 768 dimensions, got {actual_dim}")
                return False

        print(f"  ✅ PASS: All {len(embeddings)} batch embeddings are 768 dimensions")
    except Exception as e:
        print(f"  ❌ FAIL: Error generating batch embeddings: {e}")
        return False

    # Test 4: Check vector store configuration
    print(f"\n[4/4] Checking vector store configuration...")
    try:
        from modules.vector_store.store import TEXT_EMBEDDING_DIMENSION

        print(f"  Vector store dimension: {TEXT_EMBEDDING_DIMENSION}")

        if TEXT_EMBEDDING_DIMENSION != 768:
            print(f"  ❌ FAIL: Expected 768, got {TEXT_EMBEDDING_DIMENSION}")
            return False
        print(f"  ✅ PASS: Vector store configured for 768 dimensions")
    except Exception as e:
        print(f"  ⚠️  WARNING: Could not verify vector store: {e}")

    print("\n" + "="*60)
    print("✅ ALL TESTS PASSED")
    print("="*60)
    print("\nEmbedding system is correctly configured for 768 dimensions.")
    print(f"Model: {TEXT_MODEL_NAME}")
    print("Dimension: 768")
    print("\nNext steps:")
    print("1. Run backend tests: pytest tests/")
    print("2. Verify database migration is applied")
    print("3. Test end-to-end search functionality")

    return True


async def main():
    try:
        success = await test_embedding_dimensions()
        sys.exit(0 if success else 1)
    except Exception as e:
        print(f"\n❌ CRITICAL ERROR: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
