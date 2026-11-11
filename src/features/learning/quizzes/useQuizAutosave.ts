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
  const conflictRef = useRef(false);
  const [, setSavedTick] = useState(0);
  const draftsRef = useRef(drafts);
  const revisionRef = useRef(form.revision);
  const confirmedRef = useRef(confirmedOf(form));
  const retryRef = useRef<{ request: SaveLearningAssessmentResponseRequestDto; fingerprint: string } | null>(null);
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
    retryRef.current = null;
    conflictRef.current = false;
    setConflict(false);
    setSaveError(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [form.id]);

  const send = useCallback(async (request: SaveLearningAssessmentResponseRequestDto, fingerprint: string) => {
    retryRef.current = { request, fingerprint };
    try {
      const saved = await save.mutateAsync(request);
      if (saved.revision !== request.expectedRevision + 1) {
        conflictRef.current = true;
        setConflict(true);
        setSaveError('This quiz changed elsewhere. Reload it to continue; your answers stay here.');
        return false;
      }
      revisionRef.current = saved.revision;
      statusRef.current = saved.status;
      confirmedRef.current.set(request.response.itemId, fingerprint);
      retryRef.current = null;
      setSaveError(null);
      setConflict(false);
      conflictRef.current = false;
      setSavedTick((tick) => tick + 1);
      return true;
    } catch (error) {
      setSaveError(message(error));
      conflictRef.current = isConflict(error);
      setConflict(conflictRef.current);
      return false;
    }
  }, [save]);

  const flush = useCallback(async () => {
    if (statusRef.current === 'submitted') return true;
    if (flightRef.current) return flightRef.current;
    const flight = (async () => {
      while (true) {
        if (conflictRef.current) return false;
        // Resolve an uncertain write with its original payload before sending
        // newer edits, even if the learner has since reverted an answer.
        let pending = retryRef.current;
        if (!pending) {
          // Revisit every item after each save: an earlier answer can change
          // while a later one is in flight.
          const item = form.items.find((entry) => confirmedRef.current.get(entry.id) !== fingerprintOf(entry, draftsRef.current[entry.id] ?? draftOf(entry)));
          if (!item) return true;
          const draft = draftsRef.current[item.id] ?? draftOf(item);
          pending = {
            fingerprint: fingerprintOf(item, draft),
            request: { operationId: newOperationId(), programId, formId: form.id, expectedRevision: revisionRef.current, response: responseOf(item, draft), assistance: [] },
          };
        }
        if (!(await send(pending.request, pending.fingerprint))) return false;
      }
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
    const current = draftsRef.current;
    const next = { ...current, [itemId]: { ...current[itemId], ...patch } };
    draftsRef.current = next;
    setDrafts(next);
  }, []);

  const retry = useCallback(() => flushRef.current(), []);

  /** After a conflict: take the saved revision, keep the learner's answers, and save them again. */
  const reloadKeepingAnswers = useCallback(async () => {
    const fresh = await reload();
    if (!fresh) return;
    revisionRef.current = fresh.revision;
    statusRef.current = fresh.status;
    confirmedRef.current = confirmedOf(fresh);
    retryRef.current = null;
    conflictRef.current = false;
    setConflict(false);
    setSaveError(null);
    await flushRef.current();
  }, [reload]);

  const unsaved = form.items.some((item) => confirmedRef.current.get(item.id) !== fingerprintOf(item, drafts[item.id] ?? draftOf(item)));
  const state: SaveState = saveError ? 'error' : save.isPending ? 'saving' : unsaved ? 'unsaved' : 'saved';
  return { drafts, update, flush, retry, reloadKeepingAnswers, state, saveError, conflict, revision: () => revisionRef.current, status: () => statusRef.current };
}
