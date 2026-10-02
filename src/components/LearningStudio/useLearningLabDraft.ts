import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import VaultAPI from '@/lib/api';
import type { LearningLabFile, LearningPracticalDraftDto, SaveLearningPracticalDraftRequestDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';

const AUTOSAVE_DELAY_MS = 550;
type LoadState = 'idle' | 'loading' | 'ready' | 'error';
type SaveState = 'saved' | 'dirty' | 'saving' | 'error' | 'conflict';
type StableOperation = { request: SaveLearningPracticalDraftRequestDto; files: LearningLabFile[] };
type DraftContext = {
  key: string;
  programId: string;
  activityId: string;
  activityRevision: number;
  files: LearningLabFile[];
  savedFiles: LearningLabFile[] | null;
  changedPaths: Set<string>;
  editVersions: Map<string, number>;
  editCounter: number;
  loadState: LoadState;
  saveState: SaveState;
  error: string | null;
  revision: number;
  loadPromise: Promise<void> | null;
  loadToken: number;
  operation: StableOperation | null;
  inFlight: Promise<boolean> | null;
  timer: number | null;
};

function makeId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  const bytes = new Uint8Array(16);
  if (globalThis.crypto?.getRandomValues) globalThis.crypto.getRandomValues(bytes);
  else for (let index = 0; index < bytes.length; index += 1) bytes[index] = Math.floor(Math.random() * 256);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : 'The lab draft could not be saved.';
}
function isConflict(error: unknown) {
  return /revision|conflict|changed since|stale/i.test(errorMessage(error));
}
function sameFiles(a: LearningLabFile[] | null, b: LearningLabFile[]) {
  return Boolean(a && a.length === b.length && a.every((file, index) => file.path === b[index]?.path && file.content === b[index]?.content));
}
function cloneFiles(files: LearningLabFile[]) { return files.map(({ path, content }) => ({ path, content })); }
function mergeSaved(starter: LearningLabFile[], saved: LearningLabFile[], changedPaths: Set<string>) {
  const byPath = new Map(saved.map((file) => [file.path, file.content]));
  return starter.map((file) => changedPaths.has(file.path) ? file : ({ ...file, content: byPath.get(file.path) ?? file.content }));
}

