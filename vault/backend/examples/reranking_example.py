#!/usr/bin/env python3
"""
Example demonstrating cross-encoder reranking for improved search precision.

This script shows:
1. Basic reranking usage
2. Comparing results with/without reranking
3. Performance measurement
4. Cache effectiveness
"""

import asyncio
import time
import tempfile
from pathlib import Path
from typing import List

from src.modules.reranker.service import RerankService
from src.modules.search_engine.types import SearchResult


def create_sample_documents() -> List[tuple[str, str]]:
    """Create sample documents for testing."""
    documents = [
        (
            "python_tutorial.txt",
            "Python is a high-level programming language known for its simplicity. "
            "It's great for beginners and widely used in web development, data science, and automation."
        ),
        (
            "java_intro.txt",
            "Java is an object-oriented programming language used for building enterprise applications. "
            "It runs on the Java Virtual Machine (JVM) and is platform-independent."
        ),
        (
            "python_web_frameworks.txt",
            "Django and Flask are popular Python web frameworks. Django is a high-level framework "
            "that encourages rapid development, while Flask is a lightweight microframework."
        ),
        (
            "javascript_basics.txt",
            "JavaScript is the programming language of the web. It runs in browsers and enables "
            "interactive web pages. Node.js allows JavaScript to run on servers too."
        ),
        (
            "python_data_science.txt",
            "Python is the dominant language in data science. Libraries like NumPy, pandas, "
            "and scikit-learn make it easy to analyze data and build machine learning models."
        ),
        (
            "cpp_performance.txt",
            "C++ is a high-performance programming language used for system programming, "
            "game development, and applications requiring direct hardware access."
        ),
        (
            "python_automation.txt",
            "Python excels at automation tasks. You can automate file operations, web scraping, "
            "API interactions, and system administration tasks with simple scripts."
        ),
        (
            "ruby_rails.txt",
            "Ruby on Rails is a web application framework written in Ruby. It follows "
            "convention over configuration and is known for developer happiness."
        ),
        (
            "go_concurrency.txt",
            "Go is designed for concurrent programming with goroutines and channels. "
            "It's fast, simple, and great for building scalable network services."
        ),
        (
            "python_machine_learning.txt",
            "Python's machine learning ecosystem includes TensorFlow, PyTorch, and Keras. "
            "These frameworks enable deep learning and neural network development."
        )
    ]

    return documents


def create_temp_files(documents: List[tuple[str, str]]) -> List[SearchResult]:
    """Create temporary files and SearchResult objects."""
    results = []
    temp_dir = Path(tempfile.mkdtemp())

    for i, (filename, content) in enumerate(documents):
        file_path = temp_dir / filename
        file_path.write_text(content)

        result = SearchResult(
            file_id=str(i),
            file_path=str(file_path),
            score=0.5 + (i * 0.02),
            metadata={"filename": filename}
        )
        results.append(result)

    return results


async def example_basic_reranking():
    """Example 1: Basic reranking usage."""
    print("\n" + "=" * 70)
    print("EXAMPLE 1: Basic Reranking")
    print("=" * 70 + "\n")

    documents = create_sample_documents()
    results = create_temp_files(documents)

    service = RerankService(
        model_name="cross-encoder/ms-marco-MiniLM-L-6-v2",
        device="cpu",
        cache_enabled=False
    )

    query = "Python web framework tutorial"

    print(f"Query: '{query}'\n")
    print("Initial ranking (by BM25/embedding score):")
    for i, result in enumerate(results[:5]):
        filename = result.metadata.get("filename", "unknown")
        print(f"  {i+1}. {filename} (score: {result.score:.4f})")

    print("\nReranking...")
    start = time.time()
    reranked = await service.rerank(query, results, top_k=5, timeout=10.0)
    duration = (time.time() - start) * 1000

    print(f"Reranking took: {duration:.2f}ms\n")
    print("After reranking:")
    for i, result in enumerate(reranked):
        filename = result.metadata.get("filename", "unknown")
        print(f"  {i+1}. {filename} (score: {result.score:.4f})")

    cleanup_temp_files(results)


async def example_comparison():
    """Example 2: Compare results with and without reranking."""
    print("\n" + "=" * 70)
    print("EXAMPLE 2: Precision Comparison")
    print("=" * 70 + "\n")

    documents = create_sample_documents()
    results = create_temp_files(documents)

    service = RerankService(
        model_name="BAAI/bge-reranker-v2-m3",
        device="cpu",
        cache_enabled=False
    )

    queries = [
        "Python for data science and machine learning",
        "web development frameworks",
        "high performance programming languages"
    ]

    for query in queries:
        print(f"\nQuery: '{query}'")
        print("-" * 70)

        top_without_rerank = sorted(results, key=lambda x: x.score, reverse=True)[:3]
        print("\nTop 3 without reranking:")
        for i, result in enumerate(top_without_rerank):
            filename = result.metadata.get("filename", "unknown")
            print(f"  {i+1}. {filename}")

        reranked = await service.rerank(query, results, top_k=3, timeout=10.0)
        print("\nTop 3 with reranking:")
        for i, result in enumerate(reranked):
            filename = result.metadata.get("filename", "unknown")
            print(f"  {i+1}. {filename} ⭐")

    cleanup_temp_files(results)


