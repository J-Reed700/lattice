import { create } from 'zustand';

import { onJobStatus } from '@/features/jobs/api';
import { newId } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';
import { apiCall } from '@/shared/ipc/transport';

import { programApi } from './programApi';
import { sessionApi } from '../../session/sessionApi';

import type { CodingBuildChoice } from './CodingEntry';

export const PRACTICE_BUILD_JOB_KIND = 'learning.practice_build';

export type CodingBuildPhase =
  | { kind: 'building' }
  | { kind: 'built' }
  | { kind: 'starting' }
  | { kind: 'ready'; sessionId: string }
  | { kind: 'failed'; message: string; stage: 'build' | 'start' };

export interface CodingBuildHandle {
  programId: string;
  choice: CodingBuildChoice;
  operationId: string;
  startOperationId: string;
  jobId: string | null;
  step: number;
  phase: CodingBuildPhase;
  activityId: string | null;
  seenJobs: Record<string, Wire.JobDto>;
}

export function isCodingBuildSubject(subjectId: string | null | undefined, programId: string): boolean {
  return subjectId === `${programId}:coding` || subjectId === programId;
}

export function isCodingBuildActive(handle: CodingBuildHandle | undefined): boolean {
  return Boolean(handle && (handle.phase.kind === 'building' || handle.phase.kind === 'starting'));
}

export async function listActivePracticeBuildJob(programId: string): Promise<Wire.JobDto | null> {
  try {
    const result = await apiCall('list_jobs', { kinds: [PRACTICE_BUILD_JOB_KIND] });
    if (!result.ok || !Array.isArray(result.data)) return null;
    return (
      result.data.find(
        (job) =>
          isCodingBuildSubject(job.subjectId, programId) &&
          (job.status === 'pending' || job.status === 'running'),
      ) ?? null
    );
  } catch {
    return null;
  }
}

let listenerUnsubscribe: (() => void) | null = null;
const buildRequestTokens = new Map<string, number>();
const inFlightBuildPrograms = new Set<string>();
const startingSessionPrograms = new Set<string>();

function ensureGlobalJobListener() {
  if (listenerUnsubscribe) return;
  try {
    if (typeof onJobStatus !== 'function') return;
    listenerUnsubscribe = onJobStatus((job) => {
      useCodingBuildStore.getState().receiveJobStatus(job);
    });
  } catch {
    // Tests that partially mock '@/features/jobs/api' only provide useJobStatus.
  }
}

interface CodingBuildStore {
  handles: Record<string, CodingBuildHandle>;
  ensureBuild: (programId: string, choice: CodingBuildChoice, forceNew?: boolean) => Promise<void>;
  attachBackendJob: (programId: string, job: Wire.JobDto, fallbackChoice: CodingBuildChoice) => void;
  receiveJobStatus: (job: Wire.JobDto) => void;
  startSessionForBuild: (programId: string) => Promise<string | null>;
  clearHandle: (programId: string) => void;
}

