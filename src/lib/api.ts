/**
 * Public API facade. Feature clients own commands; shared IPC owns transport.
 *
 * Every member is an async call, so the clients load on the first one rather
 * than with the initial bundle. Each member keeps one identity, so it is safe
 * in dependency arrays and as a query function.
 */
import type { VaultClients } from './apiClients';
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

export type UnwrapResult<T> =
  | { ok: true; data: T }
  | { ok: false; error?: unknown; data?: unknown }
  | { status: 'ok'; data: T }
  | { status: 'error'; error?: unknown; data?: unknown }
  | { ok: boolean; data?: T; error?: unknown }
  | { status: 'ok' | 'error'; data?: T; error?: unknown };

export function unwrap<T>(result: UnwrapResult<T>): T {
  if (result && typeof result === 'object') {
    if ('ok' in result) {
      if (result.ok) return (result as { data: T }).data;
      if (result.error instanceof Error) throw result.error;
      throw new Error(getErrorMessage(result.error, 'The request could not be completed.'));
    }
    if ('status' in result) {
      if (result.status === 'ok') return (result as { data: T }).data;
      if (result.error instanceof Error) throw result.error;
      throw new Error(getErrorMessage(result.error, 'The request could not be completed.'));
    }
  }
  throw new Error('Invalid result structure');
}

function generateUuid(): string {
  if (typeof globalThis.crypto?.randomUUID === 'function') {
    return globalThis.crypto.randomUUID();
  }
  const bytes = new Uint8Array(16);
  if (typeof globalThis.crypto?.getRandomValues === 'function') {
    globalThis.crypto.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i += 1) {
      bytes[i] = Math.floor(Math.random() * 256);
    }
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function newId(prefix?: string): string {
  const uuid = generateUuid();
  return prefix ? `${prefix}-${uuid}` : uuid;
}

export default VaultAPI;
