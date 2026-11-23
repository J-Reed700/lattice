import { create } from 'zustand';

import { invalidateLearningSourceDependents } from '@/features/learning/api/learningInvalidation';
import { learningPracticalWorkspaceKey } from '@/features/learning/api/learningQueryKeys';
import type { ApiClient } from '@/lib/api';
import { unwrap } from '@/lib/api';
import type {
  GenerateLearningPracticalActivityRequestDto,
  LearningPracticalActivityKind,
  LearningPracticalWorkspaceDto,
} from '@/lib/bindings';

import type { QueryClient } from '@tanstack/react-query';

export type PracticalGenerationStatus = 'running' | 'failed' | 'succeeded';

export interface PracticalGenerationEntry {
  key: string;
  programId: string;
  lessonId: string;
  status: PracticalGenerationStatus;
  kind: LearningPracticalActivityKind;
  role: string | null;
  company: string | null;
  activityId: string;
  error: unknown;
}

export const practicalGenerationKey = (programId: string, lessonId: string) =>
  `${programId}:${lessonId}`;

interface PracticalGenerationStore {
  generations: Record<string, PracticalGenerationEntry>;
  startGeneration: (
    api: ApiClient,
    client: QueryClient,
    request: GenerateLearningPracticalActivityRequestDto,
  ) => Promise<LearningPracticalWorkspaceDto>;
  dismissGeneration: (programId: string, lessonId: string) => void;
}

export const usePracticalGenerationStore = create<PracticalGenerationStore>((set, get) => ({
  generations: {},

  startGeneration: async (api, client, request) => {
    const key = practicalGenerationKey(request.programId, request.lessonId);
    const existing = get().generations[key];
    if (existing?.status === 'running') {
      throw new Error('An activity is already being prepared for this lesson.');
    }

    const entryBase = {
      key,
      programId: request.programId,
      lessonId: request.lessonId,
      kind: request.kind,
      role: request.interviewTarget?.targetRole ?? null,
      company: request.interviewTarget?.companyName ?? null,
      activityId: request.activityId,
    };

    set((state) => ({
      generations: {
        ...state.generations,
        [key]: { ...entryBase, status: 'running', error: null },
      },
    }));

    try {
      const data = unwrap(await api.generateLearningPracticalActivity(request));
      client.setQueryData(learningPracticalWorkspaceKey(request.programId), data);
      void client.invalidateQueries({
        queryKey: learningPracticalWorkspaceKey(request.programId),
        refetchType: 'none',
      });
      set((state) => ({
        generations: {
          ...state.generations,
          [key]: { ...entryBase, status: 'succeeded', error: null },
        },
      }));
      return data;
    } catch (cause) {
      set((state) => ({
        generations: {
          ...state.generations,
          [key]: { ...entryBase, status: 'failed', error: cause },
        },
      }));
      throw cause;
    } finally {
      // Research can save useful evidence even when a later model call fails.
      if (request.kind === 'interview') {
        void invalidateLearningSourceDependents(client, request.programId);
        void client.invalidateQueries({
          queryKey: ['learning-sources', request.programId],
        });
      }
    }
  },

  dismissGeneration: (programId, lessonId) => {
    const key = practicalGenerationKey(programId, lessonId);
    set((state) => {
      if (!(key in state.generations)) return state;
      const next = { ...state.generations };
      delete next[key];
      return { generations: next };
    });
  },
}));

export function resetPracticalGenerationStore() {
  usePracticalGenerationStore.setState({ generations: {} });
}
