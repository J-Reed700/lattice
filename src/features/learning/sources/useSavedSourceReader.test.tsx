import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { useSavedSourceReader } from '@/features/learning/sources/useSavedSourceReader';
import { headingTrail, placeAnchors } from '@/features/reading/citations';
import { articleBlocks } from '@/features/reading/model/articleBlocks';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import { useToastStore } from '@/stores/toastStore';

const mocks = vi.hoisted(() => ({ workspace: vi.fn(), version: vi.fn() }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });

function versionSummary(id: string, title: string) {
  return {
    id, versionNumber: 1, title, publisher: null, resolvedUrl: null, excerpt: '', contentSha256: `sha-${id}`,
    wordCount: 3, truncated: false, extractionVersion: 'reader-1', acquiredAt: 1_790_000_000_000, snapshotHash: null,
  };
}
const libraryItem = (id: string, versions: Array<{ id: string; title: string }>) => ({
  id, kind: 'text', versions: versions.map((v) => versionSummary(v.id, v.title)),
});
const library = {
  programId: 'program-1',
  sources: [
    libraryItem('source-a', [{ id: 'va', title: 'Alpha notes' }]),
    libraryItem('source-b', [{ id: 'vb', title: 'Beta notes' }]),
    libraryItem('source-c', [{ id: 'vc', title: 'Gamma notes' }]),
  ],
};
const detailFor = (sourceId: string, versionId: string, title: string, fullText: string) => ({
  sourceId, version: versionSummary(versionId, title), fullText, usage: [],
});

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const api = { getLearningSourceWorkspace: mocks.workspace, getLearningSourceVersion: mocks.version } as unknown as ApiClient;
  return (
    <QueryClientProvider client={client}>
      <ApiProvider client={api}>{children}</ApiProvider>
    </QueryClientProvider>
  );
}

const toasts = () => useToastStore.getState().toasts;

beforeEach(() => {
  vi.resetAllMocks();
  useReaderStore.setState({ session: null });
  useToastStore.getState().dismissAll();
  mocks.workspace.mockResolvedValue(ok(library));
  mocks.version.mockImplementation(async (request: { sourceId: string; versionId: string }) => {
    const title = { va: 'Alpha notes', vb: 'Beta notes', vc: 'Gamma notes' }[request.versionId] ?? 'Unknown';
    return ok(detailFor(request.sourceId, request.versionId, title, `# ${title}\n\nFull text of ${request.versionId}.`));
  });
});

