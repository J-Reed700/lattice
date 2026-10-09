/**
 * Public API facade. Feature clients own commands; shared IPC owns transport.
 *
 * Every member is an async call, so the clients load on the first one rather
 * than with the initial bundle. Each member keeps one identity, so it is safe
 * in dependency arrays and as a query function.
 */
import type { VaultClients } from './apiClients';

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
export default VaultAPI;
