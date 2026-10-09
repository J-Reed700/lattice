import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { learningLessonEvidenceKey, learningOutlineEvidenceKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type { LearningLessonEvidenceDto, LearningOutlineEvidenceDto } from '@/lib/bindings';

export type OutlineEvidenceQuery = UseQueryResult<LearningOutlineEvidenceDto | null, Error>;
export type LessonEvidenceQuery = UseQueryResult<LearningLessonEvidenceDto | null, Error>;

export function useLearningOutlineEvidence(programId: string, programRevision: number, enabled = true): OutlineEvidenceQuery {
  return useQuery({
    queryKey: learningOutlineEvidenceKey(programId, programRevision),
    queryFn: async () => {
      const result = await VaultAPI.getLearningOutlineEvidence(programId);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled: enabled && Boolean(programId),
    retry: false,
    staleTime: 30_000,
  });
}

export function useLearningLessonEvidence(programId: string, lessonId: string, programRevision: number, enabled = true): LessonEvidenceQuery {
  return useQuery({
    queryKey: learningLessonEvidenceKey(programId, lessonId, programRevision),
    queryFn: async () => {
      const result = await VaultAPI.getLearningLessonEvidence(programId, lessonId);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled: enabled && Boolean(programId && lessonId),
    retry: false,
    staleTime: 30_000,
  });
}
