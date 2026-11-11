import { invoke, type InvokeArgs } from '@tauri-apps/api/core';

import type { CommandArgs, CommandResults } from '@/lib/bindings';
import { parseApiError } from '@/lib/errorHandling';
import type { ApiResult } from '@/types';
import { diagnostics } from '@/utils/diagnostics';

import { COMMAND_PLUGINS, type CommandName } from './routes.generated';

export type { CommandName } from './routes.generated';

/** A command's argument list: optional when every argument may be left out. */
export type CommandArgList<C extends CommandName> = undefined extends CommandArgs[C]
  ? [args?: CommandArgs[C]]
  : [args: CommandArgs[C]];

/**
 * Invoke a command through the plugin that registers it and wrap the outcome
 * in an ApiResult. Failures are recorded in diagnostics, never thrown.
 */
export async function apiCall<C extends CommandName>(
  command: C,
  ...[args]: CommandArgList<C>
): Promise<ApiResult<CommandResults[C]>> {
  try {
    const data = await invoke<CommandResults[C]>(
      `plugin:${COMMAND_PLUGINS[command]}|${command}`,
      (args ?? {}) as InvokeArgs,
    );
    return { ok: true, data };
  } catch (error) {
    const apiError = parseApiError(error);
    diagnostics.capture(
      apiError,
      `API · ${command}`,
      error instanceof Error ? { stack: error.stack } : undefined,
    );
    return {
      ok: false,
      error: apiError.message,
      details: apiError,
    };
  }
}

export function unwrapNestedApiResult<T>(
  response: ApiResult<unknown>,
  fallbackError: string,
): ApiResult<T> {
  if (!response.ok) {
    return response as unknown as ApiResult<T>;
  }

  const payload = response.data as Record<string, unknown> | null;
  if (
    payload &&
    typeof payload === 'object' &&
    Object.prototype.hasOwnProperty.call(payload, 'ok') &&
    typeof payload.ok === 'boolean'
  ) {
    if (payload.ok === true) {
      return { ok: true, data: payload.data as T };
    }

    const nestedError = (payload.error ?? {}) as Record<string, unknown>;
    const details = parseApiError({
      code: nestedError.code,
      message:
        typeof nestedError.message === 'string'
          ? nestedError.message
          : fallbackError,
      details: nestedError.details,
    });
    return { ok: false, error: details.message, details };
  }

  return { ok: true, data: response.data as T };
}
