"""
Agentic RAG with ReAct (Reasoning + Acting) pattern, reflection, and self-correction.

This module implements an advanced RAG system where the LLM acts as an autonomous agent
that can reason about what information it needs, take actions to retrieve that information,
reflect on the quality of its answers, and self-correct when necessary.

Key Features:
- ReAct Pattern: Thought -> Action -> Observation loop
- Self-Reflection: Agent critiques its own answers
- Self-Correction: Agent refines answers based on reflection
- Multi-Round Search: Agent can perform multiple search iterations
- Tool Use: Agent has access to multiple search and analysis tools
- Guardrails: Max iterations, timeouts, and fallback mechanisms
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
import logging
import time
from typing import TYPE_CHECKING, Any

from src.modules.search_engine import SearchFilters
from src.modules.search_engine import SearchResult as DbSearchResult
# TODO(Phase 3+): OllamaService should be injected via constructor (dependency injection)
# to break circular import between services.llm and modules.rag_engine.
# Current workaround: Direct import works but makes testing harder.
from src.services.llm.ollama_service import OllamaService
from src.services.llm.prompts import create_context_from_results

if TYPE_CHECKING:
    from src.modules.search_engine import SearchService

# Type alias for clarity
SearchResult = DbSearchResult

logger = logging.getLogger(__name__)


class AgentPhase(str, Enum):
    """Phases of the agentic RAG process."""

    PLANNING = "planning"
    ACTION = "action"
    REASONING = "reasoning"
    REFLECTION = "reflection"
    CORRECTION = "correction"
    COMPLETE = "complete"


class ToolName(str, Enum):
    """Available tools for the agent."""

    SEARCH_DOCUMENTS = "search_documents"
    SEARCH_SPECIFIC_FILE = "search_specific_file"
    GET_DOCUMENT_METADATA = "get_document_metadata"
    REFORMULATE_QUERY = "reformulate_query"
    FINISH = "finish"


@dataclass
class AgentThought:
    """Represents a thought in the agent's reasoning chain."""

    phase: AgentPhase
    content: str
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))


@dataclass
class AgentAction:
    """Represents an action taken by the agent."""

    tool: ToolName
    tool_input: dict[str, Any]
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))


@dataclass
class AgentObservation:
    """Represents the observation/result of an agent action."""

    action: AgentAction
    result: Any
    success: bool
    error: str | None = None
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))


@dataclass
class ReasoningStep:
    """A complete step in the ReAct loop: Thought -> Action -> Observation."""

    thought: AgentThought
    action: AgentAction | None = None
    observation: AgentObservation | None = None
    iteration: int = 0


@dataclass
class AgenticResponse:
    """Complete response from the agentic RAG system."""

    answer: str
    reasoning_steps: list[ReasoningStep]
    sources: list[SearchResult]
    reflection: str | None = None
    corrected: bool = False
    total_iterations: int = 0
    total_time_ms: int = 0
    metadata: dict[str, Any] = field(default_factory=dict)


