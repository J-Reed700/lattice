import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { LIBRARY_DOCUMENTS_QUERY_KEY, useLibraryDocumentsQuery } from './useLibraryDocumentsQuery';

vi.mock('@/lib/api', () => ({ default: { listAllDocuments: vi.fn() } }));
vi.mock('@/hooks/queries/useClustersQuery', () => ({ useClustersQuery: () => ({ data: [] }) }));

afterEach(() => { vi.useRealTimers(); vi.clearAllMocks(); });

const imported: DocumentMetadata = {
  id: 'imported', fileName: 'TR83ICSI.pdf', filePath: '/library/TR83ICSI.pdf',
  fileType: 'pdf', category: 'document', language: 'en',
  modifiedAt: '2026-09-16', indexedAt: '2026-09-16', wordCount: 100,
};

it('shows a completed import when returning to a still-fresh library cache', async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(LIBRARY_DOCUMENTS_QUERY_KEY, []);
  vi.mocked(VaultAPI.listAllDocuments).mockResolvedValue({ ok: true, data: [imported] });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const hook = renderHook(() => useLibraryDocumentsQuery(), { wrapper });
  await waitFor(() => expect(hook.result.current.filteredDocuments).toEqual([imported]));
  hook.unmount();
  client.clear();
});

it('discovers background imports while the library stays open', async () => {
  vi.useFakeTimers();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  vi.mocked(VaultAPI.listAllDocuments).mockResolvedValue({ ok: true, data: [] });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const hook = renderHook(() => useLibraryDocumentsQuery(), { wrapper });
  await act(async () => { await vi.advanceTimersByTimeAsync(10); });
  vi.mocked(VaultAPI.listAllDocuments).mockResolvedValue({ ok: true, data: [imported] });
  await act(async () => { await vi.advanceTimersByTimeAsync(5_100); });
  expect(hook.result.current.filteredDocuments).toEqual([imported]);
  hook.unmount();
  client.clear();
});
