import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useCorrectedSince } from './useCorrectedSince';

import type { ReactNode } from 'react';

const mocks = vi.hoisted(() => ({
  listContentEdits: vi.fn(),
}));

vi.mock('./api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api')>();
  return {
    ...actual,
    listContentEdits: mocks.listContentEdits,
  };
});

const content = { kind: 'lesson.section', key: 's1' };

function createWrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

describe('useCorrectedSince', () => {
  beforeEach(() => {
    mocks.listContentEdits.mockReset();
  });

  it('does not query when since is null, and returns true when edits exist since the timestamp', async () => {
    mocks.listContentEdits.mockResolvedValue([
      {
        id: 'edit-1',
        content,
        seq: 1,
        origin: { type: 'learner' },
        paths: ['/title'],
        createdAt: 200,
        revertsEditId: null,
      },
    ]);

    const { result, rerender } = renderHook(
      ({ since }: { since: number | null }) => useCorrectedSince(content, since),
      { initialProps: { since: null as number | null }, wrapper: createWrapper() },
    );

    expect(result.current).toBe(false);
    expect(mocks.listContentEdits).not.toHaveBeenCalled();

    rerender({ since: 100 });
    await waitFor(() => expect(result.current).toBe(true));
    expect(mocks.listContentEdits).toHaveBeenCalledWith({
      scope: null,
      contents: [content],
      since: 100,
    });
  });
});
