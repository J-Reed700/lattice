import { useCallback, useMemo } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { formatSourceForReader } from '@/features/learning/sources/sourceReaderFormat';
import {
  learningSourcesKey,
  learningSourceVersionQuery,
  useLearningSourceWorkspace,
} from '@/features/learning/sources/useLearningSources';
import { fromLearningSourceVersion } from '@/features/reading/model/sourceAdapter';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningSourceLibraryItemDto, LearningSourceWorkspaceDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import { unwrapApiResult as unwrap } from '@/types/api/result';
import type { SourceWithMetadata } from '@/types/conversation';

/** One saved source version to read, and the words of it a citation quotes. */
export interface SavedSourceTarget {
  versionId: string;
  /** Looked up from the course's library when the caller only knows the version. */
  sourceId?: string | null;
  quote?: string | null;
}

export interface SavedSourceOpenOptions {
  /** Which target the reader shows first; the rest are a step away. */
  index?: number;
  /** False for a source opened to be read for itself rather than from a citation. */
  citing?: boolean;
}

/** Counts opens across every instance, so the latest click is the one that shows. */
let latestOpen = 0;

function sourceIdOf(sources: LearningSourceLibraryItemDto[], versionId: string): string | null {
  return sources.find((source) => source.versions.some((version) => version.id === versionId))?.id ?? null;
}

/**
 * Open a course's saved source versions in the docked source reader, the one
 * reader chat answers and lessons open their citations in.
 *
 * Every Learning Studio surface that shows a saved source (practice feedback,
 * the practice rail, quiz rationales, the Sources library) reads it here, so a
 * saved source looks and behaves the same wherever it is opened.
 */
export function useSavedSourceReader(programId: string) {
  const api = useApi();
  const client = useQueryClient();
  const workspace = useLearningSourceWorkspace(programId);
  const openReader = useReaderStore((state) => state.open);
  const library = workspace.data?.sources;

  const versionTitles = useMemo(() => {
    const titles = new Map<string, string>();
    for (const source of library ?? []) {
      for (const version of source.versions) titles.set(version.id, version.title);
    }
    return titles;
  }, [library]);

  const titleOf = useCallback((versionId: string | null | undefined) => (versionId ? versionTitles.get(versionId) ?? null : null), [versionTitles]);

  const open = useCallback(async (ownerKey: string, targets: SavedSourceTarget[], options: SavedSourceOpenOptions = {}) => {
    if (targets.length === 0) return;
    latestOpen += 1;
    const ticket = latestOpen;
    const sources = targets.some((target) => !target.sourceId)
      ? (await client.fetchQuery<LearningSourceWorkspaceDto>({
          queryKey: learningSourcesKey(programId),
          queryFn: async () => unwrap(await api.getLearningSourceWorkspace(programId)),
          staleTime: 5_000,
        }).catch(() => null))?.sources ?? []
      : [];

    const read = await Promise.allSettled(targets.map(async (target, index) => {
      const sourceId = target.sourceId || sourceIdOf(sources, target.versionId);
      if (!sourceId) throw new Error('This source is no longer in the course library.');
      const detail = await client.fetchQuery({
        ...learningSourceVersionQuery(api, { programId, sourceId, versionId: target.versionId }),
        staleTime: 60_000,
      });
      return fromLearningSourceVersion(detail, {
        quote: target.quote,
        number: index + 1,
        readerText: formatSourceForReader(detail.fullText, detail.version.extractionVersion, { escapeMarkup: false }),
      }).source;
    }));

    // A newer open, from any list, supersedes this one while it was fetching.
    if (ticket !== latestOpen) return;

    const wanted = options.index ?? 0;
    const opened: SourceWithMetadata[] = [];
    let index = 0;
    read.forEach((result, position) => {
      if (result.status !== 'fulfilled') return;
      if (position <= wanted) index = opened.length;
      opened.push(result.value);
    });
    // Only the source the person asked for earns a toast; the others drop out
    // of the reader's list and keep the numbers of the original targets.
    const failure = read[wanted];
    if (failure?.status === 'rejected') {
      toast.error('The saved source could not be opened', {
        message: failure.reason instanceof Error ? failure.reason.message : String(failure.reason),
      });
      return;
    }
    if (opened.length === 0) return;
    openReader(ownerKey, opened, index, null, { citing: options.citing ?? true });
  }, [api, client, openReader, programId]);

  return { open, titleOf };
}