/** SQLite-backed learner edits, keyed by the exact activity revision. */
export function useLearningLabDraft({
  programId,
  activityId,
  activityRevision,
  starterFiles,
}: {
  programId: string;
  activityId: string | null;
  activityRevision: number;
  /** Pass only editable starter files. The backend enforces that boundary again. */
  starterFiles: LearningLabFile[];
}) {
  const [, render] = useState(0);
  const contexts = useRef(new Map<string, DraftContext>());
  const currentKey = activityId ? `${programId}:${activityId}:${activityRevision}` : null;
  const starterSignature = JSON.stringify(starterFiles);
  const currentRef = useRef<DraftContext | null>(null);

  const refresh = useCallback(() => render((value) => value + 1), []);
  const ensureContext = useCallback((key: string, pId: string, aId: string, revision: number, initial: LearningLabFile[]) => {
    const existing = contexts.current.get(key);
    if (existing) {
      const known = new Set(existing.files.map((file) => file.path));
      const missing = initial.filter((file) => !known.has(file.path));
      if (missing.length) existing.files = [...existing.files, ...cloneFiles(missing)];
      return existing;
    }
    const context: DraftContext = {
      key, programId: pId, activityId: aId, activityRevision: revision,
      files: cloneFiles(initial), savedFiles: null, changedPaths: new Set(), editVersions: new Map(), editCounter: 0,
      loadState: 'loading', saveState: 'saved', error: null, revision: 0, loadPromise: null, loadToken: 0,
      operation: null, inFlight: null, timer: null,
    };
    contexts.current.set(key, context);
    return context;
  }, []);

  const load = useCallback((context: DraftContext, starter: LearningLabFile[], force = false): Promise<void> => {
    if (context.loadPromise && !force) return context.loadPromise;
    if (context.loadState === 'ready' && !force) return Promise.resolve();
    const token = ++context.loadToken;
    context.loadState = 'loading'; context.error = null; refresh();
    let promise!: Promise<void>;
    promise = (async () => {
      try {
        const draft = unwrap(await VaultAPI.getLearningPracticalDraft({
          programId: context.programId, activityId: context.activityId, activityRevision: context.activityRevision,
        }));
        if (draft.programId !== context.programId || draft.activityId !== context.activityId || draft.activityRevision !== context.activityRevision) {
          throw new Error('The saved lab draft belongs to a different activity revision.');
        }
        if (context.loadToken === token) {
          context.files = mergeSaved(context.files.length ? context.files : starter, draft.files, context.changedPaths);
          context.savedFiles = cloneFiles(draft.files);
          context.revision = draft.draftRevision;
          context.loadState = 'ready';
          context.saveState = sameFiles(context.savedFiles, context.files) ? 'saved' : 'dirty';
          context.error = null;
        }
      } catch (error) {
        if (context.loadToken === token) { context.loadState = 'error'; context.saveState = 'error'; context.error = errorMessage(error); }
      } finally {
        if (context.loadPromise === promise) context.loadPromise = null;
        refresh();
      }
    })();
    context.loadPromise = promise;
    return promise;
  }, [refresh]);

  const drainContext = useCallback((context: DraftContext): Promise<boolean> => {
    if (context.inFlight) return context.inFlight;
    const work = (async () => {
      if (context.loadState === 'loading' && context.loadPromise) await context.loadPromise;
      if (context.loadState !== 'ready') return false;
      while (true) {
        if (context.saveState === 'conflict') return false;
        if (!context.operation && sameFiles(context.savedFiles, context.files)) {
          context.saveState = 'saved'; context.error = null; refresh(); return true;
        }
        if (!context.operation) {
          const files = cloneFiles(context.files);
          context.operation = {
            files,
            request: {
              operationId: makeId(), programId: context.programId, activityId: context.activityId,
              activityRevision: context.activityRevision, expectedDraftRevision: context.revision, files,
            },
          };
        }
        const operation = context.operation;
        context.saveState = 'saving'; context.error = null; refresh();
        try {
          const saved: LearningPracticalDraftDto = unwrap(await VaultAPI.saveLearningPracticalDraft(operation.request));
          if (saved.programId !== context.programId || saved.activityId !== context.activityId || saved.activityRevision !== context.activityRevision) {
            context.saveState = 'error'; context.error = 'The saved lab draft reply belongs to a different activity revision.';
            refresh(); return false;
          }
          if (saved.draftRevision !== operation.request.expectedDraftRevision + 1) {
            context.operation = null;
            context.saveState = 'conflict'; context.error = 'This lab draft advanced to a newer revision. Reload it before saving again.';
            refresh(); return false;
          }
          context.revision = saved.draftRevision;
          context.savedFiles = cloneFiles(operation.files);
          context.operation = null;
          if (!sameFiles(context.savedFiles, context.files)) {
            context.saveState = 'dirty';
            continue;
          }
          context.saveState = 'saved'; context.error = null; refresh(); return true;
        } catch (error) {
          context.saveState = isConflict(error) ? 'conflict' : 'error';
          context.error = errorMessage(error); refresh(); return false;
        }
      }
    })();
    context.inFlight = work;
    return work.finally(() => { if (context.inFlight === work) context.inFlight = null; });
  }, [refresh]);

  if (currentKey && activityId) {
    const context = ensureContext(currentKey, programId, activityId, activityRevision, starterFiles);
    currentRef.current = context;
  } else {
    currentRef.current = null;
  }
  const current = currentKey ? contexts.current.get(currentKey) ?? null : null;

  useEffect(() => {
    if (!current) return;
    void load(current, starterFiles);
    return () => {
      if (current.timer) { window.clearTimeout(current.timer); current.timer = null; }
      if (current.saveState === 'dirty' || current.saveState === 'error') void drainContext(current);
    };
  // The file array can be reconstructed by callers on render; its serialized
  // contents, rather than its object identity, define this load boundary.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current, starterSignature, load, drainContext]);

  useEffect(() => {
    if (!current || current.loadState !== 'ready' || sameFiles(current.savedFiles, current.files) || current.saveState !== 'dirty') return;
    if (current.timer) window.clearTimeout(current.timer);
    current.timer = window.setTimeout(() => { current.timer = null; void drainContext(current); }, AUTOSAVE_DELAY_MS);
    return () => { if (current.timer) { window.clearTimeout(current.timer); current.timer = null; } };
  }, [current, current?.files, current?.savedFiles, current?.loadState, current?.saveState, drainContext]);

  const flush = useCallback(async () => {
    const results = await Promise.all([...contexts.current.values()].map((context) => drainContext(context)));
    return results.every(Boolean);
  }, [drainContext]);

  useEffect(() => {
    const unregister = registerPendingSave(() => flush());
    const beforeUnload = (event: BeforeUnloadEvent) => {
      const dirty = [...contexts.current.values()].some((context) => context.saveState !== 'saved' || context.operation || context.inFlight);
      if (dirty) { event.preventDefault(); event.returnValue = ''; }
    };
    window.addEventListener('beforeunload', beforeUnload);
    return () => {
      window.removeEventListener('beforeunload', beforeUnload);
      void flush().finally(unregister);
    };
  }, [flush]);

  const setFileContent = useCallback((path: string, content: string) => {
    const context = currentRef.current;
    if (!context || context.loadState === 'error') return;
    const index = context.files.findIndex((file) => file.path === path);
    if (index < 0 || context.files[index].content === content) return;
    context.files = context.files.map((file, fileIndex) => fileIndex === index ? { ...file, content } : file);
    context.changedPaths.add(path);
    context.editCounter += 1; context.editVersions.set(path, context.editCounter);
    if (context.loadState === 'ready' && context.saveState !== 'conflict') context.saveState = 'dirty';
    if (context.saveState !== 'conflict') context.error = null;
    refresh();
  }, [refresh]);

  const retryLoad = useCallback(async () => {
    if (!current) return false;
    await load(current, starterFiles, true);
    return current.loadState === 'ready';
  }, [current, load, starterFiles]);

  const fetchLatest = useCallback(async (context: DraftContext) => {
    const draft = unwrap(await VaultAPI.getLearningPracticalDraft({
      programId: context.programId, activityId: context.activityId, activityRevision: context.activityRevision,
    }));
    if (draft.programId !== context.programId || draft.activityId !== context.activityId || draft.activityRevision !== context.activityRevision) {
      throw new Error('The saved lab draft belongs to a different activity revision.');
    }
    return draft;
  }, []);

  const keepLocalEdits = useCallback(async () => {
    if (!current) return false;
    if (current.inFlight) await current.inFlight;
    try {
      const latest = await fetchLatest(current);
      const local = current.files;
      const changed = new Set(current.changedPaths);
      current.savedFiles = cloneFiles(latest.files);
      current.revision = latest.draftRevision;
      current.operation = null;
      current.files = mergeSaved(local, latest.files, changed);
      current.changedPaths = new Set(current.files.filter((file) => latest.files.find((saved) => saved.path === file.path)?.content !== file.content).map((file) => file.path));
      current.saveState = current.changedPaths.size ? 'dirty' : 'saved';
      current.error = null; refresh();
      return true;
    } catch (error) {
      current.saveState = 'conflict'; current.error = errorMessage(error); refresh();
      return false;
    }
  }, [current, fetchLatest, refresh]);

  const useSavedVersion = useCallback(async () => {
    if (!current) return false;
    if (current.inFlight) await current.inFlight;
    const actionVersion = current.editCounter;
    try {
      const latest = await fetchLatest(current);
      const editsAfterAction = new Set([...current.changedPaths].filter((path) => (current.editVersions.get(path) ?? 0) > actionVersion));
      current.savedFiles = cloneFiles(latest.files);
      current.revision = latest.draftRevision;
      current.operation = null;
      current.files = mergeSaved(current.files, latest.files, editsAfterAction);
      current.changedPaths = editsAfterAction;
      current.saveState = editsAfterAction.size ? 'dirty' : 'saved';
      current.error = null; refresh();
      return true;
    } catch (error) {
      current.saveState = 'conflict'; current.error = errorMessage(error); refresh();
      return false;
    }
  }, [current, fetchLatest, refresh]);

  const files = useMemo(() => cloneFiles(current?.files ?? starterFiles), [current?.files, starterFiles]);
  return {
    files,
    setFileContent,
    loadState: current?.loadState ?? 'idle' as LoadState,
    saveState: current?.saveState ?? 'saved' as SaveState,
    error: current?.error ?? null,
    flush,
    retryLoad,
    keepLocalEdits,
    useSavedVersion,
  };
}
