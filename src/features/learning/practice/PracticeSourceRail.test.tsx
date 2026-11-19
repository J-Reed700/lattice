import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { PracticeCitations, sourceClosedReason, usePracticeSourceReader } from '@/features/learning/practice/PracticeSourceRail';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';

const activeVersion = {
  id: 'version-1', versionNumber: 1, title: 'Field notes', publisher: 'Open Archive', resolvedUrl: 'https://archive.example/field',
  excerpt: 'A saved passage.', contentSha256: 'sha256-a', wordCount: 9, truncated: false,
  extractionVersion: 'reader-1', acquiredAt: 1_790_000_000_000, snapshotHash: null,
};
const librarySource = {
  id: 'source-1', kind: 'web', addedBy: 'user', origin: 'web', requestedUrl: 'https://archive.example/field', freshnessPolicy: 'manual',
  activeVersionId: 'version-1', pendingVersionId: null, revision: 1, deletedAt: null, deletionReason: null,
  activeVersion, pendingVersion: null, versions: [activeVersion], latestCheck: null, checks: [], createdAt: 0, updatedAt: 0,
};
const workspace = { programId: 'program-1', recheck: null, sources: [librarySource] };
const versionDetail = { sourceId: 'source-1', version: activeVersion, fullText: 'A saved passage about careful observation.', usage: [] };
const ok = <T,>(data: T) => ({ ok: true as const, data });

const getWorkspace = vi.fn();
const getVersion = vi.fn();

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  const api = { getLearningSourceWorkspace: getWorkspace, getLearningSourceVersion: getVersion } as unknown as ApiClient;
  return (
    <ApiProvider client={api}>
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    </ApiProvider>
  );
}

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class {
    observe() {}
    unobserve() {}
    disconnect() {}
  });
  getWorkspace.mockReset().mockResolvedValue(ok(workspace));
  getVersion.mockReset().mockResolvedValue(ok(versionDetail));
  useCitationDisplayStore.setState({ visible: true });
  useReaderStore.setState({ session: null });
});

afterEach(() => {
  vi.unstubAllGlobals();
  useCitationDisplayStore.setState({ visible: true });
  useReaderStore.setState({ session: null });
});

describe('sourceClosedReason', () => {
  it('says sources are closed when opening is not allowed, even while busy', () => {
    expect(sourceClosedReason(false, false)).toBe('Sources are closed with No help.');
    expect(sourceClosedReason(false, true)).toBe('Sources are closed with No help.');
  });

  it('says another action is in progress when busy', () => {
    expect(sourceClosedReason(true, true)).toBe('Another workbench action is in progress.');
  });

  it('gives no reason when sources can be opened', () => {
    expect(sourceClosedReason(true, false)).toBeUndefined();
  });
});

describe('PracticeCitations', () => {
  const citations = [{ sourceId: 'source-1', versionId: 'version-1', quote: 'careful observation' }];
  const openChip = () => fireEvent.click(screen.getByRole('button', { name: /Citation 1/ }));

  it('lists each saved source cited with its chip, quote and title', async () => {
    render(<PracticeCitations programId="program-1" citations={citations} onOpen={vi.fn()} />, { wrapper });

    const list = screen.getByRole('list', { name: 'Saved sources cited' });
    const item = within(list).getAllByRole('listitem')[0]!;
    expect(within(item).getByRole('button', { name: /Citation 1/ })).toBeInTheDocument();
    expect(within(item).getByText('careful observation')).toBeInTheDocument();
    await waitFor(() => expect(within(item).getByText(/Field notes/)).toBeInTheDocument());
  });

  it('opens with the original citation object', async () => {
    const onOpen = vi.fn();
    render(<PracticeCitations programId="program-1" citations={citations} onOpen={onOpen} />, { wrapper });
    openChip();

    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));

    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen.mock.calls[0]![0]).toBe(citations[0]);
  });

  it('replaces "View source" with the reason when sources cannot be opened', async () => {
    const onOpen = vi.fn();
    render(<PracticeCitations programId="program-1" citations={citations} onOpen={onOpen} disabledReason="Sources are closed with No help." />, { wrapper });
    openChip();

    expect(await screen.findByText('Sources are closed with No help.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument();
    expect(onOpen).not.toHaveBeenCalled();
  });

  it('renders nothing in reading mode', () => {
    useCitationDisplayStore.setState({ visible: false });
    render(<PracticeCitations programId="program-1" citations={citations} onOpen={vi.fn()} />, { wrapper });

    expect(screen.queryByRole('list', { name: 'Saved sources cited' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Citation 1/ })).not.toBeInTheDocument();
  });
});

