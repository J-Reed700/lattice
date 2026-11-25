import { create } from 'zustand';

import { invalidateLearningSourceDependents } from '@/features/learning/api/learningInvalidation';
import { learningClaimReviewKey, learningLessonEvidencePrefix } from '@/features/learning/api/learningQueryKeys';
import { failureLine } from '@/features/learning/lessons/review/claimReviewCopy';
import { learningSourcesKey } from '@/features/learning/sources/useLearningSources';
import { type ApiClient, isConflict } from '@/lib/api';
import type {
  AttachEvidenceDto,
  AttachScopeDto,
  ClaimAttachProgressDto,
  ClaimAttachReportDto,
  LearningSourceWorkspaceDto,
  WaitingClaimsRecheckDto,
} from '@/lib/bindings';
import { failureToError } from '@/lib/errorHandling';
import { getErrorMessage } from '@/lib/errorUtils';

import type { QueryClient } from '@tanstack/react-query';

export type SourceRetryOperation = {
  kind: string;
  fingerprint: string;
  request: Record<string, unknown>;
};

export interface SourceProgramRunState {
  busy: boolean;
  checkWaiting: boolean;
  recheckProgress: ClaimAttachProgressDto | null;
  recheck: WaitingClaimsRecheckDto | null;
  error: string | null;
  conflict: boolean;
  retryKind: string | null;
  retryItem: SourceRetryOperation | null;
  dismissed: boolean;
}

export const EMPTY_SOURCE_PROGRAM_RUN: SourceProgramRunState = {
  busy: false,
  checkWaiting: true,
  recheckProgress: null,
  recheck: null,
  error: null,
  conflict: false,
  retryKind: null,
  retryItem: null,
  dismissed: false,
};

export type AttachSourceVariables = {
  expectedRevision: number;
  evidence: AttachEvidenceDto;
} & (
  | {
    claimId: string;
    scope: AttachScopeDto;
  }
  | { lesson: true }
);

export interface LessonAttachRunState {
  token: number;
  isPending: boolean;
  backendPending: boolean;
  progress: ClaimAttachProgressDto | null;
  error: string | null;
  completedBanner: string | null;
}

export const EMPTY_LESSON_ATTACH_RUN: LessonAttachRunState = {
  token: 0,
  isPending: false,
  backendPending: false,
  progress: null,
  error: null,
  completedBanner: null,
};

export const lessonAttachRunKey = (programId: string, lessonId: string) =>
  `${programId}:${lessonId}`;

interface SourceRecheckStore {
  programRuns: Record<string, SourceProgramRunState>;
  attachRuns: Record<string, LessonAttachRunState>;

  setCheckWaiting: (programId: string, checkWaiting: boolean) => void;
  setRecheckProgress: (programId: string, progress: ClaimAttachProgressDto | null) => void;
  setDismissed: (programId: string, dismissed: boolean) => void;
  runProgramOperation: (
    programId: string,
    client: QueryClient,
    item: SourceRetryOperation,
    execute: (
      request: Record<string, unknown>,
      onProgress: (update: ClaimAttachProgressDto) => void,
    ) => Promise<LearningSourceWorkspaceDto>,
  ) => Promise<boolean>;
  retryProgramOperation: (
    programId: string,
    client: QueryClient,
    execute: (
      request: Record<string, unknown>,
      onProgress: (update: ClaimAttachProgressDto) => void,
    ) => Promise<LearningSourceWorkspaceDto>,
  ) => Promise<boolean>;

  runLessonAttach: (
    api: ApiClient,
    client: QueryClient,
    programId: string,
    lessonId: string,
    variables: AttachSourceVariables,
    describeBanner?: (report: ClaimAttachReportDto) => string | null,
  ) => Promise<ClaimAttachReportDto | null>;
  cancelLessonAttach: (programId: string, lessonId: string) => void;
  clearLessonAttachBanner: (programId: string, lessonId: string) => void;
}

