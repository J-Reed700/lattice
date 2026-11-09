/**
 * Public API facade. Feature clients own commands; shared IPC owns transport.
 *
 * Every member is an async call, so the clients load on the first one rather
 * than with the initial bundle. Each member keeps one identity, so it is safe
 * in dependency arrays and as a query function.
 */
import { type ApiResult, unwrapApiResult } from '@/types/api/result';

import type { VaultClients } from './apiClients';
import type * as Wire from './bindings';
import { getErrorMessage } from './errorUtils';

export type ApiClient = VaultClients;

type ClientMethod = (...args: unknown[]) => Promise<unknown>;

let loading: Promise<Record<string, ClientMethod>> | undefined;
function loadClients() {
  loading ??= import('./apiClients').then(
    ({ vaultClients }) => vaultClients as unknown as Record<string, ClientMethod>,
  );
  return loading;
}

const methods = new Map<string, ClientMethod>();

export const VaultAPI = new Proxy({} as VaultClients, {
  get(_target, name) {
    // Not a thenable, and no symbol-keyed members: only named client calls.
    if (typeof name !== 'string' || name === 'then') return undefined;
    let method = methods.get(name);
    if (!method) {
      method = async (...args) => {
        const clients = await loadClients();
        return clients[name](...args);
      };
      methods.set(name, method);
    }
    return method;
  },
});

/** The value of a successful result; a failed one throws, keeping the backend's error details. */
export function unwrap<T>(result: ApiResult<T> | Wire.Result<T, unknown>): T {
  if ('ok' in result) return unwrapApiResult(result);
  if (result.status === 'ok') return result.data;
  throw result.error instanceof Error ? result.error : new Error(getErrorMessage(result.error));
}

/** A fresh id for an operation, session or record. */
export function newId(): string {
  return crypto.randomUUID();
}

/** Whether the backend refused a write because the record changed since it was read. */
export function isConflict(error: unknown): boolean {
  if (typeof error !== 'object' || error === null || !('details' in error)) return false;
  const { details } = error;
  return typeof details === 'object' && details !== null && 'code' in details && details.code === 'CONFLICT';
}

export default VaultAPI;
