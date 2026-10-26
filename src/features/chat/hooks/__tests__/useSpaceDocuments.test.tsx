import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, type RenderHookOptions, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useSpaceDocuments } from '@/features/chat/hooks/useSpaceDocuments';
import type { SpaceDocument } from '@/types';

const listSpaceDocuments = vi.hoisted(() => vi.fn());

vi.mock('@/lib/api', () => {
  const mockApi = { listSpaceDocuments };
  return { VaultAPI: mockApi, default: mockApi };
});

function renderWithClient<Result, Props>(hook: (props: Props) => Result, options?: RenderHookOptions<Props>) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderHook(hook, { ...options, wrapper });
}

const halvorsen: SpaceDocument = {
  documentId: 'doc-canopy-meta',
  fileName: 'Halvorsen 2024.pdf',
  category: 'Research Paper',
  modifiedAt: '2026-09-18T00:00:00Z',
};

describe('what `@` is allowed to offer', () => {
  beforeEach(() => {
    listSpaceDocuments.mockReset();
    listSpaceDocuments.mockResolvedValue({ ok: true, data: [halvorsen] });
  });

  // Space isolation: the popup may only ever name documents this chat can read,
  // so it asks the one command that answers from the retrieval scope.
  it('asks for the documents of the space it was given', async () => {
    const { result } = renderWithClient(() => useSpaceDocuments('space-thesis', 'conv-1', 'halv'));

    await waitFor(() => expect(result.current.documents).toEqual([halvorsen]));
    expect(listSpaceDocuments).toHaveBeenCalledWith('space-thesis', 'conv-1', 'halv', 8);
  });

  it('asks nothing at all while nothing is being looked up', async () => {
    const { result } = renderWithClient(() => useSpaceDocuments('space-thesis', 'conv-1', null));

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(listSpaceDocuments).not.toHaveBeenCalled();
    expect(result.current.documents).toEqual([]);
  });

  it('offers nothing when the lookup fails, rather than the last answer', async () => {
    const { result, rerender } = renderWithClient(
      ({ query }: { query: string }) => useSpaceDocuments('space-thesis', 'conv-1', query),
      { initialProps: { query: 'halv' } }
    );
    await waitFor(() => expect(result.current.documents).toEqual([halvorsen]));

    listSpaceDocuments.mockResolvedValue({ ok: false, error: 'no such space' });
    rerender({ query: 'transect' });

    await waitFor(() => expect(result.current.documents).toEqual([]));
  });

  it('keeps the list up while the next keystroke is looked up, but never another space\'s list', async () => {
    const { result, rerender } = renderWithClient(
      ({ spaceId, query }: { spaceId: string; query: string }) => useSpaceDocuments(spaceId, 'conv-1', query),
      { initialProps: { spaceId: 'space-thesis', query: 'halv' } }
    );
    await waitFor(() => expect(result.current.documents).toEqual([halvorsen]));

    listSpaceDocuments.mockReturnValue(new Promise(() => {}));
    rerender({ spaceId: 'space-thesis', query: 'halvo' });
    expect(result.current.isLoading).toBe(true);
    await waitFor(() => expect(listSpaceDocuments).toHaveBeenCalledWith('space-thesis', 'conv-1', 'halvo', 8));
    expect(result.current.documents).toEqual([halvorsen]);

    rerender({ spaceId: 'space-other', query: 'halvo' });
    await waitFor(() => expect(listSpaceDocuments).toHaveBeenCalledWith('space-other', 'conv-1', 'halvo', 8));
    expect(result.current.documents).toEqual([]);
  });
});
