/**
 * Function Calling API Types
 *
 * Type definitions for LLM function calling operations.
 * These types match the Rust backend structures from domain/function_call.rs
 */

/**
 * Function definition for LLM tool use
 *
 * Represents a callable function with its metadata and parameter schema.
 * Used to expose functions to LLMs for agentic workflows.
 */
export interface FunctionDefinition {
  /** Function name (alphanumeric + underscores) */
  name: string;

  /** Human-readable description of functionality */
  description: string;

  /** JSON Schema for input parameters */
  inputSchema: {
    type: 'object';
    properties: Record<string, unknown>;
    required?: string[];
  };
}

/**
 * Function call from LLM
 *
 * Represents an LLM's intent to execute a specific function with arguments.
 */
export interface FunctionCall {
  /** Unique identifier for this function call */
  id: string;

  /** Name of the function to execute */
  name: string;

  /** Function arguments as JSON object */
  arguments: Record<string, unknown>;
}

/**
 * Result of executing a function call
 *
 * Encapsulates success or failure with detailed error information.
 */
export interface FunctionResult {
  /** Whether the function executed successfully */
  success: boolean;

  /** Result data (if success = true) */
  data?: unknown;

  /** Error code (if success = false) */
  errorCode?: string;

  /** Error message (if success = false) */
  errorMessage?: string;
}

/**
 * Function registry statistics
 *
 * Usage and performance metrics for the function registry.
 */
export interface RegistryStats {
  /** Total number of registered functions */
  totalFunctions: number;

  /** Total number of function executions */
  executionCount: number;

  /** Number of successful executions */
  successCount: number;

  /** Number of failed executions */
  failureCount: number;

  /** Average execution time in milliseconds */
  averageExecutionTimeMs: number;
}
