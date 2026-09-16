/**
 * Function Calling API Types
 *
 * Type definitions for LLM function calling operations.
 * These types match the Rust backend structures from domain/modules/function_call.rs
 */

/**
 * Function definition for LLM tool use
 *
 * Represents a callable function with its metadata and parameter schema.
 * Used to expose functions to LLMs for agentic workflows.
 */
export type FunctionDefinition = import('../../lib/bindings').ToolDefinition;

/**
 * Function call from LLM
 *
 * Represents an LLM's intent to execute a specific function with arguments.
 */
export type FunctionCall = import('../../lib/bindings').FunctionCall;

/**
 * Result of executing a function call
 *
 * Encapsulates success or failure with detailed error information.
 */
export type FunctionResult = import('../../lib/bindings').FunctionResult;

/**
 * Function registry statistics
 *
 * Usage and performance metrics for the function registry.
 */
export type RegistryStats = import('../../lib/bindings').RegistryStats;
