import { create } from 'zustand';

import {
  learningClaimReviewKey,
  learningLessonAuditKey,
  learningPlanKey,
  learningProgramKey,
  learningVerifierAuditKey,
} from '@/features/learning/api/learningQueryKeys';
import type { ApiClient } from '@/lib/api';
import type { LearningLessonAuditResultDto } from '@/lib/bindings';

import type { QueryClient } from '@tanstack/react-query';

export interface LessonAuditRunEntry {
  isPending: boolean;
  data: LearningLessonAuditResultDto | null;
  error: Error | null;
}

export const EMPTY_LESSON_AUDIT_RUN: LessonAuditRunEntry = {
  isPending: false,
  data: null,
  error: null,
};

export const lessonAuditRunKey = (programId: string, lessonId: string) =>
  `${programId}:${lessonId}`;

interface LessonAuditStore {
  entries: Record<string, LessonAuditRunEntry>;
  runAudit: (
    api: ApiClient,
    client: QueryClient,
    programId: string,
    lessonId: string,
  ) => Promise<LearningLessonAuditResultDto | null>;
  resetAudit: (programId: string, lessonId: string) => void;
}

export const useLessonAuditStore = create<LessonAuditStore>((set, get) => ({
  entries: {},

  runAudit: async (api, client, programId, lessonId) => {
    const key = lessonAuditRunKey(programId, lessonId);
    if (get().entries[key]?.isPending) return null;

    set((state) => ({
      entries: {
        ...state.entries,
        [key]: { isPending: true, data: null, error: null },
      },
    }));

    try {
      const result = await api.auditLearningLessonNow(programId, lessonId, crypto.randomUUID());
      if (!result.ok) {
        throw new Error(result.error);
      }
      set((state) => ({
        entries: {
          ...state.entries,
          [key]: { isPending: false, data: result.data, error: null },
        },
      }));
      return result.data;
    } catch (cause) {
      const error = cause instanceof Error ? cause : new Error('Audit failed.');
      set((state) => ({
        entries: {
          ...state.entries,
          [key]: { isPending: false, data: null, error },
        },
      }));
      return null;
    } finally {
      await Promise.all([
        client.invalidateQueries({ queryKey: learningLessonAuditKey(programId, lessonId) }),
        client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
        client.invalidateQueries({ queryKey: learningVerifierAuditKey(programId) }),
        client.invalidateQueries({ queryKey: learningPlanKey(programId) }),
        client.invalidateQueries({ queryKey: learningProgramKey(programId) }),
      ]);
    }
  },

  resetAudit: (programId, lessonId) => {
    const key = lessonAuditRunKey(programId, lessonId);
    set((state) => {
      const current = state.entries[key];
      if (!current || current.isPending) return state;
      const next = { ...state.entries };
      delete next[key];
      return { entries: next };
    });
  },
}));

export function resetLessonAuditStore() {
  useLessonAuditStore.setState({ entries: {} });
}
