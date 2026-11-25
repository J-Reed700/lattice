import { useRef } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import {
  type AttachSourceVariables,
  EMPTY_LESSON_ATTACH_RUN,
  lessonAttachRunKey,
  useSourceRecheckStore,
} from '@/features/learning/sources/sourceRecheckStore';
import type { ClaimAttachReportDto } from '@/lib/bindings';

export type AddSourceVariables = AttachSourceVariables;

/**
 * "Add a source" over `attach_learning_claim_evidence`, or over
 * `attach_learning_lesson_source` for the whole lesson (§3.6, §5.3): the
 * stage it is at, live, and the report when it ends. Persists across unmount
 * so route/tab leaves keep progress and results; explicit Cancel discards it.
 */
export function useAddSource(
  programId: string,
  lessonId: string,
  describeBanner?: (report: ClaimAttachReportDto) => string | null,
) {
  const api = useApi();
  const client = useQueryClient();
  const key = lessonAttachRunKey(programId, lessonId);
  const state = useSourceRecheckStore((store) => store.attachRuns[key] ?? EMPTY_LESSON_ATTACH_RUN);
  const runLessonAttach = useSourceRecheckStore((store) => store.runLessonAttach);
  const cancelLessonAttach = useSourceRecheckStore((store) => store.cancelLessonAttach);
  const describeRef = useRef(describeBanner);
  describeRef.current = describeBanner;

  return {
    isPending: state.isPending,
    backendPending: state.backendPending,
    progress: state.progress,
    mutateAsync: (variables: AddSourceVariables) =>
      runLessonAttach(api, client, programId, lessonId, variables, (report) =>
        describeRef.current ? describeRef.current(report) : null,
      ),
    cancel: () => cancelLessonAttach(programId, lessonId),
  };
}
