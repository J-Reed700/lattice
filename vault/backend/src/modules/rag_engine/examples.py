"""
Agentic RAG Usage Examples

Demonstrates all agentic RAG modes with practical examples.
Run these examples after indexing some documents.
"""

import asyncio

from src.modules.rag_engine import AgenticRAG, RAGMode
from src.modules.search_engine import SearchEngine


async def example_adaptive_mode():
    """Example 1: Adaptive mode (automatic selection)."""
    print("=" * 60)
    print("Example 1: Adaptive Mode (Automatic Selection)")
    print("=" * 60)

    search = SearchEngine(database_path="recall.db")
    rag = AgenticRAG(search_engine=search)

    # Check if Ollama is running
    if not await rag.health_check():
        print("❌ Ollama is not running. Start with: ollama serve")
        return

    # Simple factual question - will use Self-RAG
    question = "What is Python?"
    print(f"\nQuestion: {question}")

    result = await rag.ask(question)

    print(f"\n✅ Mode selected: {result.mode}")
    print(f"📊 Confidence: {result.confidence:.2f}")
    print(f"🔄 Iterations: {result.iterations}")
    print(f"\n💬 Answer:\n{result.answer}")

    if result.citations:
        print(f"\n📚 Citations ({len(result.citations)}):")
        for cite in result.citations[:3]:
            print(f"  [{cite.citation_id}] {cite.file_path}")
            print(f"      {cite.snippet[:100]}...")

    await rag.close()


async def example_self_rag():
    """Example 2: Self-RAG with verification."""
    print("\n" + "=" * 60)
    print("Example 2: Self-RAG (Self-Reflective with Verification)")
    print("=" * 60)

    search = SearchEngine(database_path="recall.db")
    rag = AgenticRAG(search_engine=search)

    if not await rag.health_check():
        print("❌ Ollama is not running")
        return

    question = "What is machine learning?"
    print(f"\nQuestion: {question}")

    result = await rag.ask(question, mode=RAGMode.SELF_RAG, max_iterations=3, top_k=5)

    print("\n✅ Answer generated and verified")
    print(f"🔍 Retrieval Assessment: {result.retrieval_assessment}")
    print(f"✔️  Answer Assessment: {result.answer_assessment}")
    print(f"📊 Confidence: {result.confidence:.2f}")
    print(f"🔄 Iterations: {result.iterations}")
    print(f"⏱️  Execution Time: {result.execution_time_ms:.1f}ms")

    print(f"\n💬 Verified Answer:\n{result.answer}")

    if result.citations:
        print("\n📚 Citations with inline references:")
        for cite in result.citations:
            print(f"  [{cite.citation_id}] {cite.file_path} (score: {cite.score:.2f})")
            print(f'      "{cite.snippet[:80]}..."')

    # Show if query was rewritten
    if result.metadata.get("final_query"):
        print("\n🔄 Query was rewritten:")
        print(f"   Original: {result.metadata['original_question']}")
        print(f"   Final: {result.metadata['final_query']}")

    await rag.close()


async def example_multi_step():
    """Example 3: Multi-step reasoning for complex questions."""
    print("\n" + "=" * 60)
    print("Example 3: Multi-Step Reasoning (Complex Questions)")
    print("=" * 60)

    search = SearchEngine(database_path="recall.db")
    rag = AgenticRAG(search_engine=search)

    if not await rag.health_check():
        print("❌ Ollama is not running")
        return

    # Complex question requiring multiple steps
    question = "Compare supervised and unsupervised learning"
    print(f"\nComplex Question: {question}")

    result = await rag.ask(question, mode=RAGMode.MULTI_STEP, max_sub_questions=4)

    print(f"\n✅ Question decomposed into {len(result.reasoning_steps)} steps")
    print(f"📊 Overall Confidence: {result.confidence:.2f}")
    print(f"⏱️  Total Execution Time: {result.execution_time_ms:.1f}ms")

    # Show reasoning chain
    print("\n🧩 Reasoning Steps:")
    for i, step in enumerate(result.reasoning_steps, 1):
        print(f"\n  Step {i}: {step.question}")
        print(f"  Confidence: {step.confidence:.2f}")
        print(f"  Sources: {len(step.sources)}")
        print(f"  Answer: {step.answer[:150]}...")

    print(f"\n💬 Final Synthesized Answer:\n{result.answer}")

    await rag.close()


