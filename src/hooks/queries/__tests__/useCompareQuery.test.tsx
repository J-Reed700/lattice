import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CompareTableDto } from '@/types/api/compare';

import { compareQueryKey, useCompareQuery } from '../useCompareQuery';

const compareDocuments = vi.fn();

vi.mock('@/lib/api', () => {
  const api = { compareDocuments: (...args: unknown[]) => compareDocuments(...args) };
  return { VaultAPI: api, default: api };
});

const table: CompareTableDto = {
  columns: ['method'],
  rows: [
    {
      documentId: 'doc_1',
      title: 'trial.pdf',
      filePath: '/vault/trial.pdf',
      cells: [{ value: 'randomised controlled trial', citation: null }],
      error: null,
    },
  ],
  modelName: 'llama-3',
  generatedAt: '2026-09-06T12:00:00.000Z',
};

const wrapper = ({ children }: { children: ReactNode }) => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
};

describe('useCompareQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    compareDocuments.mockResolvedValue({ ok: true, data: table });
  });

  it('keys one cache entry per (documents, columns)', () => {
    expect(compareQueryKey(['a', 'b'], ['method'])).toEqual([
      'compare',
      ['a', 'b'],
      ['method'],
    ]);
  });

  it('builds the table for two or more documents', async () => {
    const { result } = renderHook(
      () => useCompareQuery(['doc_1', 'doc_2'], ['method'], true),
      { wrapper },
    );

    await waitFor(() => expect(result.current.data).toEqual(table));
    expect(compareDocuments).toHaveBeenCalledWith({
      documentIds: ['doc_1', 'doc_2'],
      columns: ['method'],
    });
  });

  it('does not run until it is asked to', () => {
    renderHook(() => useCompareQuery(['doc_1', 'doc_2'], ['method'], false), { wrapper });
    expect(compareDocuments).not.toHaveBeenCalled();
  });

  it('does not run for fewer than two documents or no columns', () => {
    renderHook(() => useCompareQuery(['doc_1'], ['method'], true), { wrapper });
    renderHook(() => useCompareQuery(['doc_1', 'doc_2'], [], true), { wrapper });
    expect(compareDocuments).not.toHaveBeenCalled();
  });

  it('surfaces a backend failure as an error with its message', async () => {
    compareDocuments.mockResolvedValue({ ok: false, error: 'No model is loaded.' });
    const { result } = renderHook(
      () => useCompareQuery(['doc_1', 'doc_2'], ['method'], true),
      { wrapper },
    );

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.error?.message).toBe('No model is loaded.');
  });
});