export const useSourceRecheckStore = create<SourceRecheckStore>((set, get) => {
  const programState = (programId: string) => get().programRuns[programId] ?? EMPTY_SOURCE_PROGRAM_RUN;
  const patchProgram = (programId: string, patch: Partial<SourceProgramRunState>) =>
    set((state) => ({
      programRuns: {
        ...state.programRuns,
        [programId]: { ...(state.programRuns[programId] ?? EMPTY_SOURCE_PROGRAM_RUN), ...patch },
      },
    }));

  const attachState = (key: string) => get().attachRuns[key] ?? EMPTY_LESSON_ATTACH_RUN;
  const patchAttach = (key: string, patch: Partial<LessonAttachRunState>) =>
    set((state) => ({
      attachRuns: {
        ...state.attachRuns,
        [key]: { ...(state.attachRuns[key] ?? EMPTY_LESSON_ATTACH_RUN), ...patch },
      },
    }));

  return {
    programRuns: {},
    attachRuns: {},

    setCheckWaiting: (programId, checkWaiting) => patchProgram(programId, { checkWaiting }),
    setRecheckProgress: (programId, recheckProgress) => patchProgram(programId, { recheckProgress }),
    setDismissed: (programId, dismissed) => patchProgram(programId, { dismissed }),

    runProgramOperation: async (programId, client, item, execute) => {
      if (programState(programId).busy) return false;
      patchProgram(programId, {
        busy: true,
        error: null,
        conflict: false,
        dismissed: false,
        retryKind: item.kind,
        retryItem: item,
        recheckProgress: null,
      });

      const onProgress = (update: ClaimAttachProgressDto) => {
        patchProgram(programId, { recheckProgress: update });
      };

      try {
        const workspace = await execute(item.request, onProgress);
        client.setQueryData(learningSourcesKey(workspace.programId), workspace);
        await invalidateLearningSourceDependents(client, workspace.programId);
        patchProgram(programId, {
          busy: false,
          recheckProgress: null,
          retryItem: null,
          ...(item.kind.startsWith('add-') ? { recheck: workspace.recheck } : {}),
        });
        return true;
      } catch (cause) {
        const conflict = isConflict(cause);
        patchProgram(programId, {
          busy: false,
          recheckProgress: null,
          error: getErrorMessage(cause),
          conflict,
          retryItem: conflict ? null : item,
        });
        return false;
      }
    },

    retryProgramOperation: async (programId, client, execute) => {
      const item = programState(programId).retryItem;
      if (!item) return false;
      return get().runProgramOperation(programId, client, item, execute);
    },

    runLessonAttach: async (api, client, programId, lessonId, variables, describeBanner) => {
      const key = lessonAttachRunKey(programId, lessonId);
      const current = attachState(key);
      if (current.isPending || current.backendPending) return null;

      const token = current.token + 1;
      patchAttach(key, {
        token,
        isPending: true,
        backendPending: true,
        progress: null,
        error: null,
        completedBanner: null,
      });

      const { expectedRevision, evidence } = variables;
      const onProgress = (update: ClaimAttachProgressDto) => {
        if (attachState(key).token === token) {
          patchAttach(key, { progress: update });
        }
      };

      try {
        const result = 'lesson' in variables
          ? await api.attachLearningLessonSource({ programId, lessonId, expectedRevision, evidence }, onProgress)
          : await api.attachLearningClaimEvidence(
              { programId, lessonId, claimId: variables.claimId, expectedRevision, evidence, scope: variables.scope },
              onProgress,
            );
        if (attachState(key).token !== token) return null;
        if (!result.ok) {
          const err = failureToError(result);
          patchAttach(key, { isPending: false, progress: null, error: err.message });
          throw err;
        }
        const report = result.data;
        if (report.failure || !report.source) {
          const host = evidence.kind === 'attach_link'
            ? (() => { try { return new URL(evidence.url).hostname.replace(/^www\./, ''); } catch { return null; } })()
            : null;
          const attachError = report.failure
            ? failureLine(report.failure.kind, host, report.failure.message)
            : 'The source could not be added.';
          patchAttach(key, {
            isPending: false,
            progress: null,
            error: attachError,
            completedBanner: null,
          });
          return report;
        }
        const banner = describeBanner ? describeBanner(report) : null;
        patchAttach(key, {
          isPending: false,
          progress: null,
          error: null,
          completedBanner: banner,
        });
        return report;
      } catch (cause) {
        if (attachState(key).token === token) {
          patchAttach(key, {
            isPending: false,
            progress: null,
            error: cause instanceof Error ? cause.message : 'The source could not be added.',
          });
        }
        throw cause;
      } finally {
        patchAttach(key, { backendPending: false });
        await Promise.all([
          client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
          client.invalidateQueries({ queryKey: learningLessonEvidencePrefix(programId) }),
          client.invalidateQueries({ queryKey: learningSourcesKey(programId) }),
          invalidateLearningSourceDependents(client, programId),
        ]);
      }
    },

    cancelLessonAttach: (programId, lessonId) => {
      const key = lessonAttachRunKey(programId, lessonId);
      const nextToken = attachState(key).token + 1;
      patchAttach(key, {
        token: nextToken,
        isPending: false,
        progress: null,
        error: null,
      });
    },

    clearLessonAttachBanner: (programId, lessonId) => {
      const key = lessonAttachRunKey(programId, lessonId);
      patchAttach(key, { completedBanner: null, error: null });
    },
  };
});

export function resetSourceRecheckStore() {
  useSourceRecheckStore.setState({ programRuns: {}, attachRuns: {} });
}
