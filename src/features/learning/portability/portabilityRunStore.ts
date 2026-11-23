import { open } from '@tauri-apps/plugin-dialog';
import { create } from 'zustand';

import { refreshLearningProgramAfterImport } from '@/features/learning/api/learningInvalidation';
import { LEARNING_PORTABILITY_PREFIX, learningPortabilityKey } from '@/features/learning/api/learningQueryKeys';
import { DEFAULT_CONFLICT_POLICY } from '@/features/learning/portability/conflictPolicies';
import { MOVE_EXPORT_DEFAULTS, packOptionsRequest, type PackExportOptionsValue } from '@/features/learning/portability/PackExportOptions';
import { failureMessage, nextRetryEntry, type RetryEntry } from '@/features/learning/portability/requestRetry';
import type { ApiClient } from '@/lib/api';
import type {
  ApplyLearningPackImportRequestDto,
  CancelLearningPackImportPreviewRequestDto,
  ExportAllLearningPacksRequestDto,
  ExportAllLearningPacksResultDto,
  ExportLearningPackRequestDto,
  LearningPackConflictPolicy,
  LearningPackImportPreviewDto,
  PreviewLearningPackImportRequestDto,
} from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';
import { unwrapApiResult as unwrap } from '@/types/api/result';

import type { QueryClient } from '@tanstack/react-query';

export type ExportAllOutcome = Extract<ExportAllLearningPacksResultDto, { outcome: 'exported' }>;
export type ExportAction = 'export' | 'saveAs';
export type ImportFailure = { message: string; retry?: 'preview' | 'apply' };
export type ImportPendingAction = 'choosing' | 'preview' | 'apply' | 'cancel' | null;

export interface ExportAllRunState {
  options: PackExportOptionsValue;
  pending: boolean;
  result: ExportAllOutcome | null;
  error: string;
  retryEntry: RetryEntry<ExportAllLearningPacksRequestDto> | null;
  screenMounted: boolean;
  finishedWhileAway: boolean;
}

export interface ImportFlowRunState {
  scope: string | null;
  pendingAction: ImportPendingAction;
  preview: LearningPackImportPreviewDto | null;
  applyConfirm: boolean;
  failure: ImportFailure | null;
  notice: string;
  retryPreview: RetryEntry<PreviewLearningPackImportRequestDto> | null;
  retryApply: RetryEntry<ApplyLearningPackImportRequestDto> | null;
  retryCancel: RetryEntry<CancelLearningPackImportPreviewRequestDto> | null;
  lastImported: { programId: string; title: string } | null;
  importSeq: number;
  mountedCount: number;
  finishedWhileAway: boolean;
}

export interface ProgramPortabilityRunState {
  options: PackExportOptionsValue;
  pendingAction: ExportAction | null;
  error: { message: string; retry?: ExportAction } | null;
  success: string;
  lastDestination: string;
  retryExport: RetryEntry<ExportLearningPackRequestDto> | null;
  retrySaveAs: RetryEntry<ExportLearningPackRequestDto> | null;
}

export const EMPTY_PROGRAM_PORTABILITY_RUN: ProgramPortabilityRunState = {
  options: MOVE_EXPORT_DEFAULTS,
  pendingAction: null,
  error: null,
  success: '',
  lastDestination: '',
  retryExport: null,
  retrySaveAs: null,
};

function initialExportAllState(): ExportAllRunState {
  return {
    options: MOVE_EXPORT_DEFAULTS,
    pending: false,
    result: null,
    error: '',
    retryEntry: null,
    screenMounted: false,
    finishedWhileAway: false,
  };
}

function initialImportFlowState(): ImportFlowRunState {
  return {
    scope: null,
    pendingAction: null,
    preview: null,
    applyConfirm: false,
    failure: null,
    notice: '',
    retryPreview: null,
    retryApply: null,
    retryCancel: null,
    lastImported: null,
    importSeq: 0,
    mountedCount: 0,
    finishedWhileAway: false,
  };
}

interface PortabilityRunStore {
  exportAll: ExportAllRunState;
  importFlow: ImportFlowRunState;
  programRuns: Record<string, ProgramPortabilityRunState>;

  setExportAllOptions: (options: PackExportOptionsValue) => void;
  setExportAllError: (error: string) => void;
  markExportAllMounted: (mounted: boolean) => void;
  dismissExportAllBanner: () => void;
  runExportAll: (api: ApiClient, client: QueryClient, retry?: RetryEntry<ExportAllLearningPacksRequestDto>) => Promise<void>;

