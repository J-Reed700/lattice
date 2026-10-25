import VaultAPI, { type ApiClient } from '@/lib/api';

/** A scoped test double; unmodified methods retain the standard test adapter. */
export function createTestApi(overrides: Partial<ApiClient>): ApiClient {
  return Object.assign(Object.create(VaultAPI) as ApiClient, overrides);
}
