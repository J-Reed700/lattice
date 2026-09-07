import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import type { PassageReferenceDto } from '@/types/api/passageReferences';

import {
  usePassageReferenceIds,
  usePassageReferencesQuery,
} from '../usePassageReferencesQuery';


const listPassageReferences = VaultAPI.listPassageReferences as unknown as ReturnType<
  typeof vi.fn
>;

const wrapper = ({ children }: { children: ReactNode }) => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
};

const reference = (overrides: Partial<PassageReferenceDto> = {}): PassageReferenceDto => ({
  id: 'pref_1',
  documentId: 'doc-1',
  chunkId: 'chunk-1',
  filePath: '/vault/paper.pdf',
  fileName: 'paper.pdf',
  locator: 'p. 12',
  text: 'The sample size was 42 participants.',
  title: null,
  note: null,
  createdAt: '2026-09-05T10:00:00Z',
  ...overrides,
});

describe('usePassageReferencesQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('reports a load failure instead of resolving an empty list', async () => {
    // An empty list reads as "you have saved nothing"; the inbox needs to be
    // able to say "we could not look".
    listPassageReferences.mockResolvedValue({ ok: false, error: 'db locked' });

    const { result } = renderHook(() => usePassageReferencesQuery(), { wrapper });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.data).toBeUndefined();
  });

  it('returns the saved references when the call succeeds', async () => {
    listPassageReferences.mockResolvedValue({ ok: true, data: [reference()] });

    const { result } = renderHook(() => usePassageReferencesQuery(), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toHaveLength(1);
  });
});

describe('usePassageReferenceIds', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('keeps the citation mark silent when the list cannot be loaded', async () => {
    listPassageReferences.mockResolvedValue({ ok: false, error: 'db locked' });

    const { result } = renderHook(() => usePassageReferenceIds(), { wrapper });

    await waitFor(() => expect(result.current).toEqual([]));
  });

  it('exposes the document and chunk ids a citation is matched against', async () => {
    listPassageReferences.mockResolvedValue({
      ok: true,
      data: [reference(), reference({ id: 'pref_2', documentId: 'doc-2', chunkId: null })],
    });

    const { result } = renderHook(() => usePassageReferenceIds(), { wrapper });

    await waitFor(() => expect(result.current).toHaveLength(2));
    expect(result.current).toEqual([
      { documentId: 'doc-1', chunkId: 'chunk-1' },
      { documentId: 'doc-2', chunkId: null },
    ]);
  });
});
