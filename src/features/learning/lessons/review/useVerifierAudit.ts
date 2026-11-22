import { useEffect, useRef } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningClaimReviewKey, learningLessonAuditKey, learningPlanKey, learningProgramKey, learningVerifierAuditKey } from '@/features/learning/api/learningQueryKeys';
import {
  EMPTY_LESSON_AUDIT_RUN,
  lessonAuditRunKey,
  useLessonAuditStore,
} from '@/features/learning/lessons/review/lessonAuditStore';
import type { LearningLessonAuditDto, LearningPlanDto, LearningVerifierAuditDto } from '@/lib/bindings';
import type { ApiResult } from '@/types';

function unwrap<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

/** The course's opt-in to the Verifier audit, with what consent it needs (§14.2). */
export function useProgramVerifierAudit(programId: string, enabled = true) {
  const api = useApi();
  return useQuery<LearningVerifierAuditDto>({
    queryKey: learningVerifierAuditKey(programId),
    queryFn: async () => unwrap(await api.getLearningProgramVerifierAudit(programId)),
    enabled: enabled && Boolean(programId),
    retry: false,
  });
}

export function useSetProgramVerifierAudit(programId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<LearningVerifierAuditDto, Error, { on: boolean; webSearch: boolean; consentConfirmed: boolean }>({
    retry: false,
    mutationFn: async ({ on, webSearch, consentConfirmed }) =>
      unwrap(await api.setLearningProgramVerifierAudit(programId, on, webSearch, consentConfirmed)),
    onSuccess: (audit) => client.setQueryData(learningVerifierAuditKey(programId), audit),
  });
}

/** The lesson's latest audit: batches, tokens, the last error. Null before any. */
export function useLessonAudit(programId: string, lessonId: string, enabled = true) {
  const api = useApi();
  const client = useQueryClient();
  const auditKey = lessonAuditRunKey(programId, lessonId);
  const storePending = useLessonAuditStore((store) => store.entries[auditKey]?.isPending ?? false);

  const query = useQuery<LearningLessonAuditDto | null>({
    queryKey: learningLessonAuditKey(programId, lessonId),
    queryFn: async () => unwrap(await api.getLearningLessonAudit(programId, lessonId)),
    enabled: enabled && Boolean(programId && lessonId),
    staleTime: 15_000,
    refetchInterval: (current) => {
      if (current.state.data?.state === 'running' || storePending) return 2_000;
      const plan = client.getQueryData<LearningPlanDto>(learningPlanKey(programId));
      const activeJob = plan?.jobs?.some(
        (job) =>
          (job.status === 'pending' || job.status === 'running') &&
          (Boolean(job.lessonIds?.includes(lessonId)) || job.kind === 'lesson_preparation'),
      );
      return activeJob ? 2_000 : false;
    },
    retry: false,
  });

  const prevStateRef = useRef<LearningLessonAuditDto['state'] | undefined>(undefined);
  const currentState = query.data?.state;
  useEffect(() => {
    if (prevStateRef.current === 'running' && currentState && currentState !== 'running') {
      void Promise.all([
        client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
        client.invalidateQueries({ queryKey: learningVerifierAuditKey(programId) }),
        client.invalidateQueries({ queryKey: learningPlanKey(programId) }),
        client.invalidateQueries({ queryKey: learningProgramKey(programId) }),
      ]);
    }
    prevStateRef.current = currentState;
  }, [client, currentState, lessonId, programId]);

  return query;
}

/** "Audit this lesson now" on a ready lesson; a dispute sends it back to Needs review. */
export function useAuditLessonNow(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  const key = lessonAuditRunKey(programId, lessonId);
  const entry = useLessonAuditStore((store) => store.entries[key] ?? EMPTY_LESSON_AUDIT_RUN);
  const runAudit = useLessonAuditStore((store) => store.runAudit);
  const resetAudit = useLessonAuditStore((store) => store.resetAudit);

  return {
    isPending: entry.isPending,
    data: entry.data ?? undefined,
    error: entry.error,
    mutate: () => {
      void runAudit(api, client, programId, lessonId);
    },
    mutateAsync: () => runAudit(api, client, programId, lessonId),
    reset: () => resetAudit(programId, lessonId),
  };
}
