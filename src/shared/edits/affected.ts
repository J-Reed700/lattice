import type { EditScope } from './types';
import type { QueryClient } from '@tanstack/react-query';

export type Refresh = (scope: EditScope, queryClient: QueryClient) => Promise<void> | void;

const handlers = new Map<string, Refresh>();

export function registerAffected(name: string, refresh: Refresh): void {
  handlers.set(name, refresh);
}

export async function refreshAffected(
  names: readonly string[],
  scope: EditScope,
  queryClient: QueryClient,
): Promise<void> {
  const seen = new Set<string>();
  const pending: Promise<void>[] = [];
  for (const name of names) {
    if (seen.has(name)) continue;
    seen.add(name);
    const refresh = handlers.get(name);
    if (!refresh) continue;
    pending.push(Promise.resolve(refresh(scope, queryClient)));
  }
  await Promise.all(pending);
}
