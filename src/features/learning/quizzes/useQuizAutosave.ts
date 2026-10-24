import { useCallback, useEffect, useRef, useState } from 'react';

import { draftOf, fingerprintOf, responseOf, type ItemDraft } from '@/features/learning/quizzes/items/itemResponse';
import { newOperationId, useSaveQuizResponse } from '@/features/learning/quizzes/useQuizzes';
import type { LearningAssessmentFormDto, SaveLearningAssessmentResponseRequestDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';

export type SaveState = 'saved' | 'saving' | 'unsaved' | 'error';

const DEBOUNCE_MS = 650;

function message(error: unknown) {
  return error instanceof Error ? error.message : 'Your answer could not be saved.';
}

const isConflict = (error: unknown) => /changed|revision|conflict|reload and retry/i.test(message(error));

function draftsOf(form: LearningAssessmentFormDto) {
  return Object.fromEntries(form.items.map((item) => [item.id, draftOf(item)]));
}

function confirmedOf(form: LearningAssessmentFormDto) {
  return new Map(form.items.map((item) => [item.id, fingerprintOf(item, draftOf(item))]));
}

/**
 * Per-item autosave for an active quiz form. Each changed answer is saved once,
 * against the form's revision, with an operation id reused until it lands; the
 * app's pending-save registry flushes it before navigation.
 */
export function useQuizAutosave(form: LearningAssessmentFormDto, programId: string, reload: () => Promise<LearningAssessmentFormDto | undefined>) {
  const save = useSaveQuizResponse();
  const [drafts, setDrafts] = useState<Record<string, ItemDraft>>(() => draftsOf(form));
  const [saveError, setSaveError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [, setSavedTick] = useState(0);
  const draftsRef = useRef(drafts);
  const revisionRef = useRef(form.revision);
  const confirmedRef = useRef(confirmedOf(form));
  const requestsRef = useRef(new Map<string, SaveLearningAssessmentResponseRequestDto>());
  const retryRef = useRef<SaveLearningAssessmentResponseRequestDto | null>(null);
  const flightRef = useRef<Promise<boolean> | null>(null);
  // The status after the last save: answering a paused form resumes it.
  const statusRef = useRef(form.status);

  useEffect(() => {
    // A new immutable form is the only reason to replace the local answers.
    const initial = draftsOf(form);
    setDrafts(initial);
    draftsRef.current = initial;
    revisionRef.current = form.revision;
    statusRef.current = form.status;
    confirmedRef.current = confirmedOf(form);
    requestsRef.current.clear();
    retryRef.current = null;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [form.id]);

  const send = useCallback(async (request: SaveLearningAssessmentResponseRequestDto, fingerprint: string) => {
    retryRef.current = request;
    try {
      const saved = await save.mutateAsync(request);
      if (saved.revision !== request.expectedRevision + 1) {
        setConflict(true);
        setSaveError('This quiz changed elsewhere. Reload it to continue; your answers stay here.');
        return false;
      }
      revisionRef.current = saved.revision;
      statusRef.current = saved.status;
      confirmedRef.current.set(request.response.itemId, fingerprint);
      for (const key of [...requestsRef.current.keys()]) if (key.startsWith(`${request.response.itemId}:`)) requestsRef.current.delete(key);
      retryRef.current = null;
      setSaveError(null);
      setConflict(false);
      setSavedTick((tick) => tick + 1);
      return true;
    } catch (error) {
      setSaveError(message(error));
      setConflict(isConflict(error));
      return false;
    }
  }, [save]);

  const flush = useCallback(async () => {
    if (form.status === 'submitted') return true;
    if (flightRef.current) return flightRef.current;
    const flight = (async () => {
      for (const item of form.items) {
        // Re-read after each save: the learner may keep typing while one is in flight.
        while (true) {
          const draft = draftsRef.current[item.id] ?? draftOf(item);
          const fingerprint = fingerprintOf(item, draft);
          if (confirmedRef.current.get(item.id) === fingerprint) break;
          const key = `${item.id}:${fingerprint}`;
          let request = requestsRef.current.get(key);
          if (!request) {
            request = { operationId: newOperationId(), programId, formId: form.id, expectedRevision: revisionRef.current, response: responseOf(item, draft), assistance: [] };
            requestsRef.current.set(key, request);
          }
          if (!(await send(request, fingerprint))) return false;
        }
      }
      return true;
    })();
    flightRef.current = flight;
    try { return await flight; } finally { if (flightRef.current === flight) flightRef.current = null; }
  }, [form, programId, send]);

  const flushRef = useRef(flush);
  flushRef.current = flush;
  useEffect(() => registerPendingSave(() => flushRef.current()), [form.id]);

  useEffect(() => {
    if (form.status === 'submitted') return;
    const timer = setTimeout(() => { void flushRef.current(); }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [drafts, form.status, form.id]);

  const update = useCallback((itemId: string, patch: Partial<ItemDraft>) => {
    setDrafts((current) => {
      const next = { ...current, [itemId]: { ...current[itemId], ...patch } };
      draftsRef.current = next;
      return next;
    });
  }, []);

  const retry = useCallback(async () => {
    const request = retryRef.current;
    if (!request) return flushRef.current();
    const item = form.items.find((entry) => entry.id === request.response.itemId);
    const ok = item ? await send(request, fingerprintOf(item, draftsRef.current[item.id] ?? draftOf(item))) : false;
    return ok ? flushRef.current() : false;
  }, [form.items, send]);

  /** After a conflict: take the saved revision, keep the learner's answers, and save them again. */
  const reloadKeepingAnswers = useCallback(async () => {
    const fresh = await reload();
    if (!fresh) return;
    revisionRef.current = fresh.revision;
    statusRef.current = fresh.status;
    confirmedRef.current = confirmedOf(fresh);
    requestsRef.current.clear();
    retryRef.current = null;
    setConflict(false);
    setSaveError(null);
    void flushRef.current();
  }, [reload]);

  const unsaved = form.items.some((item) => confirmedRef.current.get(item.id) !== fingerprintOf(item, drafts[item.id] ?? draftOf(item)));
  const state: SaveState = saveError ? 'error' : save.isPending ? 'saving' : unsaved ? 'unsaved' : 'saved';
  return { drafts, update, flush, retry, reloadKeepingAnswers, state, saveError, conflict, revision: () => revisionRef.current, status: () => statusRef.current };
}
