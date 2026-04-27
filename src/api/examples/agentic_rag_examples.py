"""
Examples demonstrating Agentic RAG usage.

This file shows various use cases for the Agentic RAG system,
from simple API calls to advanced streaming and comparison scenarios.
"""

import asyncio
import httpx
import json
from typing import AsyncIterator


BASE_URL = "http://localhost:8000/api/v1"


async def example_1_basic_query():
    """
    Example 1: Basic agentic query

    Shows the simplest usage of agentic RAG.
    """
    print("\n" + "="*80)
    print("EXAMPLE 1: Basic Agentic Query")
    print("="*80 + "\n")

    async with httpx.AsyncClient(timeout=30.0) as client:
        response = await client.post(
            f"{BASE_URL}/agentic-rag/ask",
            json={
                "query": "What are the key risks mentioned in project documentation?",
                "search_mode": "hybrid",
                "enable_reflection": True,
                "enable_correction": True,
            }
        )

        result = response.json()

        print(f"Question: {result['metadata']['question']}")
        print(f"\nAnswer:\n{result['answer']}\n")
        print(f"Sources Used: {len(result['sources'])}")
        print(f"Total Iterations: {result['total_iterations']}")
        print(f"Processing Time: {result['total_time_ms']}ms")
        print(f"Self-Corrected: {result['corrected']}")

        if result['reasoning_steps']:
            print("\nReasoning Trace:")
            for i, step in enumerate(result['reasoning_steps'][:3], 1):
                print(f"\n  Step {i} ({step['phase']}):")
                print(f"    Thought: {step['thought'][:100]}...")
                if step.get('action'):
                    print(f"    Action: {step['action']}")


async def example_2_streaming_query():
    """
    Example 2: Streaming agentic query

    Shows how to stream the reasoning process in real-time.
    """
    print("\n" + "="*80)
    print("EXAMPLE 2: Streaming Agentic Query")
    print("="*80 + "\n")

    async with httpx.AsyncClient(timeout=60.0) as client:
        async with client.stream(
            "POST",
            f"{BASE_URL}/agentic-rag/ask",
            json={
                "query": "Compare Q3 and Q4 2024 financial performance",
                "streaming": True,
            }
        ) as response:
            async for line in response.aiter_lines():
                if line.startswith("data: "):
                    event_data = line[6:]
                    try:
                        event = json.loads(event_data)

                        if event["type"] == "status":
                            print(f"\n🔄 {event['content']}")

                        elif event["type"] == "thought":
                            print(f"\n💭 Thinking: {event['content'][:100]}...")

                        elif event["type"] == "action":
                            print(f"\n🔧 Action {event['iteration']}: {event['tool']}")
                            print(f"   Input: {event['input']}")

                        elif event["type"] == "observation":
                            if event["success"]:
                                print(f"   ✅ Found {len(event.get('sources', []))} sources")
                            else:
                                print(f"   ❌ Error: {event.get('error')}")

                        elif event["type"] == "answer":
                            print(f"\n📝 Answer:\n{event['content']}\n")

                        elif event["type"] == "reflection":
                            print(f"\n🔍 Self-Reflection:\n{event['content'][:200]}...")

                        elif event["type"] == "correction":
                            print(f"\n✨ Improved Answer:\n{event['content']}\n")

                        elif event["type"] == "complete":
                            print(f"\n✅ Complete in {event['total_time_ms']}ms")

                    except json.JSONDecodeError:
                        pass


async def example_3_compare_approaches():
    """
    Example 3: Compare simple vs agentic RAG

    Shows the difference in quality and performance.
    """
    print("\n" + "="*80)
    print("EXAMPLE 3: Compare Simple vs Agentic RAG")
    print("="*80 + "\n")

    test_questions = [
        "What is the Q4 budget?",  # Simple question
        "Compare the technical approaches in the 2023 and 2024 architecture documents",  # Complex
    ]

    async with httpx.AsyncClient(timeout=60.0) as client:
        for question in test_questions:
            print(f"\nQuestion: {question}")
            print("-" * 80)

            response = await client.post(
                f"{BASE_URL}/agentic-rag/compare",
                json={
                    "query": question,
                    "search_mode": "hybrid",
                }
            )

            result = response.json()

            print(f"\n📊 SIMPLE RAG:")
            print(f"  Time: {result['simple_rag']['time_ms']}ms")
            print(f"  Sources: {result['simple_rag']['sources_count']}")
            print(f"  Answer: {result['simple_rag']['answer'][:150]}...")

            print(f"\n🤖 AGENTIC RAG:")
            print(f"  Time: {result['agentic_rag']['time_ms']}ms")
            print(f"  Sources: {result['agentic_rag']['sources_count']}")
            print(f"  Iterations: {result['agentic_rag']['metadata']['iterations']}")
            print(f"  Corrected: {result['agentic_rag']['metadata']['corrected']}")
            print(f"  Answer: {result['agentic_rag']['answer'][:150]}...")

            print(f"\n💡 COMPARISON:")
            print(f"  Time Multiplier: {result['comparison']['time_multiplier']}x")
            print(f"  Additional Sources: {result['comparison']['additional_sources']}")
            print(f"\n  Recommendation: {result['comparison']['recommendation']}")