describe('useSavedSourceReader', () => {
  it('resolves sources from the library, fetches the exact versions and opens the reader', async () => {
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() => result.current.open('owner-1', [
      { versionId: 'va', quote: 'quoted a', number: 4 },
      { versionId: 'vb', quote: 'quoted b', number: 9 },
    ], { index: 1 }));

    expect(mocks.workspace).toHaveBeenCalledWith('program-1');
    expect(mocks.version).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-a', versionId: 'va' });
    expect(mocks.version).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-b', versionId: 'vb' });
    const session = useReaderStore.getState().session!;
    expect(session.ownerKey).toBe('owner-1');
    expect(session.index).toBe(1);
    expect(session.citing).toBe(true);
    expect(session.citations).toHaveLength(2);
    const first = session.citations[0]!;
    expect(first.documentId).toBe('course:va');
    expect(first.content).toBe('quoted a');
    expect(first.citationId).toBe(4);
    expect(first.webSnapshot?.text).toContain('Full text of va.');
    expect(session.citations[1]!.citationId).toBe(9);
    expect(toasts()).toHaveLength(0);
  });

  it('does not fabricate index + 1 citation numbers when number is omitted', async () => {
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() => result.current.open('owner-1', [{ versionId: 'va', sourceId: 'source-a' }], { citing: false }));
    const opened = useReaderStore.getState().session!.citations[0]!;
    expect(opened.citationId).toBeUndefined();
  });

  it('passes raw fullText when support has UTF-8 byte spans so reader placement and headingTrail align', async () => {
    const targetPhrase = 'Gluten forms when flour meets water.';
    const text = `# Chapter One\n\n## Intro\n\n${targetPhrase}\n\n## Details\n\nPréface — café:\n\n${targetPhrase}\n\nMore prose.`;
    const enc = new TextEncoder();
    const secondOffset = text.lastIndexOf(targetPhrase);
    const prefix = text.slice(0, secondOffset);
    const start = enc.encode(prefix).length;
    const end = start + enc.encode(targetPhrase).length;
    mocks.version.mockResolvedValue(ok(detailFor('source-a', 'va', 'Alpha notes', text)));

    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() =>
      result.current.open('owner-1', [
        {
          versionId: 'va',
          sourceId: 'source-a',
          quote: targetPhrase,
          number: 5,
          support: { relation: 'quotes', anchors: [{ exact: targetPhrase, span: { start, end } }] },
        },
      ]),
    );

    const opened = useReaderStore.getState().session!.citations[0]!;
    expect(opened.citationId).toBe(5);
    expect(opened.anchorBase).toBe(0);
    expect(opened.webSnapshot?.text).toBe(text);
    expect(opened.anchoredBy?.[0]?.relation).toBe('quotes');
    const placed = placeAnchors(opened.webSnapshot!.text, opened.anchoredBy![0]!.anchors, opened.anchorBase ?? 0);
    expect(placed.ranges).toHaveLength(1);
    const range = placed.ranges[0]!;
    expect(range.start).toBe(secondOffset);
    expect(opened.webSnapshot!.text.slice(range.start, range.end)).toBe(targetPhrase);
    expect(headingTrail(opened.webSnapshot!.text, articleBlocks(opened.webSnapshot!.text), range.start)).toEqual(['Chapter One', 'Details']);
  });

  it('uses a target\'s own sourceId without the library', async () => {
    // The library knows nothing of this version; only the target's sourceId can find it.
    mocks.workspace.mockResolvedValue(ok({ programId: 'program-1', sources: [] }));
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() => result.current.open('owner-1', [{ versionId: 'va', sourceId: 'source-a' }]));
    expect(mocks.version).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-a', versionId: 'va' });
    expect(useReaderStore.getState().session?.index).toBe(0);
  });

  it('passes citing: false through', async () => {
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() => result.current.open('owner-1', [{ versionId: 'va' }], { citing: false }));
    expect(useReaderStore.getState().session?.citing).toBe(false);
  });

  it('opens without an earlier failed target, remapping the index and keeping citation numbers', async () => {
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() => result.current.open('owner-1', [{ versionId: 'missing', number: 1 }, { versionId: 'vb', number: 2 }, { versionId: 'vc', number: 3 }], { index: 2 }));

    const session = useReaderStore.getState().session!;
    expect(session.citations.map((c) => c.documentId)).toEqual(['course:vb', 'course:vc']);
    expect(session.citations.map((c) => c.citationId)).toEqual([2, 3]);
    expect(session.index).toBe(1);
    expect(toasts()).toHaveLength(0);
  });

  it('toasts and does not open when the wanted source is not in the library', async () => {
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() => result.current.open('owner-1', [{ versionId: 'va' }, { versionId: 'missing' }], { index: 1 }));

    expect(useReaderStore.getState().session).toBeNull();
    expect(toasts()).toHaveLength(1);
    expect(toasts()[0]).toMatchObject({ type: 'error', title: 'The saved source could not be opened', message: 'This source is no longer in the course library.' });
  });

  it('toasts the reason and does not open when the wanted version fetch errors', async () => {
    mocks.version.mockResolvedValue(fail('Version store offline'));
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await act(() => result.current.open('owner-1', [{ versionId: 'va', sourceId: 'source-a' }]));

    expect(useReaderStore.getState().session).toBeNull();
    expect(toasts()[0]).toMatchObject({ type: 'error', title: 'The saved source could not be opened' });
    expect(toasts()[0]!.message).toContain('Version store offline');
  });

  it('lets the latest open win when an earlier one finishes later', async () => {
    let releaseA: (value: unknown) => void = () => {};
    mocks.version.mockImplementation((request: { sourceId: string; versionId: string }) => {
      if (request.versionId === 'va') {
        return new Promise((resolve) => { releaseA = resolve; });
      }
      return Promise.resolve(ok(detailFor(request.sourceId, request.versionId, 'Beta notes', 'Beta text')));
    });
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });

    let openA: Promise<void> = Promise.resolve();
    act(() => { openA = result.current.open('owner-a', [{ versionId: 'va', sourceId: 'source-a' }]); });
    await waitFor(() => expect(mocks.version).toHaveBeenCalledTimes(1));
    await act(() => result.current.open('owner-b', [{ versionId: 'vb', sourceId: 'source-b' }]));
    expect(useReaderStore.getState().session?.ownerKey).toBe('owner-b');

    await act(async () => {
      releaseA(ok(detailFor('source-a', 'va', 'Alpha notes', 'Alpha text')));
      await openA;
    });
    expect(useReaderStore.getState().session?.ownerKey).toBe('owner-b');
    expect(toasts()).toHaveLength(0);
  });

  it('does not toast for a superseded open whose wanted source fails', async () => {
    let releaseA: (value: unknown) => void = () => {};
    mocks.version.mockImplementation((request: { sourceId: string; versionId: string }) => {
      if (request.versionId === 'va') return new Promise((resolve) => { releaseA = resolve; });
      return Promise.resolve(ok(detailFor(request.sourceId, request.versionId, 'Beta notes', 'Beta text')));
    });
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    let openA: Promise<void> = Promise.resolve();
    act(() => { openA = result.current.open('owner-a', [{ versionId: 'va', sourceId: 'source-a' }]); });
    await waitFor(() => expect(mocks.version).toHaveBeenCalledTimes(1));
    await act(() => result.current.open('owner-b', [{ versionId: 'vb', sourceId: 'source-b' }]));
    await act(async () => {
      releaseA(fail('late failure'));
      await openA;
    });
    expect(toasts()).toHaveLength(0);
    expect(useReaderStore.getState().session?.ownerKey).toBe('owner-b');
  });

  it('titleOf returns the library title, and null for unknown or empty ids', async () => {
    const { result } = renderHook(() => useSavedSourceReader('program-1'), { wrapper });
    await waitFor(() => expect(result.current.titleOf('vb')).toBe('Beta notes'));
    expect(result.current.titleOf('va')).toBe('Alpha notes');
    expect(result.current.titleOf('nope')).toBeNull();
    expect(result.current.titleOf('')).toBeNull();
    expect(result.current.titleOf(null)).toBeNull();
  });
});
