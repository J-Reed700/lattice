import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, FunctionDefinition, FunctionResult } from '@/types';

export const functionsApi = {
  /**
   * Executes an LLM function call with security checks.
   * Executes a function from the registry with rate limiting and validation.
   *
   * @param name - Function name to execute
   * @param args - Function arguments as JSON object
   * @returns Execution result with success flag and data/error
   */
  executeFunction: async (
    name: string,
    args: Record<string, Wire.JsonValue>,
  ): Promise<ApiResult<FunctionResult>> =>
    apiCall<FunctionResult>('execute_function', {
      call: {
        id: crypto.randomUUID(),
        name,
        arguments: args,
      },
    }),

  /**
   * Lists all available tool definitions for LLM function calling.
   * Returns metadata for all registered functions including schemas.
   *
   * @returns Array of tool definitions with names, descriptions, and input schemas
   */
  listAvailableFunctions: async (): Promise<ApiResult<FunctionDefinition[]>> =>
    apiCall<FunctionDefinition[]>('list_available_functions'),

  /**
   * Gets statistics about LLM function calling usage.
   * Returns execution counts, success rates, and timing info.
   *
   * @returns Function calling statistics
   */
  getFunctionStats: async (): Promise<ApiResult<Record<string, unknown>>> =>
    apiCall<Record<string, unknown>>('get_function_stats'),
};
