import { useMemo } from 'react';

import { useQuery } from '@tanstack/react-query';

import { activitySnapshotQueryOptions } from '@/features/activity/api';
import { useIndexingStatusQuery } from '@/features/files/hooks/useIndexingStatusQuery';
import { useFlashcardDecks } from '@/features/learning/recall/flashcards/useFlashcards';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';

import { readIris, type IrisReading } from '../model/irisReading';
import { useIrisStore } from '../stores/irisStore';

/**
 * Everything Iris reads, from the queries that already hold it. The activity
 * snapshot is kept current by `ActivityIndicator`, which owns its event
 * subscription; Iris only observes the same cache entry.
 */
export function useIrisReading(): IrisReading {
  const activity = useQuery(activitySnapshotQueryOptions()).data;
  const indexing = useIndexingStatusQuery().data;
  const { downloadsMap } = useDownloadState();
  const decks = useFlashcardDecks().data;
  const seenNoteIds = useIrisStore((state) => state.seenNoteIds);

  const failedDownloads = useMemo(
    () =>
      Array.from(downloadsMap.values())
        .filter((download) => download.state === 'Failed')
        .map((download) => ({ id: download.id, name: download.model_name ?? 'model' })),
    [downloadsMap],
  );
  const dueCards = decks ? decks.reduce((sum, deck) => sum + deck.dueCount, 0) : null;
  const indexingFailures = indexing?.failures.length ?? 0;

  return useMemo(
    () => readIris({ activity, indexingFailures, failedDownloads, dueCards, seenNoteIds }),
    [activity, indexingFailures, failedDownloads, dueCards, seenNoteIds],
  );
}
