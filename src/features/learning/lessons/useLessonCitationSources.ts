import { useMemo } from 'react';

import { useQueries } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningSourceVersionQuery } from '@/features/learning/sources/useLearningSources';
import { fromLearningEvidencePassage } from '@/features/reading/model/sourceAdapter';
import type { LearningLessonEvidenceDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';

/** The sources a lesson's citations open, numbered as its chips are, each with the saved page its passage came from once that has loaded. */
export function useLessonCitationSources(
  programId: string,
  report: LearningLessonEvidenceDto | null | undefined,
  enabled: boolean,
): SourceWithMetadata[] {
  const api = useApi();
  const citations = report?.citations;
  const versions = useMemo(() => {
    const distinct = new Map<string, { programId: string; sourceId: string; versionId: string }>();
    for (const citation of citations ?? []) {
      if (!citation.sourceId) continue;
      const versionId = citation.passage.sourceVersionId;
      distinct.set(`${citation.sourceId}:${versionId}`, { programId, sourceId: citation.sourceId, versionId });
    }
    return [...distinct.entries()];
  }, [citations, programId]);

  const fullTexts = useQueries({
    queries: versions.map(([, request]) => ({
      ...learningSourceVersionQuery(api, request),
      enabled,
      staleTime: Infinity,
      retry: false,
    })),
    combine: (results) => results.map((result) => result.data?.fullText),
  });

  return useMemo(() => {
    const textOf = new Map(versions.map(([key], index) => [key, fullTexts[index]]));
    return (citations ?? []).map((citation) => {
      const text = citation.sourceId ? textOf.get(`${citation.sourceId}:${citation.passage.sourceVersionId}`) : undefined;
      return fromLearningEvidencePassage(citation.passage, citation.number - 1, text).source;
    });
  }, [citations, versions, fullTexts]);
}