async def example_performance():
    """Example 3: Performance measurement."""
    print("\n" + "=" * 70)
    print("EXAMPLE 3: Performance Measurement")
    print("=" * 70 + "\n")

    documents = create_sample_documents()
    results = create_temp_files(documents)

    models = [
        "cross-encoder/ms-marco-MiniLM-L-6-v2",
        "BAAI/bge-reranker-v2-m3"
    ]

    query = "Python programming tutorial"

    for model_name in models:
        print(f"\nTesting model: {model_name}")
        print("-" * 70)

        service = RerankService(
            model_name=model_name,
            device="cpu",
            cache_enabled=False
        )

        latencies = []
        for i in range(5):
            start = time.time()
            await service.rerank(query, results, top_k=5, timeout=10.0)
            latency = (time.time() - start) * 1000
            latencies.append(latency)

        avg_latency = sum(latencies) / len(latencies)
        min_latency = min(latencies)
        max_latency = max(latencies)

        print(f"  Average latency: {avg_latency:.2f}ms")
        print(f"  Min latency: {min_latency:.2f}ms")
        print(f"  Max latency: {max_latency:.2f}ms")
        print(f"  Throughput: {1000/avg_latency:.2f} queries/sec")

    cleanup_temp_files(results)


async def example_cache_effectiveness():
    """Example 4: Cache effectiveness demonstration."""
    print("\n" + "=" * 70)
    print("EXAMPLE 4: Cache Effectiveness")
    print("=" * 70 + "\n")

    documents = create_sample_documents()
    results = create_temp_files(documents)

    service = RerankService(
        model_name="cross-encoder/ms-marco-MiniLM-L-6-v2",
        device="cpu",
        cache_enabled=True,
        cache_ttl=3600
    )

    query = "Python web framework"

    print("First query (cache miss):")
    start = time.time()
    await service.rerank(query, results, top_k=5, timeout=10.0)
    first_latency = (time.time() - start) * 1000
    print(f"  Latency: {first_latency:.2f}ms")

    print("\nRepeated queries (cache hits):")
    cache_latencies = []
    for i in range(5):
        start = time.time()
        await service.rerank(query, results, top_k=5, timeout=10.0)
        latency = (time.time() - start) * 1000
        cache_latencies.append(latency)
        print(f"  Query {i+1}: {latency:.2f}ms")

    avg_cache_latency = sum(cache_latencies) / len(cache_latencies)
    speedup = first_latency / avg_cache_latency

    print(f"\nCache performance:")
    print(f"  First query (no cache): {first_latency:.2f}ms")
    print(f"  Avg cached query: {avg_cache_latency:.2f}ms")
    print(f"  Speedup: {speedup:.2f}x")

    model_info = RerankService.get_model_info()
    print(f"\nCache stats:")
    for model_name, info in model_info.items():
        print(f"  Model: {model_name}")
        print(f"  Cache size: {info['cache_size']} entries")

    cleanup_temp_files(results)


async def example_timeout_handling():
    """Example 5: Timeout and fallback behavior."""
    print("\n" + "=" * 70)
    print("EXAMPLE 5: Timeout Handling")
    print("=" * 70 + "\n")

    documents = create_sample_documents()
    results = create_temp_files(documents)

    service = RerankService(
        model_name="BAAI/bge-reranker-v2-m3",
        device="cpu",
        cache_enabled=False
    )

    query = "test query"

    print("Normal reranking (sufficient timeout):")
    start = time.time()
    reranked = await service.rerank(query, results, top_k=5, timeout=10.0)
    duration = (time.time() - start) * 1000
    print(f"  Completed in: {duration:.2f}ms")
    print(f"  Results returned: {len(reranked)}")

    print("\nWith aggressive timeout (may timeout):")
    start = time.time()
    reranked = await service.rerank(query, results, top_k=5, timeout=0.001)
    duration = (time.time() - start) * 1000
    print(f"  Completed in: {duration:.2f}ms")
    print(f"  Results returned: {len(reranked)}")
    print("  Note: Falls back to original results on timeout")

    cleanup_temp_files(results)


def cleanup_temp_files(results: List[SearchResult]):
    """Clean up temporary files."""
    for result in results:
        try:
            Path(result.file_path).unlink()
        except Exception:
            pass

    try:
        temp_dir = Path(results[0].file_path).parent
        temp_dir.rmdir()
    except Exception:
        pass


async def main():
    """Run all examples."""
    print("\n" + "=" * 70)
    print("CROSS-ENCODER RERANKING EXAMPLES")
    print("=" * 70)

    try:
        await example_basic_reranking()
        await example_comparison()
        await example_performance()
        await example_cache_effectiveness()
        await example_timeout_handling()

        print("\n" + "=" * 70)
        print("All examples completed successfully!")
        print("=" * 70 + "\n")

    except KeyboardInterrupt:
        print("\n\nExamples interrupted by user.")
    except Exception as e:
        print(f"\n\nError running examples: {e}")
        import traceback
        traceback.print_exc()


if __name__ == "__main__":
    asyncio.run(main())