  markImportMounted: (mounted: boolean) => void;
  dismissImportBanner: () => void;
  dismissImportFeedback: () => void;
  setImportApplyConfirm: (open: boolean) => void;
  chooseAndPreviewImportFile: (api: ApiClient, client: QueryClient, scope?: string) => Promise<void>;
  retryImportPreview: (api: ApiClient, client: QueryClient) => Promise<void>;
  chooseImportPolicy: (api: ApiClient, client: QueryClient, policy: LearningPackConflictPolicy) => Promise<void>;
  abandonImportPreview: (api: ApiClient, client: QueryClient) => Promise<void>;
  applyImport: (api: ApiClient, client: QueryClient) => Promise<{ programId: string; title: string } | null>;

  setProgramOptions: (programId: string, options: PackExportOptionsValue) => void;
  setProgramError: (programId: string, error: { message: string; retry?: ExportAction } | null) => void;
  runProgramExport: (programId: string, action: ExportAction, api: ApiClient, client: QueryClient) => Promise<void>;
}

/** Prompt user to choose a course file (.lattice-learning). */
export async function promptChooseCourseFile(): Promise<string | null> {
  const chosen = await open({
    multiple: false,
    directory: false,
    title: 'Choose a course file',
    filters: [{ name: 'Lattice course', extensions: ['lattice-learning'] }],
  });
  if (!chosen || Array.isArray(chosen)) return null;
  return chosen;
}

