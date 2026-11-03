import { useMemo } from 'react';

import { useIndexingStatusQuery } from '@/features/files/hooks/useIndexingStatusQuery';
import { useFlashcardDecks } from '@/features/learning/recall/flashcards/useFlashcards';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import type { ActivitySnapshotDto } from '@/lib/bindings';

import { readIris, type IrisReading } from '../model/irisReading';
import { useIrisStore } from '../stores/irisStore';

/** What `useActivitySnapshot` hands over. */
interface ActivityRead {
  snapshot: ActivitySnapshotDto | undefined;
  failed: boolean;
}

/**
 * Everything Iris reads, from the queries that already hold it. The activity
 * snapshot comes from the caller, which owns its event subscription.
 */
export function useIrisReading({ snapshot: activity, failed: activityFailed }: ActivityRead): IrisReading {
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
    () => readIris({ activity, activityFailed, indexingFailures, failedDownloads, dueCards, seenNoteIds }),
    [activity, activityFailed, indexingFailures, failedDownloads, dueCards, seenNoteIds],
  );
}