async def example_crag():
    """Example 4: CRAG with fallback sources."""
    print("\n" + "=" * 60)
    print("Example 4: CRAG (Corrective RAG with Fallbacks)")
    print("=" * 60)

    search = SearchEngine(database_path="recall.db")
    rag = AgenticRAG(search_engine=search)

    if not await rag.health_check():
        print("❌ Ollama is not running")
        return

    # Question that might need web fallback
    question = "What is the latest version of Python?"
    print(f"\nQuestion: {question}")
    print("(This might need web fallback if local docs are outdated)")

    result = await rag.ask(
        question,
        mode=RAGMode.CRAG,
        enable_web_fallback=False,  # Web search not yet implemented
    )

    print("\n✅ CRAG completed")
    print(f"🌐 Fallback used: {result.fallback_used}")
    print(f"📍 Source: {result.metadata.get('source', 'local')}")
    print(f"📊 Confidence: {result.confidence:.2f}")

    print(f"\n💬 Answer:\n{result.answer}")

    if result.sources:
        print(f"\n📚 Sources ({len(result.sources)}):")
        for src in result.sources[:3]:
            print(
                f"  • {src.get('file_path', 'unknown')} (score: {src.get('score', 0):.2f})"
            )

    await rag.close()


async def example_decompose_only():
    """Example 5: Just decompose a complex question."""
    print("\n" + "=" * 60)
    print("Example 5: Question Decomposition (Preview)")
    print("=" * 60)

    search = SearchEngine(database_path="recall.db")
    rag = AgenticRAG(search_engine=search)

    if not await rag.health_check():
        print("❌ Ollama is not running")
        return

    # Preview how a question will be decomposed
    question = "How do neural networks learn and what are their limitations?"
    print(f"\nComplex Question: {question}")

    print("\nDecomposing into sub-questions...")
    sub_questions = await rag.multi_step._decompose_question(
        question, max_sub_questions=4
    )

    print(f"\n✅ Decomposed into {len(sub_questions)} sub-questions:")
    for i, sub_q in enumerate(sub_questions, 1):
        print(f"  {i}. {sub_q}")

    print("\n💡 Multi-Step would now answer each sub-question")
    print("   and synthesize them into a comprehensive answer.")

    await rag.close()


async def example_compare_modes():
    """Example 6: Compare different modes on same question."""
    print("\n" + "=" * 60)
    print("Example 6: Mode Comparison (Same Question)")
    print("=" * 60)

    search = SearchEngine(database_path="recall.db")
    rag = AgenticRAG(search_engine=search)

    if not await rag.health_check():
        print("❌ Ollama is not running")
        return

    question = "What is deep learning?"
    print(f"\nQuestion: {question}")

    modes = [RAGMode.STANDARD, RAGMode.SELF_RAG, RAGMode.MULTI_STEP]

    print("\n📊 Comparing modes...\n")
    results = []

    for mode in modes:
        print(f"Testing {mode.value}...")
        result = await rag.ask(
            question,
            mode=mode,
            max_iterations=1 if mode == RAGMode.SELF_RAG else None,
            max_sub_questions=2 if mode == RAGMode.MULTI_STEP else None,
        )
        results.append((mode, result))

    # Compare results
    print("\n" + "=" * 60)
    print("Results Comparison")
    print("=" * 60)

    for mode, result in results:
        print(f"\n{mode.value.upper()}:")
        print(f"  Confidence: {result.confidence:.2f}")
        print(f"  Execution Time: {result.execution_time_ms:.0f}ms")
        print(f"  Citations: {len(result.citations)}")
        print(f"  Answer Length: {len(result.answer)} chars")
        if result.answer_assessment:
            print(f"  Verified: {result.answer_assessment}")

    await rag.close()


async def main():
    """Run all examples."""
    print("\n" + "=" * 60)
    print("AGENTIC RAG EXAMPLES")
    print("=" * 60)
    print("\nMake sure you have:")
    print("1. Ollama running: ollama serve")
    print("2. Model downloaded: ollama pull llama3.1:8b")
    print("3. Documents indexed in recall.db")

    input("\nPress Enter to start examples...")

    # Run examples
    await example_adaptive_mode()
    await example_self_rag()
    await example_multi_step()
    await example_crag()
    await example_decompose_only()
    await example_compare_modes()

    print("\n" + "=" * 60)
    print("All examples completed!")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