describe('usePracticeSourceReader', () => {
  type Selected = { sourceId: string; versionId: string; quote?: string | null } | null;
  const settle = () => act(async () => { await new Promise((resolve) => setTimeout(resolve, 30)); });

  it('opens the selected version in the docked reader for the owner', async () => {
    const selected = { sourceId: 'source-1', versionId: 'version-1', quote: 'careful observation' };
    renderHook(() => usePracticeSourceReader('program-1', 'attempt-1', selected), { wrapper });

    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());
    const session = useReaderStore.getState().session!;
    expect(session.ownerKey).toBe('attempt-1');
    expect(session.citations).toHaveLength(1);
    expect(getVersion).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-1', versionId: 'version-1' });
  });

  it('is a citation when the selection carries a quote', async () => {
    const selected = { sourceId: 'source-1', versionId: 'version-1', quote: 'careful observation' };
    renderHook(() => usePracticeSourceReader('program-1', 'attempt-1', selected), { wrapper });

    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());
    expect(useReaderStore.getState().session!.citing).toBe(true);
  });

  it('is read for itself when the selection has no quote', async () => {
    const selected = { sourceId: 'source-1', versionId: 'version-1' };
    renderHook(() => usePracticeSourceReader('program-1', 'attempt-1', selected), { wrapper });

    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());
    expect(useReaderStore.getState().session!.citing).toBe(false);
  });

  it('does nothing without a selection', async () => {
    renderHook(() => usePracticeSourceReader('program-1', 'attempt-1', null), { wrapper });
    await settle();

    expect(useReaderStore.getState().session).toBeNull();
    expect(getVersion).not.toHaveBeenCalled();
  });

  it('does not re-open a closed source when only the owner key changes', async () => {
    const selected = { sourceId: 'source-1', versionId: 'version-1' };
    const { rerender } = renderHook(
      ({ ownerKey }: { ownerKey: string }) => usePracticeSourceReader('program-1', ownerKey, selected),
      { wrapper, initialProps: { ownerKey: 'attempt-1' } },
    );
    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());

    act(() => useReaderStore.getState().close());
    rerender({ ownerKey: 'attempt-2' });
    await settle();

    expect(useReaderStore.getState().session).toBeNull();
  });

  it('re-opens for a new selection object with the same content', async () => {
    const { rerender } = renderHook(
      ({ selected }: { selected: Selected }) => usePracticeSourceReader('program-1', 'attempt-1', selected),
      { wrapper, initialProps: { selected: { sourceId: 'source-1', versionId: 'version-1' } as Selected } },
    );
    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());

    act(() => useReaderStore.getState().close());
    rerender({ selected: { sourceId: 'source-1', versionId: 'version-1' } });

    await waitFor(() => expect(useReaderStore.getState().session?.ownerKey).toBe('attempt-1'));
  });

  it('releases the owner reader on unmount', async () => {
    const selected = { sourceId: 'source-1', versionId: 'version-1' };
    const { unmount } = renderHook(() => usePracticeSourceReader('program-1', 'attempt-1', selected), { wrapper });
    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());

    unmount();

    expect(useReaderStore.getState().session).toBeNull();
  });
});
