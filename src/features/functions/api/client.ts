import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, FunctionDefinition, FunctionResult } from '@/types';

export const functionsApi = {
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

  listAvailableFunctions: async (): Promise<ApiResult<FunctionDefinition[]>> =>
    apiCall<FunctionDefinition[]>('list_available_functions'),

  getFunctionStats: async (): Promise<ApiResult<Record<string, unknown>>> =>
    apiCall<Record<string, unknown>>('get_function_stats'),
};
