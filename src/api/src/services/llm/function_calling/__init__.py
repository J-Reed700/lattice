"""Function calling system for LLM tools.

This module implements a registry-based function calling system that allows
LLMs to invoke tools to retrieve information from the vault and the web.

Public Interface:
- FunctionRegistry: Central registry for managing tool definitions
- FunctionExecutor: Executes function calls with security controls
- get_default_registry: Factory function for pre-configured registry

Design Philosophy (Bricks and Studs):
- Self-contained: All function calling logic in this module
- Clear interface: Public API via __all__
- Local-first: Works with local LLMs (Ollama)
- Security-first: Rate limiting, validation, audit logging
"""

from __future__ import annotations

from .executor import FunctionExecutor
from .registry import FunctionRegistry, ToolDefinition, get_default_registry

__all__ = [
    "FunctionExecutor",
    "FunctionRegistry",
    "ToolDefinition",
    "get_default_registry",
]