class AgenticRAGPrompts:
    """Prompt templates for agentic RAG."""

    @staticmethod
    def create_planning_prompt(question: str) -> str:
        """Create prompt for the planning phase."""
        return f"""You are an intelligent research assistant. A user has asked you a question about their personal documents.

Your task is to PLAN what information you need to answer this question thoroughly.

Question: {question}

Think step-by-step:
1. What are the key concepts in this question?
2. What information would be needed to answer it completely?
3. Are there multiple documents I should search for?
4. Do I need to compare information across documents?
5. What specific search queries would help me find this information?

Provide your planning thoughts. Be specific about what you need to search for."""

    @staticmethod
    def create_action_decision_prompt(
        question: str, planning_thoughts: str, previous_observations: list[str], iteration: int
    ) -> str:
        """Create prompt for deciding next action."""
        observations_text = (
            "\n\n".join(
                [f"Previous Search {i + 1}:\n{obs}" for i, obs in enumerate(previous_observations)]
            )
            if previous_observations
            else "No previous searches yet."
        )

        return f"""You are an intelligent research assistant with access to tools.

Question: {question}

Your Plan:
{planning_thoughts}

Previous Observations:
{observations_text}

Current Iteration: {iteration}

Available Tools:
1. search_documents(query: str) - Search all documents for relevant information
2. search_specific_file(file_id: str, query: str) - Search within a specific file
3. get_document_metadata(file_id: str) - Get metadata about a document
4. reformulate_query(original: str) - Improve a search query
5. finish(answer: str) - Provide final answer when you have enough information

Decide your next action:
- If you need more information, choose a search tool and specify what to search for
- If you have enough information to answer, use the finish tool
- If previous searches were poor, use reformulate_query

Respond in this EXACT format:
THOUGHT: [Your reasoning about what to do next]
ACTION: [tool_name]
INPUT: [specific input for the tool as JSON]

Example:
THOUGHT: I need to find Q3 financial data to compare with Q4
ACTION: search_documents
INPUT: {{"query": "Q3 2024 financial report revenue"}}"""

    @staticmethod
    def create_reasoning_prompt(question: str, context: str, previous_attempts: list[str]) -> str:
        """Create prompt for generating an answer."""
        attempts_text = ""
        if previous_attempts:
            attempts_text = "\n\nPrevious Attempts:\n" + "\n".join(
                [f"Attempt {i + 1}:\n{attempt}" for i, attempt in enumerate(previous_attempts)]
            )

        return f"""You are an intelligent research assistant. Based on the gathered information, provide a comprehensive answer.

Question: {question}

Context from Documents:
{context}
{attempts_text}

Provide a thorough, accurate answer based ONLY on the information in the context.
- Use specific details and examples from the documents
- Cite sources using [Source N] notation
- Be comprehensive but concise
- If information is incomplete, acknowledge it clearly

Answer:"""

    @staticmethod
    def create_reflection_prompt(question: str, answer: str, sources_count: int) -> str:
        """Create prompt for self-reflection."""
        return f"""You are a critical evaluator reviewing an answer to ensure quality.

Question: {question}

Proposed Answer:
{answer}

Number of Sources Used: {sources_count}

Critically evaluate this answer:
1. Does it fully address the question?
2. Is it accurate based on the provided sources?
3. Are there any logical gaps or inconsistencies?
4. Does it cite sources appropriately?
5. Is any important information missing?
6. Could it be more comprehensive or clearer?

Provide a CRITICAL evaluation. Be honest about flaws.

Format your response as:
EVALUATION: [Your critical assessment]
QUALITY_SCORE: [0-10]
NEEDS_CORRECTION: [yes/no]
ISSUES: [List specific issues if any]"""

    @staticmethod
    def create_correction_prompt(
        question: str, original_answer: str, reflection: str, context: str
    ) -> str:
        """Create prompt for self-correction."""
        return f"""You are improving an answer based on critical feedback.

Question: {question}

Original Answer:
{original_answer}

Critical Evaluation:
{reflection}

Available Context:
{context}

Based on the evaluation, provide an IMPROVED answer that addresses the identified issues.
Make sure to:
- Fix any inaccuracies or gaps
- Add missing information
- Improve clarity and structure
- Better cite sources

Improved Answer:"""