async def example_4_complex_multi_doc_analysis():
    """
    Example 4: Complex question requiring multiple documents

    Shows agentic RAG handling a sophisticated analysis task.
    """
    print("\n" + "="*80)
    print("EXAMPLE 4: Complex Multi-Document Analysis")
    print("="*80 + "\n")

    async with httpx.AsyncClient(timeout=60.0) as client:
        response = await client.post(
            f"{BASE_URL}/agentic-rag/ask",
            json={
                "query": (
                    "What are the common themes in customer feedback "
                    "across all sales calls in the last quarter?"
                ),
                "search_mode": "hybrid",
                "max_iterations": 7,  # Allow more iterations for complex task
                "enable_reflection": True,
                "enable_correction": True,
            }
        )

        result = response.json()

        print(f"Question: {result['metadata']['question']}")
        print(f"\n{'='*80}")
        print("REASONING TRACE")
        print('='*80)

        for step in result['reasoning_steps']:
            print(f"\n[Iteration {step['iteration']}] Phase: {step['phase'].upper()}")
            print(f"Thought: {step['thought']}")

            if step.get('action'):
                print(f"Action: {step['action']}")
                print(f"Input: {step['action_input']}")

            if step.get('observation'):
                print(f"Result: {step['observation']}")

        print(f"\n{'='*80}")
        print("FINAL ANSWER")
        print('='*80)
        print(f"\n{result['answer']}\n")

        print(f"\n{'='*80}")
        print("METADATA")
        print('='*80)
        print(f"Sources Used: {len(result['sources'])}")
        for i, source in enumerate(result['sources'][:5], 1):
            print(f"\n  [{i}] {source['filename']} (score: {source['score']:.2f})")
            print(f"      {source['snippet'][:100]}...")

        print(f"\nTotal Processing Time: {result['total_time_ms']}ms")
        print(f"Self-Corrected: {result['corrected']}")

        if result.get('reflection'):
            print(f"\n{'='*80}")
            print("SELF-REFLECTION")
            print('='*80)
            print(result['reflection'])


async def example_5_custom_configuration():
    """
    Example 5: Custom configuration for different use cases

    Shows how to tune parameters for various scenarios.
    """
    print("\n" + "="*80)
    print("EXAMPLE 5: Custom Configurations")
    print("="*80 + "\n")

    configurations = [
        {
            "name": "Fast Mode (Minimal Reasoning)",
            "config": {
                "query": "What is the project deadline?",
                "max_iterations": 2,
                "enable_reflection": False,
                "enable_correction": False,
                "temperature": 0.3,
            }
        },
        {
            "name": "Balanced Mode (Standard)",
            "config": {
                "query": "What is the project deadline?",
                "max_iterations": 5,
                "enable_reflection": True,
                "enable_correction": True,
                "temperature": 0.7,
            }
        },
        {
            "name": "Deep Analysis Mode",
            "config": {
                "query": "What is the project deadline?",
                "max_iterations": 10,
                "enable_reflection": True,
                "enable_correction": True,
                "temperature": 0.7,
            }
        },
    ]

    async with httpx.AsyncClient(timeout=60.0) as client:
        for config_set in configurations:
            print(f"\n{config_set['name']}")
            print("-" * 80)

            response = await client.post(
                f"{BASE_URL}/agentic-rag/ask",
                json=config_set['config']
            )

            result = response.json()

            print(f"Iterations: {result['total_iterations']}")
            print(f"Time: {result['total_time_ms']}ms")
            print(f"Corrected: {result['corrected']}")
            print(f"Answer: {result['answer'][:100]}...")


