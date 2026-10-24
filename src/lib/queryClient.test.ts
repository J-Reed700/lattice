import { describe, expect, it } from 'vitest';

import { queryClient } from './queryClient';
import { diagnostics } from '../utils/diagnostics';


describe('queryClient', () => {
  it('records failed background reads and saves in diagnostics', async () => {
    diagnostics.clear();
    await queryClient.fetchQuery({ queryKey: ['test', 'failing-read'], queryFn: async () => { throw new Error('read failed'); }, retry: false }).catch(() => undefined);
    await queryClient.getMutationCache().build(queryClient, { mutationFn: async () => { throw new Error('save failed'); } }).execute(undefined).catch(() => undefined);
    const recorded = diagnostics.getSnapshot().map((entry) => [entry.source, entry.message]);
    expect(recorded).toEqual(expect.arrayContaining([['Background query', 'read failed'], ['Save operation', 'save failed']]));
  });

  it('never retries a mutation by default', () => {
    expect(queryClient.getDefaultOptions().mutations?.retry).toBe(false);
    const retryDelay = queryClient.getDefaultOptions().queries?.retryDelay as (attempt: number) => number;
    expect([0, 1, 2, 10].map((attempt) => retryDelay(attempt))).toEqual([1000, 2000, 4000, 30000]);
  });
});
