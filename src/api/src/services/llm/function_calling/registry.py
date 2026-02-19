"""Function registry for managing tool definitions.

The registry pattern provides:
- Centralized tool management
- Runtime tool discovery
- Type-safe tool definitions
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

from pydantic import BaseModel


@dataclass
class ToolDefinition:
    """Definition of a callable tool for LLMs.

    A tool consists of:
    - name: Unique identifier
    - description: What the tool does (shown to LLM)
    - input_schema: Pydantic model for input validation
    - output_schema: Pydantic model for output validation
    - handler: Async function that executes the tool
    - rate_limit: Per-function rate limit (requests per minute)
    """

    name: str
    description: str
    input_schema: type[BaseModel]
    output_schema: type[BaseModel]
    handler: Callable[[BaseModel], Any]
    rate_limit: int = 100  # requests per minute
    metadata: dict[str, Any] = field(default_factory=dict)


class FunctionRegistry:
    """Central registry for function calling tools.

    Manages tool definitions for local LLM function calling.

    Example:
        >>> registry = FunctionRegistry()
        >>> registry.register(my_tool)
        >>> tools = registry.list_tools()
    """

    def __init__(self) -> None:
        """Initialize empty registry."""
        self._tools: dict[str, ToolDefinition] = {}

    def register(self, tool: ToolDefinition) -> None:
        """
        Register a tool in the registry.

        Args:
            tool: Tool definition to register

        Raises:
            ValueError: If tool with same name already registered
        """
        if tool.name in self._tools:
            msg = f"Tool '{tool.name}' already registered"
            raise ValueError(msg)

        self._tools[tool.name] = tool

    def unregister(self, name: str) -> None:
        """
        Unregister a tool from the registry.

        Args:
            name: Tool name to unregister

        Raises:
            KeyError: If tool not found
        """
        if name not in self._tools:
            msg = f"Tool '{name}' not found"
            raise KeyError(msg)

        del self._tools[name]

    def get_tool(self, name: str) -> ToolDefinition | None:
        """
        Get tool definition by name.

        Args:
            name: Tool name

        Returns:
            Tool definition or None if not found
        """
        return self._tools.get(name)

    def list_tools(self) -> list[ToolDefinition]:
        """
        List all registered tools.

        Returns:
            List of tool definitions
        """
        return list(self._tools.values())

    def __len__(self) -> int:
        """Get number of registered tools."""
        return len(self._tools)

    def __contains__(self, name: str) -> bool:
        """Check if tool is registered."""
        return name in self._tools


def get_default_registry() -> FunctionRegistry:
    """
    Create registry with all default tools registered.

    Returns:
        FunctionRegistry with Phase 1 and Phase 2 tools

    Note:
        This is a factory function that creates a new registry instance.
        Tools are registered from tools.py module.
    """
    from .tools import create_all_tools

    registry = FunctionRegistry()

    # Register all tools
    for tool in create_all_tools():
        registry.register(tool)

    return registry
