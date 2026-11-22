import { create } from 'zustand';


import { learningPlanKey } from '@/features/learning/api/learningQueryKeys';
import { learningQuizOverviewKey } from '@/features/learning/quizzes/quizKeys';
import type { ApiClient } from '@/lib/api';
import type { LearningQuizScopeDto, LearningQuizStatusDto } from '@/lib/bindings';
import { unwrapApiResult as unwrap } from '@/types/api/result';

import type { QueryClient } from '@tanstack/react-query';

export const scopeKey = (scope: LearningQuizScopeDto) => `${scope.kind}:${scope.id ?? 'course'}`;

const errorMessage = (error: unknown) => (error instanceof Error ? error.message : 'That did not work. Try again.');

function newOperationId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `op-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export interface ProgramQuizLauncherState {
  busyScopes: Record<string, boolean>;
  errorsByScope: Record<string, string>;
  preparingAll: boolean;
  operationIds: Record<string, string>;
}

export const EMPTY_PROGRAM_QUIZ_LAUNCHER: ProgramQuizLauncherState = {
  busyScopes: {},
  errorsByScope: {},
  preparingAll: false,
  operationIds: {},
};

interface QuizLauncherStore {
  programs: Record<string, ProgramQuizLauncherState>;
  takeOperationId: (programId: string, key: string) => string;
  clearOperationId: (programId: string, key: string) => void;
  runScope: (programId: string, status: LearningQuizStatusDto, work: () => Promise<void>) => Promise<void>;
  prepareAll: (
    programId: string,
    statuses: LearningQuizStatusDto[],
    api: Pick<ApiClient, 'prepareLearningQuizBank'>,
    client: QueryClient,
  ) => Promise<void>;
}

const initialState = () => ({ programs: {} as Record<string, ProgramQuizLauncherState> });

/** Persists per-quiz launch/prepare busy flags, errors, and Prepare-all loops across tab switches. */
export const useQuizLauncherStore = create<QuizLauncherStore>((set, get) => {
  const entryOf = (programId: string): ProgramQuizLauncherState =>
    get().programs[programId] ?? EMPTY_PROGRAM_QUIZ_LAUNCHER;

  const patchProgram = (
    programId: string,
    updater: (current: ProgramQuizLauncherState) => ProgramQuizLauncherState,
  ) => {
    set((state) => ({
      programs: {
        ...state.programs,
        [programId]: updater(state.programs[programId] ?? EMPTY_PROGRAM_QUIZ_LAUNCHER),
      },
    }));
  };

  return {
    ...initialState(),

    takeOperationId: (programId, key) => {
      const existing = entryOf(programId).operationIds[key];
      if (existing) return existing;
      const created = newOperationId();
      patchProgram(programId, (current) => ({
        ...current,
        operationIds: { ...current.operationIds, [key]: created },
      }));
      return created;
    },

    clearOperationId: (programId, key) => {
      if (!(key in entryOf(programId).operationIds)) return;
      patchProgram(programId, (current) => {
        const next = { ...current.operationIds };
        delete next[key];
        return { ...current, operationIds: next };
      });
    },

    runScope: async (programId, status, work) => {
      const key = scopeKey(status.scope);
      if (entryOf(programId).busyScopes[key]) return;
      patchProgram(programId, (current) => {
        const nextErrors = { ...current.errorsByScope };
        delete nextErrors[key];
        return {
          ...current,
          busyScopes: { ...current.busyScopes, [key]: true },
          errorsByScope: nextErrors,
        };
      });
      try {
        await work();
      } catch (cause) {
        patchProgram(programId, (current) => ({
          ...current,
          errorsByScope: { ...current.errorsByScope, [key]: errorMessage(cause) },
        }));
      } finally {
        patchProgram(programId, (current) => {
          const nextBusy = { ...current.busyScopes };
          delete nextBusy[key];
          return { ...current, busyScopes: nextBusy };
        });
      }
    },

    prepareAll: async (programId, statuses, api, client) => {
      if (entryOf(programId).preparingAll) return;
      patchProgram(programId, (current) => ({ ...current, preparingAll: true }));
      try {
        for (const status of statuses) {
          const scopeId = scopeKey(status.scope);
          const opKey = `prepare:${scopeId}:false`;
          await get().runScope(programId, status, async () => {
            const operationId = get().takeOperationId(programId, opKey);
            unwrap(await api.prepareLearningQuizBank({
              operationId,
              programId,
              scope: status.scope,
              topUp: false,
            }));
            get().clearOperationId(programId, opKey);
            await Promise.all([
              client.invalidateQueries({ queryKey: learningQuizOverviewKey(programId) }),
              client.invalidateQueries({ queryKey: learningPlanKey(programId) }),
            ]);
          });
        }
      } finally {
        patchProgram(programId, (current) => ({ ...current, preparingAll: false }));
      }
    },
  };
});

export function resetQuizLauncherStore(): void {
  useQuizLauncherStore.setState(initialState());
}
