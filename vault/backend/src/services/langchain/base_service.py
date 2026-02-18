from collections.abc import AsyncGenerator
import logging
import time
from typing import Any

from langchain.agents import AgentExecutor, create_react_agent
from langchain.callbacks import AsyncIteratorCallbackHandler
from langchain.prompts import PromptTemplate
from langchain.tools import Tool
from langchain_ollama import ChatOllama

from src.config.settings import Settings

from .models import (
    AgentConfig,
    AgentResponse,
    AgentStreamChunk,
    ToolDefinition,
    ToolRegistrationResult,
)

logger = logging.getLogger(__name__)


class BaseLangChainService:
    def __init__(self, settings: Settings) -> None:
        self._settings = settings
        self._llm: ChatOllama | None = None
        self._agent_executor: AgentExecutor | None = None
        self._tools: list[Tool] = []
        self._tool_registry: dict[str, ToolDefinition] = {}
        self._initialized = False

    @property
    def llm(self) -> ChatOllama:
        if self._llm is None:
            self._llm = self._create_llm()
        return self._llm

    def _create_llm(self, config: AgentConfig | None = None) -> ChatOllama:
        if config is None:
            config = self._default_config()

        llm = ChatOllama(
            model=config.model,
            base_url=self._settings.ollama_base_url,
            temperature=config.temperature,
            timeout=self._settings.ollama_timeout,
        )

        logger.info(
            f"Created Ollama LLM with model={config.model}, "
            f"temperature={config.temperature}, base_url={self._settings.ollama_base_url}"
        )

        return llm

    def _default_config(self) -> AgentConfig:
        return AgentConfig(
            model=self._settings.langchain_ollama_model,
            temperature=self._settings.langchain_ollama_temperature,
            max_iterations=self._settings.langchain_max_iterations,
            max_execution_time=self._settings.langchain_max_execution_time,
            verbose=self._settings.langchain_verbose,
            enable_tracing=self._settings.langchain_enable_tracing,
        )

    def register_tool(self, tool_def: ToolDefinition) -> ToolRegistrationResult:
        try:
            if tool_def.name in self._tool_registry:
                logger.warning(f"Tool '{tool_def.name}' already registered, updating...")

            if tool_def.func is None and tool_def.async_func is None:
                error_msg = f"Tool '{tool_def.name}' must have either func or async_func"
                logger.error(error_msg)
                return ToolRegistrationResult(
                    tool_name=tool_def.name,
                    success=False,
                    error=error_msg,
                )

            tool_func = tool_def.async_func if tool_def.async_func else tool_def.func
            coroutine = tool_def.async_func is not None

            tool = Tool(
                name=tool_def.name,
                description=tool_def.description,
                func=tool_func,
                coroutine=coroutine,
                return_direct=tool_def.return_direct,
            )

            self._tools.append(tool)
            self._tool_registry[tool_def.name] = tool_def

            self._initialized = False

            logger.info(
                f"Registered tool '{tool_def.name}' "
                f"(async={coroutine}, return_direct={tool_def.return_direct})"
            )

            return ToolRegistrationResult(
                tool_name=tool_def.name,
                success=True,
                error=None,
            )

        except Exception as e:
            error_msg = f"Failed to register tool '{tool_def.name}': {e!s}"
            logger.error(error_msg, exc_info=True)
            return ToolRegistrationResult(
                tool_name=tool_def.name,
                success=False,
                error=error_msg,
            )

    def unregister_tool(self, tool_name: str) -> bool:
        if tool_name not in self._tool_registry:
            logger.warning(f"Tool '{tool_name}' not found in registry")
            return False

        self._tools = [t for t in self._tools if t.name != tool_name]
        del self._tool_registry[tool_name]

        self._initialized = False

        logger.info(f"Unregistered tool '{tool_name}'")
        return True

    def list_tools(self) -> list[str]:
        return list(self._tool_registry.keys())

    def get_tool_info(self, tool_name: str) -> ToolDefinition | None:
        return self._tool_registry.get(tool_name)

    def initialize_agent(
        self,
        config: AgentConfig | None = None,
        custom_prompt: str | None = None,
    ) -> AgentExecutor:
        if config is None:
            config = self._default_config()

        llm = self._create_llm(config)

        if custom_prompt:
            prompt_template = PromptTemplate.from_template(custom_prompt)
        else:
            prompt_template = self._get_default_prompt()

        agent = create_react_agent(
            llm=llm,
            tools=self._tools,
            prompt=prompt_template,
        )

        agent_executor = AgentExecutor(
            agent=agent,
            tools=self._tools,
            verbose=config.verbose,
            max_iterations=config.max_iterations,
            max_execution_time=config.max_execution_time,
            handle_parsing_errors=True,
            return_intermediate_steps=True,
        )

        self._agent_executor = agent_executor
        self._initialized = True

        logger.info(
            f"Initialized agent with {len(self._tools)} tools, "
            f"max_iterations={config.max_iterations}"
        )

        return agent_executor

    def _get_default_prompt(self) -> PromptTemplate:
        template = """You are a helpful assistant that can use tools to help answer questions.

You have access to the following tools:

{tools}

Use the following format:

Question: the input question you must answer
Thought: you should always think about what to do
Action: the action to take, should be one of [{tool_names}]
Action Input: the input to the action
Observation: the result of the action
... (this Thought/Action/Action Input/Observation can repeat N times)
Thought: I now know the final answer
Final Answer: the final answer to the original input question

Begin!

Question: {input}
Thought: {agent_scratchpad}"""

        return PromptTemplate.from_template(template)

    async def invoke_agent(
        self,
        input_text: str,
        config: AgentConfig | None = None,
        additional_context: dict[str, Any] | None = None,
    ) -> AgentResponse:
        if not self._initialized or self._agent_executor is None:
            self.initialize_agent(config)

        start_time = time.time()

        try:
            agent_input = {"input": input_text}
            if additional_context:
                agent_input.update(additional_context)

            result = await self._agent_executor.ainvoke(agent_input)

            execution_time = time.time() - start_time

            response = AgentResponse(
                output=result.get("output", ""),
                intermediate_steps=self._format_intermediate_steps(
                    result.get("intermediate_steps", [])
                ),
                execution_time=execution_time,
                success=True,
                error=None,
                metadata={
                    "input": input_text,
                    "tool_count": len(self._tools),
                },
            )

            logger.info(
                f"Agent invocation completed in {execution_time:.2f}s "
                f"with {len(response.intermediate_steps)} steps"
            )

            return response

        except Exception as e:
            execution_time = time.time() - start_time
            error_msg = f"Agent invocation failed: {e!s}"
            logger.error(error_msg, exc_info=True)

            return AgentResponse(
                output="",
                intermediate_steps=[],
                execution_time=execution_time,
                success=False,
                error=error_msg,
                metadata={
                    "input": input_text,
                    "tool_count": len(self._tools),
                },
            )

    async def stream_agent(
        self,
        input_text: str,
        config: AgentConfig | None = None,
        additional_context: dict[str, Any] | None = None,
    ) -> AsyncGenerator[AgentStreamChunk, None]:
        if not self._initialized or self._agent_executor is None:
            self.initialize_agent(config)

        callback_handler = AsyncIteratorCallbackHandler()

        try:
            agent_input = {"input": input_text}
            if additional_context:
                agent_input.update(additional_context)

            from langchain.callbacks import AsyncCallbackManager

            self.llm.bind(callbacks=AsyncCallbackManager([callback_handler]))

            import asyncio

            async def run_agent() -> None:
                try:
                    await self._agent_executor.ainvoke(
                        agent_input,
                        config={"callbacks": [callback_handler]},
                    )
                except Exception as e:
                    logger.error(f"Agent execution error: {e}", exc_info=True)
                finally:
                    callback_handler.done.set()

            task = asyncio.create_task(run_agent())

            async for token in callback_handler.aiter():
                yield AgentStreamChunk(
                    chunk_type="token",
                    content=token,
                    metadata={"input": input_text},
                )

            await task

            yield AgentStreamChunk(
                chunk_type="final",
                content="",
                metadata={"completed": True},
            )

        except Exception as e:
            error_msg = f"Streaming error: {e!s}"
            logger.error(error_msg, exc_info=True)

            yield AgentStreamChunk(
                chunk_type="error",
                content=error_msg,
                metadata={"error": True},
            )

    def _format_intermediate_steps(self, steps: list[tuple[Any, str]]) -> list[dict[str, Any]]:
        formatted_steps = []

        for action, observation in steps:
            step = {
                "action": getattr(action, "tool", "unknown"),
                "action_input": getattr(action, "tool_input", ""),
                "observation": observation,
            }
            formatted_steps.append(step)

        return formatted_steps

    def reset(self) -> None:
        self._agent_executor = None
        self._initialized = False
        logger.info("Agent executor reset")

    def get_config_info(self) -> dict[str, Any]:
        config = self._default_config()
        return {
            "initialized": self._initialized,
            "tool_count": len(self._tools),
            "registered_tools": list(self._tool_registry.keys()),
            "ollama_base_url": self._settings.ollama_base_url,
            "model": config.model,
            "temperature": config.temperature,
            "max_iterations": config.max_iterations,
            "max_execution_time": config.max_execution_time,
        }