class AgenticRAGService:
    """
    Agentic RAG service with reasoning, reflection, and self-correction.

    This service implements an advanced RAG pattern where the LLM acts as an
    autonomous agent that can:
    - Plan what information it needs
    - Execute multiple search actions
    - Reason about gathered information
    - Reflect on its own answers
    - Correct and improve its responses
    """

    MAX_ITERATIONS = 5
    TIMEOUT_SECONDS = 60
    MIN_QUALITY_SCORE = 7

    def __init__(
        self,
        search_service: SearchService,
        ollama_service: OllamaService,
        model: str = "llama2",
        max_iterations: int = 5,
        timeout: int = 60,
        enable_reflection: bool = True,
        enable_correction: bool = True,
    ):
        """
        Initialize the agentic RAG service.

        Args:
            search_service: SearchService for document retrieval
            ollama_service: OllamaService for LLM generation
            model: Model to use for reasoning
            max_iterations: Maximum reasoning iterations
            timeout: Maximum time in seconds
            enable_reflection: Whether to enable self-reflection
            enable_correction: Whether to enable self-correction
        """
        self.search_service = search_service
        self.ollama_service = ollama_service
        self.model = model
        self.max_iterations = max_iterations
        self.timeout = timeout
        self.enable_reflection = enable_reflection
        self.enable_correction = enable_correction
        self.prompts = AgenticRAGPrompts()

    async def ask_question(
        self,
        question: str,
        user_id: str | None = None,
        search_mode: str = "hybrid",
        filters: SearchFilters | None = None,
        temperature: float = 0.7,
        streaming: bool = False,
    ) -> AgenticResponse:
        """
        Answer a question using agentic RAG with reasoning and reflection.

        Process:
        1. Planning: Agent decides what information it needs
        2. Action Loop: Agent searches and gathers information (multiple rounds)
        3. Reasoning: Agent synthesizes information into an answer
        4. Reflection: Agent critiques its own answer
        5. Correction: Agent improves answer if needed

        Args:
            question: User's question
            user_id: Optional user ID for filtering
            search_mode: Search mode ("vector", "text", or "hybrid")
            filters: Optional search filters
            temperature: Sampling temperature
            streaming: Whether to stream the response

        Returns:
            AgenticResponse with answer, reasoning trace, and sources
        """
        start_time = time.time()
        reasoning_steps: list[ReasoningStep] = []
        all_sources: list[SearchResult] = []

        logger.info(f"Starting agentic RAG for question: '{question[:100]}...'")

        try:
            phase_1_planning = await self._planning_phase(question)
            reasoning_steps.append(
                ReasoningStep(
                    thought=AgentThought(AgentPhase.PLANNING, phase_1_planning), iteration=0
                )
            )

            await self._action_phase(
                question=question,
                planning_thoughts=phase_1_planning,
                search_mode=search_mode,
                filters=filters,
                reasoning_steps=reasoning_steps,
                all_sources=all_sources,
                start_time=start_time,
            )

            context = create_context_from_results(all_sources)

            phase_3_answer = await self._reasoning_phase(
                question=question, context=context, temperature=temperature
            )
            reasoning_steps.append(
                ReasoningStep(
                    thought=AgentThought(
                        AgentPhase.REASONING, f"Generated initial answer: {phase_3_answer[:100]}..."
                    ),
                    iteration=len(reasoning_steps),
                )
            )

            final_answer = phase_3_answer
            reflection_text = None
            corrected = False

            if self.enable_reflection:
                phase_4_reflection = await self._reflection_phase(
                    question=question, answer=phase_3_answer, sources_count=len(all_sources)
                )
                reflection_text = phase_4_reflection
                reasoning_steps.append(
                    ReasoningStep(
                        thought=AgentThought(AgentPhase.REFLECTION, phase_4_reflection),
                        iteration=len(reasoning_steps),
                    )
                )

                needs_correction = self._parse_reflection_needs_correction(phase_4_reflection)

                if self.enable_correction and needs_correction:
                    phase_5_corrected = await self._correction_phase(
                        question=question,
                        original_answer=phase_3_answer,
                        reflection=phase_4_reflection,
                        context=context,
                        temperature=temperature,
                    )
                    final_answer = phase_5_corrected
                    corrected = True
                    reasoning_steps.append(
                        ReasoningStep(
                            thought=AgentThought(
                                AgentPhase.CORRECTION,
                                f"Corrected answer: {phase_5_corrected[:100]}...",
                            ),
                            iteration=len(reasoning_steps),
                        )
                    )

            total_time_ms = int((time.time() - start_time) * 1000)

            logger.info(
                f"Agentic RAG completed in {total_time_ms}ms with "
                f"{len(reasoning_steps)} steps and {len(all_sources)} sources"
            )

            return AgenticResponse(
                answer=final_answer,
                reasoning_steps=reasoning_steps,
                sources=all_sources,
                reflection=reflection_text,
                corrected=corrected,
                total_iterations=len(reasoning_steps),
                total_time_ms=total_time_ms,
                metadata={
                    "question": question,
                    "model": self.model,
                    "search_mode": search_mode,
                    "enable_reflection": self.enable_reflection,
                    "enable_correction": self.enable_correction,
                },
            )

        except TimeoutError:
            logger.error(f"Agentic RAG timeout after {self.timeout}s")
            raise
        except Exception as e:
            logger.exception(f"Agentic RAG failed: {e}")
            raise

    async def ask_question_stream(
        self,
        question: str,
        user_id: str | None = None,
        search_mode: str = "hybrid",
        filters: SearchFilters | None = None,
        temperature: float = 0.7,
    ) -> AsyncIterator[dict[str, Any]]:
        """
        Stream agentic RAG response with reasoning steps.

        Yields dictionaries with:
        - type: "thought", "action", "observation", "answer", "reflection", "correction"
        - content: The actual content
        - metadata: Additional information
        """
        start_time = time.time()

        yield {
            "type": "status",
            "phase": AgentPhase.PLANNING,
            "content": "Planning information gathering strategy...",
        }

        planning = await self._planning_phase(question)
        yield {"type": "thought", "phase": AgentPhase.PLANNING, "content": planning}

        yield {
            "type": "status",
            "phase": AgentPhase.ACTION,
            "content": "Gathering information from documents...",
        }

        reasoning_steps: list[ReasoningStep] = []
        all_sources: list[SearchResult] = []

        async for action_event in self._action_phase_stream(
            question=question,
            planning_thoughts=planning,
            search_mode=search_mode,
            filters=filters,
            start_time=start_time,
        ):
            if action_event["type"] == "action":
                reasoning_steps.append(
                    ReasoningStep(
                        thought=AgentThought(AgentPhase.ACTION, action_event["thought"]),
                        action=action_event["action"],
                        iteration=len(reasoning_steps),
                    )
                )
            elif action_event["type"] == "observation":
                if action_event.get("sources"):
                    all_sources.extend(action_event["sources"])

            yield action_event

        yield {
            "type": "status",
            "phase": AgentPhase.REASONING,
            "content": "Synthesizing answer from gathered information...",
        }

        context = create_context_from_results(all_sources)
        answer = await self._reasoning_phase(question, context, temperature)

        yield {
            "type": "answer",
            "phase": AgentPhase.REASONING,
            "content": answer,
            "sources": [
                {
                    "file_path": s.file_path,
                    "filename": s.filename,
                    "score": s.score,
                    "snippet": s.snippet,
                }
                for s in all_sources
            ],
        }

        if self.enable_reflection:
            yield {
                "type": "status",
                "phase": AgentPhase.REFLECTION,
                "content": "Evaluating answer quality...",
            }

            reflection = await self._reflection_phase(question, answer, len(all_sources))
            yield {"type": "reflection", "phase": AgentPhase.REFLECTION, "content": reflection}

            if self.enable_correction and self._parse_reflection_needs_correction(reflection):
                yield {
                    "type": "status",
                    "phase": AgentPhase.CORRECTION,
                    "content": "Improving answer based on reflection...",
                }

                corrected_answer = await self._correction_phase(
                    question, answer, reflection, context, temperature
                )

                yield {
                    "type": "correction",
                    "phase": AgentPhase.CORRECTION,
                    "content": corrected_answer,
                }

        total_time_ms = int((time.time() - start_time) * 1000)
        yield {
            "type": "complete",
            "phase": AgentPhase.COMPLETE,
            "total_time_ms": total_time_ms,
            "total_steps": len(reasoning_steps),
        }

    async def _planning_phase(self, question: str) -> str:
        """Phase 1: Agent plans what information it needs."""
        prompt = self.prompts.create_planning_prompt(question)
        planning = await self.ollama_service.generate_text(
            prompt=prompt, model=self.model, temperature=0.3, max_tokens=500
        )
        logger.info(f"Planning completed: {planning[:100]}...")
        return planning

    async def _action_phase(
        self,
        question: str,
        planning_thoughts: str,
        search_mode: str,
        filters: SearchFilters | None,
        reasoning_steps: list[ReasoningStep],
        all_sources: list[SearchResult],
        start_time: float,
    ) -> list[ReasoningStep]:
        """Phase 2: Agent takes actions to gather information."""
        observations: list[str] = []

        for iteration in range(self.max_iterations):
            if time.time() - start_time > self.timeout:
                logger.warning(f"Action phase timeout at iteration {iteration}")
                break

            action_prompt = self.prompts.create_action_decision_prompt(
                question=question,
                planning_thoughts=planning_thoughts,
                previous_observations=observations,
                iteration=iteration,
            )

            action_response = await self.ollama_service.generate_text(
                prompt=action_prompt, model=self.model, temperature=0.5, max_tokens=300
            )

            parsed_action = self._parse_action_response(action_response)

            if parsed_action["tool"] == ToolName.FINISH:
                logger.info(f"Agent decided to finish after {iteration + 1} iterations")
                break

            action = AgentAction(tool=parsed_action["tool"], tool_input=parsed_action["input"])

            observation = await self._execute_tool(
                action=action, search_mode=search_mode, filters=filters
            )

            reasoning_steps.append(
                ReasoningStep(
                    thought=AgentThought(AgentPhase.ACTION, parsed_action["thought"]),
                    action=action,
                    observation=observation,
                    iteration=iteration + 1,
                )
            )

            if observation.success and isinstance(observation.result, list):
                all_sources.extend(observation.result)
                observations.append(
                    f"Found {len(observation.result)} documents: "
                    + ", ".join([s.filename for s in observation.result[:3]])
                )
            else:
                observations.append(f"Action failed: {observation.error}")

        return reasoning_steps

    async def _action_phase_stream(
        self,
        question: str,
        planning_thoughts: str,
        search_mode: str,
        filters: SearchFilters | None,
        start_time: float,
    ) -> AsyncIterator[dict[str, Any]]:
        """Streaming version of action phase."""
        observations: list[str] = []

        for iteration in range(self.max_iterations):
            if time.time() - start_time > self.timeout:
                yield {"type": "error", "content": "Action phase timeout", "iteration": iteration}
                break

            action_prompt = self.prompts.create_action_decision_prompt(
                question, planning_thoughts, observations, iteration
            )

            action_response = await self.ollama_service.generate_text(
                prompt=action_prompt, model=self.model, temperature=0.5, max_tokens=300
            )

            parsed = self._parse_action_response(action_response)

            yield {
                "type": "action",
                "thought": parsed["thought"],
                "tool": parsed["tool"].value,
                "input": parsed["input"],
                "iteration": iteration + 1,
            }

            if parsed["tool"] == ToolName.FINISH:
                break

            action = AgentAction(tool=parsed["tool"], tool_input=parsed["input"])
            observation = await self._execute_tool(action, search_mode, filters)

            yield {
                "type": "observation",
                "success": observation.success,
                "sources": observation.result if observation.success else [],
                "error": observation.error,
                "iteration": iteration + 1,
            }

            if observation.success and isinstance(observation.result, list):
                observations.append(f"Found {len(observation.result)} documents")

    async def _reasoning_phase(
        self,
        question: str,
        context: str,
        temperature: float,
        previous_attempts: list[str] | None = None,
    ) -> str:
        """Phase 3: Agent reasons and generates an answer."""
        prompt = self.prompts.create_reasoning_prompt(
            question=question, context=context, previous_attempts=previous_attempts or []
        )

        answer = await self.ollama_service.generate_text(
            prompt=prompt, model=self.model, temperature=temperature, max_tokens=1000
        )

        logger.info(f"Reasoning completed: {answer[:100]}...")
        return answer

    async def _reflection_phase(self, question: str, answer: str, sources_count: int) -> str:
        """Phase 4: Agent reflects on its answer quality."""
        prompt = self.prompts.create_reflection_prompt(question, answer, sources_count)

        reflection = await self.ollama_service.generate_text(
            prompt=prompt, model=self.model, temperature=0.3, max_tokens=500
        )

        logger.info(f"Reflection completed: {reflection[:100]}...")
        return reflection

    async def _correction_phase(
        self, question: str, original_answer: str, reflection: str, context: str, temperature: float
    ) -> str:
        """Phase 5: Agent corrects and improves its answer."""
        prompt = self.prompts.create_correction_prompt(
            question=question,
            original_answer=original_answer,
            reflection=reflection,
            context=context,
        )

        corrected_answer = await self.ollama_service.generate_text(
            prompt=prompt, model=self.model, temperature=temperature, max_tokens=1000
        )

        logger.info(f"Correction completed: {corrected_answer[:100]}...")
        return corrected_answer

    def _parse_action_response(self, response: str) -> dict[str, Any]:
        """Parse the agent's action response."""
        import json
        import re

        thought_match = re.search(r"THOUGHT:\s*(.+?)(?=ACTION:|$)", response, re.DOTALL)
        action_match = re.search(r"ACTION:\s*(\w+)", response)
        input_match = re.search(r"INPUT:\s*(\{.+?\})", response, re.DOTALL)

        thought = thought_match.group(1).strip() if thought_match else "No thought provided"
        tool_name = action_match.group(1).strip() if action_match else "finish"

        try:
            tool = ToolName(tool_name.lower())
        except ValueError:
            tool = ToolName.FINISH

        tool_input = {}
        if input_match:
            try:
                tool_input = json.loads(input_match.group(1))
            except json.JSONDecodeError:
                logger.warning(f"Failed to parse tool input: {input_match.group(1)}")

        return {"thought": thought, "tool": tool, "input": tool_input}

    async def _execute_tool(
        self, action: AgentAction, search_mode: str, filters: SearchFilters | None
    ) -> AgentObservation:
        """Execute a tool action and return observation."""
        try:
            if action.tool == ToolName.SEARCH_DOCUMENTS:
                query = action.tool_input.get("query", "")
                results = await self.search_service.search(
                    query=query, mode=search_mode, limit=5, filters=filters, rerank=True
                )
                return AgentObservation(action=action, result=results.results, success=True)

            if action.tool == ToolName.REFORMULATE_QUERY:
                original = action.tool_input.get("original", "")
                reformulated = await self._reformulate_query(original)
                results = await self.search_service.search(
                    query=reformulated, mode=search_mode, limit=5, filters=filters, rerank=True
                )
                return AgentObservation(action=action, result=results.results, success=True)

            return AgentObservation(
                action=action,
                result=None,
                success=False,
                error=f"Tool {action.tool} not implemented",
            )

        except Exception as e:
            logger.error(f"Tool execution failed: {e}", exc_info=True)
            return AgentObservation(action=action, result=None, success=False, error=str(e))

    async def _reformulate_query(self, original: str) -> str:
        """Reformulate a query to improve search results."""
        prompt = f"""Improve this search query to get better results from a document search system.
Make it more specific, add relevant keywords, fix any issues.

Original Query: {original}

Improved Query (respond with ONLY the improved query, nothing else):"""

        reformulated = await self.ollama_service.generate_text(
            prompt=prompt, model=self.model, temperature=0.5, max_tokens=100
        )

        return reformulated.strip()

    def _parse_reflection_needs_correction(self, reflection: str) -> bool:
        """Parse reflection to determine if correction is needed."""
        import re

        needs_correction_match = re.search(
            r"NEEDS_CORRECTION:\s*(yes|no)", reflection, re.IGNORECASE
        )

        if needs_correction_match:
            return needs_correction_match.group(1).lower() == "yes"

        quality_score_match = re.search(r"QUALITY_SCORE:\s*(\d+)", reflection)
        if quality_score_match:
            score = int(quality_score_match.group(1))
            return score < self.MIN_QUALITY_SCORE

        return "yes" in reflection.lower() and "correction" in reflection.lower()
