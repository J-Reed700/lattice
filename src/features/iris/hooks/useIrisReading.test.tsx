import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ActivitySnapshotDto } from '@/lib/bindings';

import { useIrisReading } from './useIrisReading';

const mocks = vi.hoisted(() => ({
  indexing: undefined as { failures: unknown[] } | undefined,
  downloads: new Map<string, { id: string; state: string; model_name: string | null }>(),
  decks: undefined as { dueCount: number }[] | undefined,
}));

vi.mock('@/features/files/hooks/useIndexingStatusQuery', () => ({
  useIndexingStatusQuery: () => ({ data: mocks.indexing }),
}));
vi.mock('@/features/learning/recall/flashcards/useFlashcards', () => ({
  useFlashcardDecks: () => ({ data: mocks.decks }),
}));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({ downloadsMap: mocks.downloads }),
}));

beforeEach(() => {
  mocks.indexing = undefined;
  mocks.downloads = new Map();
  mocks.decks = undefined;
});

describe('useIrisReading', () => {
  it('reads the snapshot it is given together with the index, the downloads and the decks', () => {
    mocks.indexing = { failures: [{}, {}] };
    mocks.downloads = new Map([
      ['d1', { id: 'd1', state: 'Failed', model_name: 'MiniLM' }],
      ['d2', { id: 'd2', state: 'Completed', model_name: 'Qwen3-8B' }],
    ]);
    mocks.decks = [{ dueCount: 2 }, { dueCount: 3 }];
    const snapshot: ActivitySnapshotDto = { items: [], running: 0 };

    const { result } = renderHook(() => useIrisReading({ snapshot, failed: false }));

    expect(result.current.notes.map((note) => note.text)).toEqual([
      'The MiniLM download failed.',
      '2 files couldn\'t be indexed.',
      '5 flashcards are due for review.',
    ]);
    expect(result.current.activityFailed).toBe(false);
  });

  it('passes on that the snapshot could not be read', () => {
    const { result } = renderHook(() => useIrisReading({ snapshot: undefined, failed: true }));
    expect(result.current).toMatchObject({ activityFailed: true, headline: 'Could not read what is running.' });
  });
});