export const useCodingBuildStore = create<CodingBuildStore>((set, get) => {
  const patchHandle = (programId: string, updater: (handle: CodingBuildHandle) => CodingBuildHandle) => {
    set((state) => {
      const current = state.handles[programId];
      if (!current) return state;
      return {
        handles: {
          ...state.handles,
          [programId]: updater(current),
        },
      };
    });
  };

  const applyJobToHandle = (programId: string, job: Wire.JobDto) => {
    const current = get().handles[programId];
    if (!current) return;
    if (current.jobId !== job.id) return;

    if (job.status === 'completed') {
      if (job.resultRef) {
        patchHandle(programId, (handle) => ({
          ...handle,
          step: job.progressCurrent,
          activityId: job.resultRef,
          phase: handle.phase.kind === 'starting' || handle.phase.kind === 'ready' ? handle.phase : { kind: 'built' },
        }));
      } else {
        patchHandle(programId, (handle) => ({
          ...handle,
          step: job.progressCurrent,
          phase: {
            kind: 'failed',
            message: job.error ?? 'The built problem could not be opened.',
            stage: 'build',
          },
        }));
      }
      return;
    }

    if (job.status === 'failed' || job.status === 'cancelled' || job.status === 'interrupted') {
      patchHandle(programId, (handle) => ({
        ...handle,
        step: job.progressCurrent,
        phase: {
          kind: 'failed',
          message: job.error ?? 'The problem could not be built.',
          stage: 'build',
        },
      }));
      return;
    }

    patchHandle(programId, (handle) => ({
      ...handle,
      step: job.progressCurrent,
    }));
  };

  return {
    handles: {},

    attachBackendJob: (programId, job, fallbackChoice) => {
      ensureGlobalJobListener();
      set((state) => {
        const existing = state.handles[programId];
        if (existing?.jobId === job.id) return state;
        return {
          handles: {
            ...state.handles,
            [programId]: {
              programId,
              choice: existing?.choice ?? fallbackChoice,
              operationId: existing?.operationId ?? newId(),
              startOperationId: existing?.startOperationId ?? newId(),
              jobId: job.id,
              step: job.progressCurrent,
              phase:
                job.status === 'failed' || job.status === 'cancelled' || job.status === 'interrupted'
                  ? { kind: 'failed', message: job.error ?? 'The problem could not be built.', stage: 'build' }
                  : job.status === 'completed'
                    ? job.resultRef
                      ? { kind: 'built' }
                      : { kind: 'failed', message: job.error ?? 'The built problem could not be opened.', stage: 'build' }
                    : { kind: 'building' },
              activityId: job.resultRef ?? null,
              seenJobs: { [job.id]: job },
            },
          },
        };
      });
    },

    ensureBuild: async (programId, choice, forceNew = false) => {
      ensureGlobalJobListener();
      const existing = get().handles[programId];
      if (!forceNew && existing) {
        if (
          existing.phase.kind === 'building'
          || existing.phase.kind === 'starting'
          || existing.phase.kind === 'built'
          || existing.phase.kind === 'ready'
        ) {
          return;
        }
        if (existing.phase.kind === 'failed') {
          return;
        }
      }
      if (!forceNew && inFlightBuildPrograms.has(programId)) return;
      const requestToken = (buildRequestTokens.get(programId) ?? 0) + 1;
      buildRequestTokens.set(programId, requestToken);
      inFlightBuildPrograms.add(programId);

      try {
        if (!forceNew) {
          const runningJob = await listActivePracticeBuildJob(programId);
          if (buildRequestTokens.get(programId) !== requestToken) return;
          if (runningJob) {
            get().attachBackendJob(programId, runningJob, choice);
            return;
          }
        }

        const operationId = newId();
        const startOperationId = newId();
        set((state) => ({
          handles: {
            ...state.handles,
            [programId]: {
              programId,
              choice,
              operationId,
              startOperationId,
              jobId: null,
              step: 0,
              phase: { kind: 'building' },
              activityId: null,
              seenJobs: {},
            },
          },
        }));

        const result = await programApi.startBuild({
          operationId,
          programId,
          lessonId: choice.lessonId ?? null,
          form: choice.form,
          language: choice.language,
          difficulty: choice.difficulty,
          focus: choice.focus ?? null,
        });
        if (buildRequestTokens.get(programId) !== requestToken) return;

        patchHandle(programId, (handle) => {
          if (handle.operationId !== operationId) return handle;
          return { ...handle, jobId: result.jobId };
        });

        const updated = get().handles[programId];
        const early = updated?.seenJobs[result.jobId];
        if (early) {
          applyJobToHandle(programId, early);
        }
      } catch (error) {
        if (buildRequestTokens.get(programId) !== requestToken) return;
        patchHandle(programId, (handle) => ({
          ...handle,
          phase: { kind: 'failed', message: getErrorMessage(error), stage: 'build' },
        }));
      } finally {
        if (buildRequestTokens.get(programId) === requestToken) {
          inFlightBuildPrograms.delete(programId);
        }
      }
    },

    receiveJobStatus: (job) => {
      const state = get();
      for (const [programId, handle] of Object.entries(state.handles)) {
        if (handle.jobId === job.id || (handle.jobId === null && isCodingBuildSubject(job.subjectId, programId))) {
          patchHandle(programId, (current) => ({
            ...current,
            seenJobs: { ...current.seenJobs, [job.id]: job },
          }));
          if (handle.jobId === job.id) {
            applyJobToHandle(programId, job);
          }
        }
      }
    },

    startSessionForBuild: async (programId) => {
      const current = get().handles[programId];
      if (!current?.activityId) return null;
      if (startingSessionPrograms.has(programId)) return null;
      startingSessionPrograms.add(programId);

      patchHandle(programId, (handle) => ({
        ...handle,
        phase: { kind: 'starting' },
      }));

      try {
        const session = await sessionApi.startSession({
          operationId: current.startOperationId,
          programId,
          activityId: current.activityId,
          setting: current.choice.setting,
          budgetSeconds: current.choice.budgetSeconds ?? null,
        });
        patchHandle(programId, (handle) => ({
          ...handle,
          phase: { kind: 'ready', sessionId: session.sessionId },
        }));
        return session.sessionId;
      } catch (error) {
        patchHandle(programId, (handle) => ({
          ...handle,
          phase: { kind: 'failed', message: getErrorMessage(error), stage: 'start' },
        }));
        return null;
      } finally {
        startingSessionPrograms.delete(programId);
      }
    },

    clearHandle: (programId) => {
      set((state) => {
        if (!(programId in state.handles)) return state;
        const next = { ...state.handles };
        delete next[programId];
        return { handles: next };
      });
    },
  };
});

export function resetCodingBuildStore() {
  buildRequestTokens.clear();
  inFlightBuildPrograms.clear();
  startingSessionPrograms.clear();
  if (listenerUnsubscribe) {
    listenerUnsubscribe();
    listenerUnsubscribe = null;
  }
  useCodingBuildStore.setState({ handles: {} });
}
