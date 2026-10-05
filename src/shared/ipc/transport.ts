import { invoke } from '@tauri-apps/api/core';

import { parseApiError } from '@/lib/errorHandling';
import { beginStudyActivity } from '@/lib/studyActivity';
import type { ApiResult } from '@/types';
import { diagnostics } from '@/utils/diagnostics';

import { COMMAND_DOMAIN_MAP } from './routes';

/**
 * Tauri's own rejection when no handler owns the invoked route: the core's
 * `Command {cmd} not found` / `plugin {name} not found`, and the ACL's
 * `… not allowed. Command not found` / `Plugin not found`. Only these mean
 * "try another route"; a backend error that merely contains "not found"
 * (e.g. "Space not found: x") is a real answer and must reach the caller.
 */
const UNKNOWN_COMMAND_PATTERNS: readonly RegExp[] = [
  /^Command \S+ not found$/,
  /^plugin \S+ not found$/,
  /not allowed\. (Command|Plugin) not found$/,
];

export function isUnknownCommandError(error: unknown): boolean {
  const message =
    typeof error === 'string'
      ? error
      : error instanceof Error
        ? error.message
        : '';
  return UNKNOWN_COMMAND_PATTERNS.some((pattern) =>
    pattern.test(message.trim()),
  );
}

/**
 * Wrap a Tauri command invocation with ApiResult type
 * Routes through Plugin Pattern using plugin:domain|command syntax
 */
async function invokeCommandWithFallback<T>(
  args: Record<string, unknown> | undefined,
  domain: string,
  pluginCommand: string,
): Promise<T> {
  const attempts = [
    `plugin:${domain}|${pluginCommand}`,
    `${domain}.${pluginCommand}`,
    pluginCommand,
  ];

  // A route whose handler answered, even with an error, ends the search.
  // When every route is unknown, the first (canonical) route's error is the
  // one worth showing; the fallbacks' "not found" would only hide it.
  let firstError: unknown = null;
  for (let idx = 0; idx < attempts.length; idx += 1) {
    try {
      return await invoke<T>(attempts[idx], args || {});
    } catch (error) {
      if (idx === 0) firstError = error;
      if (!isUnknownCommandError(error)) throw error;
    }
  }

  throw firstError;
}

export async function apiCall<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<ApiResult<T>> {
  const pluginRoute = COMMAND_DOMAIN_MAP[command];
  if (!pluginRoute) {
    // A missing route is a programming error, not a backend failure: every
    // command must be declared in COMMAND_DOMAIN_MAP so `contracts:check` can
    // verify it against the generated bindings.
    throw new Error(
      `[API] Command '${command}' has no COMMAND_DOMAIN_MAP entry`,
    );
  }

  const activity = beginStudyActivity(pluginRoute.domain, command, args);
  try {
    const data = await invokeCommandWithFallback<T>(
      args,
      pluginRoute.domain,
      pluginRoute.command,
    );

    activity?.finish(undefined, data);
    return { ok: true, data };
  } catch (error) {
    const apiError = parseApiError(error);
    activity?.finish(apiError.message);
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
    const nestedMessage =
      typeof nestedError.message === 'string'
        ? nestedError.message
        : fallbackError;
    return {
      ok: false,
      error: nestedMessage,
      details: {
        code:
          typeof nestedError.code === 'string' ? nestedError.code : 'UNKNOWN',
        message: nestedMessage,
        details: nestedError,
      },
    };
  }

  return { ok: true, data: response.data as T };
}