export const usePortabilityRunStore = create<PortabilityRunStore>((set, get) => {
  const patchExportAll = (patch: Partial<ExportAllRunState>) =>
    set((state) => ({ exportAll: { ...state.exportAll, ...patch } }));

  const patchImportFlow = (patch: Partial<ImportFlowRunState>) =>
    set((state) => ({ importFlow: { ...state.importFlow, ...patch } }));

  const programState = (programId: string) => get().programRuns[programId] ?? EMPTY_PROGRAM_PORTABILITY_RUN;

  const patchProgram = (programId: string, patch: Partial<ProgramPortabilityRunState>) =>
    set((state) => ({
      programRuns: {
        ...state.programRuns,
        [programId]: { ...(state.programRuns[programId] ?? EMPTY_PROGRAM_PORTABILITY_RUN), ...patch },
      },
    }));

  const executePreview = async (
    api: ApiClient,
    client: QueryClient,
    entry: RetryEntry<PreviewLearningPackImportRequestDto>,
  ) => {
    patchImportFlow({
      pendingAction: 'preview',
      failure: null,
      notice: '',
      applyConfirm: false,
      retryPreview: entry,
    });
    try {
      const updated = unwrap(await api.previewLearningPackImport(entry.request));
      client.setQueryData(learningPortabilityKey(updated.programId), updated);
      patchImportFlow({
        pendingAction: null,
        retryPreview: null,
        preview: updated.importPreviews.find((item) => item.id === entry.request.previewId) ?? updated.importPreviews[0] ?? null,
      });
    } catch (cause) {
      patchImportFlow({
        pendingAction: null,
        failure: { message: failureMessage(cause, 'The selected file could not be previewed.'), retry: 'preview' },
      });
    }
  };

  const cancelPendingPreview = async (
    api: ApiClient,
    client: QueryClient,
    target: LearningPackImportPreviewDto,
  ): Promise<LearningPackImportPreviewDto | undefined> => {
    const entry = nextRetryEntry(
      get().importFlow.retryCancel,
      { previewId: target.id },
      (id) => ({ operationId: id() }),
    );
    patchImportFlow({ pendingAction: 'cancel', retryCancel: entry });
    try {
      const updated = unwrap(await api.cancelLearningPackImportPreview(entry.request));
      client.setQueryData(learningPortabilityKey(updated.programId), updated);
      patchImportFlow({ pendingAction: null, retryCancel: null });
      return updated.importPreviews.find((item) => item.id === target.id) ?? { ...target, status: 'cancelled' as const };
    } catch (cause) {
      patchImportFlow({
        pendingAction: null,
        failure: { message: failureMessage(cause, 'The preview could not be cancelled.') },
      });
      return undefined;
    }
  };

  return {
    exportAll: initialExportAllState(),
    importFlow: initialImportFlowState(),
    programRuns: {},

    setExportAllOptions: (options) => patchExportAll({ options }),
    setExportAllError: (error) => patchExportAll({ error }),
    markExportAllMounted: (mounted) =>
      set((state) => ({
        exportAll: {
          ...state.exportAll,
          screenMounted: mounted,
          finishedWhileAway: mounted ? false : state.exportAll.finishedWhileAway,
        },
      })),
    dismissExportAllBanner: () => patchExportAll({ finishedWhileAway: false }),

    runExportAll: async (api, client, explicitRetry) => {
      const current = get().exportAll;
      if (current.pending) return;
      const entry = explicitRetry ?? nextRetryEntry(
        current.retryEntry,
        { options: packOptionsRequest(current.options) },
        (id) => ({ operationId: id() }),
      );
      patchExportAll({ pending: true, error: '', retryEntry: entry, finishedWhileAway: false });
      try {
        const response = unwrap(await api.exportAllLearningPacks(entry.request));
        if (response.outcome === 'exported') {
          await client.invalidateQueries({ queryKey: LEARNING_PORTABILITY_PREFIX });
          patchExportAll({
            pending: false,
            retryEntry: null,
            result: response,
            finishedWhileAway: !get().exportAll.screenMounted,
          });
        } else {
          // Closing the folder dialog writes and records nothing.
          patchExportAll({ pending: false, retryEntry: null });
        }
      } catch (cause) {
        patchExportAll({
          pending: false,
          error: failureMessage(cause, 'The courses could not be exported.'),
          finishedWhileAway: !get().exportAll.screenMounted,
        });
      }
    },

    markImportMounted: (mounted) =>
      set((state) => {
        const mountedCount = Math.max(0, state.importFlow.mountedCount + (mounted ? 1 : -1));
        return {
          importFlow: {
            ...state.importFlow,
            mountedCount,
            finishedWhileAway: mounted ? false : state.importFlow.finishedWhileAway,
          },
        };
      }),
    dismissImportBanner: () => patchImportFlow({ finishedWhileAway: false }),
    dismissImportFeedback: () =>
      set((state) => {
        if (state.importFlow.pendingAction !== null) return state;
        return {
          importFlow: {
            ...state.importFlow,
            notice: '',
            failure: null,
            finishedWhileAway: false,
            lastImported: null,
            retryPreview: null,
            retryApply: null,
            preview: state.importFlow.preview?.status === 'applied' ? null : state.importFlow.preview,
          },
        };
      }),
    setImportApplyConfirm: (open) => patchImportFlow({ applyConfirm: open }),

    chooseAndPreviewImportFile: async (api, client, scope = 'standalone') => {
      const current = get().importFlow;
      if (current.pendingAction !== null) return;
      const scopeChanged = current.scope !== null && current.scope !== scope;
      patchImportFlow({
        scope,
        pendingAction: 'choosing',
        failure: null,
        notice: '',
        applyConfirm: false,
        ...(scopeChanged ? { preview: null, retryPreview: null, retryApply: null, lastImported: null } : {}),
      });
      try {
        const chosen = await promptChooseCourseFile();
        if (!chosen) {
          patchImportFlow({ pendingAction: null });
          return;
        }
        const entry = nextRetryEntry(
          get().importFlow.retryPreview,
          { sourcePath: chosen, conflictPolicy: DEFAULT_CONFLICT_POLICY },
          (id) => ({ operationId: id(), previewId: id() }),
        );
        await executePreview(api, client, entry);
      } catch (cause) {
        patchImportFlow({
          pendingAction: null,
          failure: { message: failureMessage(cause, 'The selected file could not be previewed.') },
        });
      }
    },

    retryImportPreview: async (api, client) => {
      const current = get().importFlow;
      if (current.pendingAction !== null || !current.retryPreview) return;
      await executePreview(api, client, current.retryPreview);
    },

    chooseImportPolicy: async (api, client, policy) => {
      const current = get().importFlow;
      if (!current.preview || current.pendingAction !== null) return;
      patchImportFlow({ failure: null, notice: '', applyConfirm: false });
      let targetPreview = current.preview;
      if (targetPreview.status === 'pending') {
        const cancelled = await cancelPendingPreview(api, client, targetPreview);
        if (!cancelled) return;
        patchImportFlow({ preview: cancelled });
        targetPreview = cancelled;
      }
      const entry = nextRetryEntry(
        get().importFlow.retryPreview,
        { sourcePath: targetPreview.sourcePath, conflictPolicy: policy },
        (id) => ({ operationId: id(), previewId: id() }),
      );
      await executePreview(api, client, entry);
    },

    abandonImportPreview: async (api, client) => {
      const current = get().importFlow;
      if (!current.preview || current.pendingAction !== null) return;
      patchImportFlow({ failure: null, notice: '', applyConfirm: false });
      if (await cancelPendingPreview(api, client, current.preview)) {
        patchImportFlow({
          preview: null,
          notice: 'Preview cancelled. Nothing was imported.',
        });
      }
    },

    applyImport: async (api, client) => {
      const current = get().importFlow;
      const targetPreview = current.preview;
      if (!targetPreview?.canApply || current.pendingAction !== null) return null;
      const entry = nextRetryEntry(
        current.retryApply,
        { previewId: targetPreview.id, expectedRootSha256: targetPreview.manifest.rootSha256 },
        (id) => ({ operationId: id() }),
      );
      patchImportFlow({
        pendingAction: 'apply',
        failure: null,
        notice: '',
        retryApply: entry,
        finishedWhileAway: false,
      });
      try {
        if (!await flushPendingSaves()) {
          throw new Error('Your work could not be saved before importing. Resolve the save error and retry the import.');
        }
        const updated = unwrap(await api.applyLearningPackImport(entry.request));
        client.setQueryData(learningPortabilityKey(updated.programId), updated);
        await refreshLearningProgramAfterImport(client, updated.programId);
        const title = targetPreview.incomingProgramTitle;
        patchImportFlow({
          pendingAction: null,
          retryApply: null,
          applyConfirm: false,
          preview: updated.importPreviews.find((item) => item.id === targetPreview.id) ?? { ...targetPreview, status: 'applied' },
          notice: `“${title}” is ready on this computer.`,
          lastImported: { programId: updated.programId, title },
          importSeq: get().importFlow.importSeq + 1,
          finishedWhileAway: get().importFlow.mountedCount === 0,
        });
        return { programId: updated.programId, title };
      } catch (cause) {
        patchImportFlow({
          pendingAction: null,
          applyConfirm: false,
          failure: { message: failureMessage(cause, 'The import could not be applied.'), retry: 'apply' },
          finishedWhileAway: get().importFlow.mountedCount === 0,
        });
        return null;
      }
    },

    setProgramOptions: (programId, options) => patchProgram(programId, { options }),
    setProgramError: (programId, error) => patchProgram(programId, { error }),

    runProgramExport: async (programId, action, api, client) => {
      const current = programState(programId);
      if (current.pendingAction !== null) return;
      const retryKey = action === 'export' ? 'retryExport' : 'retrySaveAs';
      const entry = nextRetryEntry(
        current[retryKey],
        { programId, options: packOptionsRequest(current.options) },
        (id) => ({ operationId: id() }),
      );
      patchProgram(programId, {
        pendingAction: action,
        error: null,
        success: '',
        [retryKey]: entry,
      });
      try {
        if (action === 'export') {
          const updated = unwrap(await api.exportLearningPack(entry.request));
          client.setQueryData(learningPortabilityKey(updated.programId), updated);
          await client.invalidateQueries({ queryKey: learningPortabilityKey(programId) });
          const path = updated.exports.at(0)?.destinationPath;
          patchProgram(programId, {
            pendingAction: null,
            retryExport: null,
            lastDestination: path ?? programState(programId).lastDestination,
            success: path ? `Course file saved to ${path}` : 'Course file saved.',
          });
        } else {
          const result = unwrap(await api.saveLearningPackAs(entry.request));
          await client.invalidateQueries({ queryKey: learningPortabilityKey(programId) });
          if (result.outcome === 'saved') {
            const path = result.export.destinationPath;
            patchProgram(programId, {
              pendingAction: null,
              retrySaveAs: null,
              lastDestination: path,
              success: `Course file saved to ${path}`,
            });
          } else {
            // Closing the save dialog writes and records nothing.
            patchProgram(programId, { pendingAction: null, retrySaveAs: null });
          }
        }
      } catch (cause) {
        const fallback = action === 'export' ? 'The course file could not be created.' : 'The course file could not be saved.';
        patchProgram(programId, {
          pendingAction: null,
          error: { message: failureMessage(cause, fallback), retry: action },
        });
      }
    },
  };
});

export function resetPortabilityRunStore() {
  usePortabilityRunStore.setState({
    exportAll: initialExportAllState(),
    importFlow: initialImportFlowState(),
    programRuns: {},
  });
}
