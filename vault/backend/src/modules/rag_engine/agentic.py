"""
Agentic RAG: Unified Interface for Advanced RAG Modes

Provides single interface for all agentic RAG patterns:
- Self-RAG: Self-reflective with verification
- CRAG: Corrective retrieval with fallbacks
- Multi-Step: Complex question decomposition
- Adaptive: Automatically selects best mode

Basic Usage:
    >>> from src.modules.rag_engine import AgenticRAG
    >>>
    >>> rag = AgenticRAG(search_engine, ollama_client)
    >>>
    >>> # Let it choose the best mode
    >>> result = await rag.ask("What is machine learning?")
    >>>
    >>> # Or specify a mode
    >>> result = await rag.ask(
    ...     "Compare supervised and unsupervised learning",
    ...     mode=RAGMode.MULTI_STEP
    ... )
"""

import logging

import httpx

from src.modules.rag_engine.crag import CRAG
from src.modules.rag_engine.deep_research import DeepResearchRAG
from src.modules.rag_engine.multi_step import MultiStepRAG
from src.modules.rag_engine.self_rag import SelfRAG
from src.modules.rag_engine.types import AgenticResult, RAGMode

logger = logging.getLogger(__name__)


class AgenticRAG:
    """
    Unified interface for agentic RAG modes.

    Provides single entry point for all advanced RAG patterns
    with automatic mode selection or manual control.

    Supported modes:
    - STANDARD: Traditional RAG (retrieve + generate)
    - SELF_RAG: Self-reflective with answer verification
    - CRAG: Corrective with fallback sources
    - MULTI_STEP: Multi-step reasoning for complex questions
    - ADAPTIVE: Automatically select best mode

    Performance characteristics:
    - STANDARD: ~1-2s, baseline quality
    - SELF_RAG: ~2-4s, verified answers with citations
    - CRAG: ~1-3s (local), ~3-5s (with web fallback)
    - MULTI_STEP: ~3-8s, best for complex questions
    - ADAPTIVE: Variable (analyzes question first)

    Example:
        >>> rag = AgenticRAG(search_engine)
        >>>
        >>> # Adaptive mode (default)
        >>> result = await rag.ask("What is Python?")
        >>> print(f"Mode used: {result.mode}")
        >>>
        >>> # Manual mode selection
        >>> result = await rag.ask(
        ...     "Explain neural networks and deep learning",
        ...     mode=RAGMode.MULTI_STEP
        ... )
    """

    def __init__(
        self,
        search_engine,
        web_search=None,
        ollama_url: str = "http://localhost:11434",
        model_name: str = "llama3.1:8b",
        enable_web_search: bool = False,
        max_parallel_llm_calls: int = 3,
        max_parallel_search_calls: int = 4,
    ):
        """
        Initialize Agentic RAG with all modes.

        Args:
            search_engine: SearchEngine for local knowledge base
            web_search: Optional web search engine for CRAG fallback (deprecated)
            ollama_url: URL of Ollama API server
            model_name: Ollama model to use
            enable_web_search: Enable DuckDuckGo web search for CRAG fallback
            max_parallel_llm_calls: Max concurrent Ollama calls for deep research
            max_parallel_search_calls: Max concurrent retrieval calls for deep research

        Example:
            >>> from src.modules.search_engine import SearchService
            >>>
            >>> search = SearchService(db=db)
            >>> rag = AgenticRAG(
            ...     search_engine=search,
            ...     ollama_url="http://localhost:11434",
            ...     model_name="llama3.1:8b",
            ...     enable_web_search=True  # Enable web fallback
            ... )
        """
        self.search = search_engine
        self.web_search = web_search
        self.ollama_url = ollama_url
        self.model_name = model_name
        self.enable_web_search = enable_web_search
        self.max_parallel_llm_calls = max(1, max_parallel_llm_calls)
        self.max_parallel_search_calls = max(1, max_parallel_search_calls)

        # Initialize HTTP client for Ollama
        self.ollama_client = httpx.AsyncClient(base_url=ollama_url, timeout=60.0)

        # Initialize RAG modes
        self.self_rag = SelfRAG(search_engine, self.ollama_client)
        self.crag = CRAG(
            search_engine, web_search, self.ollama_client, enable_web_search
        )
        self.multi_step = MultiStepRAG(search_engine, self.ollama_client)
        self.deep_research = DeepResearchRAG(
            search_engine=search_engine,
            ollama_client=self.ollama_client,
            model_name=model_name,
            enable_web_search=enable_web_search,
            max_parallel_llm_calls=self.max_parallel_llm_calls,
            max_parallel_search_calls=self.max_parallel_search_calls,
        )

    async def ask(
        self, question: str, mode: RAGMode = RAGMode.ADAPTIVE, **kwargs
    ) -> AgenticResult:
        """
        Ask question using specified or adaptive RAG mode.

        Args:
            question: Question to answer
            mode: RAG mode to use (default: ADAPTIVE)
            **kwargs: Mode-specific parameters:
                - max_iterations: For SELF_RAG (default: 3)
                - enable_web_fallback: For CRAG (default: False)
                - max_sub_questions: For MULTI_STEP (default: 4)
                - max_depth: For DEEP_RESEARCH (default: 2)
                - branch_factor: For DEEP_RESEARCH (default: 3)
                - target_confidence: For DEEP_RESEARCH (default: 0.82)
                - top_k: Documents to retrieve (default: 5)

        Returns:
            AgenticResult with answer and metadata

        Example:
            >>> # Simple question (will use Self-RAG or standard)
            >>> result = await rag.ask("What is Python?")
            >>>
            >>> # Complex question (will use Multi-Step)
            >>> result = await rag.ask(
            ...     "Compare Python and JavaScript for web development"
            ... )
            >>>
            >>> # Manual mode selection
            >>> result = await rag.ask(
            ...     "What is machine learning?",
            ...     mode=RAGMode.SELF_RAG,
            ...     max_iterations=2
            ... )
        """
        # Check Ollama availability
        if not await self.health_check():
            logger.error("Ollama is not available")
            return AgenticResult(
                answer="Ollama service is not available. Please start Ollama to use Q&A features.",
                mode=mode,
                confidence=0.0,
                metadata={"error": "ollama_unavailable"},
            )

        # If adaptive mode, select best mode based on question
        if mode == RAGMode.ADAPTIVE:
            mode = await self._select_mode(question)
            logger.info(f"Adaptive mode selected: {mode.value}")

        # Route to appropriate RAG implementation
        if mode == RAGMode.SELF_RAG:
            return await self.self_rag.ask_with_reflection(
                question,
                max_iterations=kwargs.get("max_iterations", 3),
                top_k=kwargs.get("top_k", 5),
            )

        elif mode == RAGMode.CRAG:
            return await self.crag.ask_with_correction(
                question,
                enable_web_fallback=kwargs.get("enable_web_fallback", False),
                top_k=kwargs.get("top_k", 5),
            )

        elif mode == RAGMode.MULTI_STEP:
            return await self.multi_step.ask_multi_step(
                question,
                max_sub_questions=kwargs.get("max_sub_questions", 4),
                top_k_per_step=kwargs.get("top_k", 3),
            )

        elif mode == RAGMode.DEEP_RESEARCH:
            return await self.deep_research.ask_deep_research(
                question=question,
                search_mode=kwargs.get("search_mode", "hybrid"),
                max_depth=kwargs.get("max_depth", 2),
                max_iterations=kwargs.get("max_iterations", 8),
                branch_factor=kwargs.get("branch_factor", 3),
                top_k=kwargs.get("top_k", 5),
                target_confidence=kwargs.get("target_confidence", 0.82),
                min_marginal_gain=kwargs.get("min_marginal_gain", 0.05),
                time_budget_seconds=kwargs.get("time_budget_seconds", 120),
            )

        elif mode == RAGMode.STANDARD:
            # Use basic Self-RAG with 1 iteration (no retries)
            return await self.self_rag.ask_with_reflection(
                question, max_iterations=1, top_k=kwargs.get("top_k", 5)
            )

        else:
            logger.error(f"Unknown RAG mode: {mode}")
            return AgenticResult(
                answer=f"Unknown RAG mode: {mode}",
                mode=mode,
                confidence=0.0,
                metadata={"error": "unknown_mode"},
            )

    async def _select_mode(self, question: str) -> RAGMode:
        """
        Automatically select best RAG mode based on question characteristics.

        Heuristics:
        - Complex/multi-part questions -> MULTI_STEP
        - Questions needing verification -> SELF_RAG
        - Simple factual questions -> STANDARD

        Args:
            question: Question to analyze

        Returns:
            Selected RAG mode
        """
        question_lower = question.lower()

        deep_research_indicators = [
            "deep research",
            "comprehensive",
            "state of the art",
            "latest",
            "recent",
            "survey",
            "systematic",
            "roadmap",
            "multi-step",
            "recursive",
            "web",
            "papers",
        ]
        if any(indicator in question_lower for indicator in deep_research_indicators):
            logger.info("Question appears research-heavy, selecting DEEP_RESEARCH")
            return RAGMode.DEEP_RESEARCH

        # Check for complexity indicators
        complexity_indicators = [
            "compare",
            "contrast",
            "difference",
            "versus",
            "vs",
            "pros and cons",
            "advantages and disadvantages",
            "both",
            "and",
            "as well as",
            "multiple",
            "explain how",
            "why and how",
            "what are the",
        ]

        # Check for multi-part questions
        has_complexity = any(ind in question_lower for ind in complexity_indicators)
        has_multiple_questions = question.count("?") > 1 or " and " in question_lower

        if has_complexity or has_multiple_questions:
            # Complex question -> use multi-step
            logger.info("Question appears complex, selecting MULTI_STEP")
            return RAGMode.MULTI_STEP

        # Check for verification needs (factual claims)
        verification_indicators = [
            "is",
            "are",
            "does",
            "did",
            "was",
            "were",
            "what is",
            "what are",
            "who is",
            "when did",
        ]
        needs_verification = any(
            ind in question_lower for ind in verification_indicators
        )

        if needs_verification:
            # Factual question needing verification -> use self-RAG
            logger.info("Question needs verification, selecting SELF_RAG")
            return RAGMode.SELF_RAG

        # Default to Self-RAG (good balance)
        logger.info("Using SELF_RAG as default")
        return RAGMode.SELF_RAG

    async def health_check(self) -> bool:
        """
        Check if Ollama service is available.

        Returns:
            True if Ollama is running and accessible

        Example:
            >>> rag = AgenticRAG(search_engine)
            >>> if await rag.health_check():
            ...     print("Ollama is ready")
            ... else:
            ...     print("Start Ollama: ollama serve")
        """
        try:
            response = await self.ollama_client.get("/api/tags", timeout=5.0)
            return response.status_code == 200
        except Exception as e:
            logger.error(f"Ollama health check failed: {e}")
            return False

    async def close(self):
        """Close HTTP client connections."""
        await self.ollama_client.aclose()

    async def __aenter__(self):
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()


__all__ = ["AgenticRAG"]