async def example_6_error_handling():
    """
    Example 6: Proper error handling

    Shows how to handle various error scenarios.
    """
    print("\n" + "="*80)
    print("EXAMPLE 6: Error Handling")
    print("="*80 + "\n")

    async with httpx.AsyncClient(timeout=60.0) as client:
        # Test 1: Empty query
        print("Test 1: Empty Query")
        try:
            response = await client.post(
                f"{BASE_URL}/agentic-rag/ask",
                json={"query": ""}
            )
            print(f"Status: {response.status_code}")
            print(f"Error: {response.json()}")
        except Exception as e:
            print(f"Error: {e}")

        # Test 2: Invalid parameters
        print("\nTest 2: Invalid Parameters")
        try:
            response = await client.post(
                f"{BASE_URL}/agentic-rag/ask",
                json={
                    "query": "test",
                    "max_iterations": 100,  # Too high
                }
            )
            print(f"Status: {response.status_code}")
            if response.status_code != 200:
                print(f"Error: {response.json()}")
        except Exception as e:
            print(f"Error: {e}")

        # Test 3: Health check before query
        print("\nTest 3: Health Check")
        try:
            health_response = await client.get(
                f"{BASE_URL}/agentic-rag/health"
            )
            health = health_response.json()
            print(f"Status: {health['status']}")
            print(f"Ollama Available: {health['ollama_available']}")

            if health['status'] == 'healthy':
                print("✅ Service is healthy, safe to query")
            else:
                print("❌ Service is unhealthy, queries may fail")
        except Exception as e:
            print(f"Error: {e}")


async def example_7_reasoning_trace_analysis():
    """
    Example 7: Analyze reasoning traces

    Shows how to analyze and understand the agent's decision-making process.
    """
    print("\n" + "="*80)
    print("EXAMPLE 7: Reasoning Trace Analysis")
    print("="*80 + "\n")

    async with httpx.AsyncClient(timeout=60.0) as client:
        response = await client.post(
            f"{BASE_URL}/agentic-rag/ask",
            json={
                "query": "What security measures are recommended in the security audit?",
                "search_mode": "hybrid",
            }
        )

        result = response.json()

        # Analyze reasoning patterns
        phases = {}
        for step in result['reasoning_steps']:
            phase = step['phase']
            if phase not in phases:
                phases[phase] = []
            phases[phase].append(step)

        print("REASONING ANALYSIS")
        print("=" * 80)

        for phase, steps in phases.items():
            print(f"\n{phase.upper()} Phase ({len(steps)} steps):")

            if phase == 'action':
                actions = [s.get('action') for s in steps if s.get('action')]
                print(f"  Actions taken: {', '.join(set(actions))}")
                print(f"  Average iterations: {len(steps)}")

            elif phase == 'planning':
                thoughts = [s['thought'] for s in steps]
                print(f"  Planning strategy: {thoughts[0][:100]}...")

            elif phase == 'reflection':
                if steps:
                    print(f"  Self-critique performed: Yes")
                    print(f"  Quality concerns raised: {result['corrected']}")

        print(f"\n{'='*80}")
        print("EFFICIENCY METRICS")
        print('='*80)
        print(f"Total Time: {result['total_time_ms']}ms")
        print(f"Time per Iteration: {result['total_time_ms'] / max(result['total_iterations'], 1):.0f}ms")
        print(f"Sources per Iteration: {len(result['sources']) / max(result['total_iterations'], 1):.1f}")


async def main():
    """Run all examples."""
    print("\n" + "="*80)
    print("AGENTIC RAG EXAMPLES")
    print("="*80)
    print("\nThese examples demonstrate various use cases of Agentic RAG.")
    print("Make sure the Vault API server is running at http://localhost:8000")
    print("\nNote: These examples require documents to be indexed in your vault.")
    print("="*80)

    examples = [
        ("Basic Query", example_1_basic_query),
        ("Streaming Query", example_2_streaming_query),
        ("Compare Approaches", example_3_compare_approaches),
        ("Complex Analysis", example_4_complex_multi_doc_analysis),
        ("Custom Configuration", example_5_custom_configuration),
        ("Error Handling", example_6_error_handling),
        ("Reasoning Analysis", example_7_reasoning_trace_analysis),
    ]

    for i, (name, example_func) in enumerate(examples, 1):
        try:
            print(f"\n\n{'#'*80}")
            print(f"Running Example {i}: {name}")
            print('#'*80)
            await example_func()
        except httpx.ConnectError:
            print(f"\n❌ Cannot connect to API server at {BASE_URL}")
            print("Please ensure the Vault API is running.")
            break
        except Exception as e:
            print(f"\n❌ Example failed: {e}")
            import traceback
            traceback.print_exc()

    print("\n\n" + "="*80)
    print("Examples completed!")
    print("="*80)


if __name__ == "__main__":
    asyncio.run(main())
