import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { ArrowDown, ArrowUp, Check, CircleHelp, Clock3, FileCheck2, LoaderCircle, RotateCcw, Send, ShieldCheck, Sparkles } from 'lucide-react';

import { ModuleCheckpoint } from '@/features/learning/assessment/ModuleCheckpoint';
import {
  useAcceptLearningFollowUp,
  useDismissLearningFollowUp,
  useInterruptLearningAssessmentForm,
  useLearningAssessmentForm,
  useLearningAssessmentWorkspace,
  useSaveLearningAssessmentResponse,
  useStartLearningAssessmentForm,
  useSubmitLearningAssessmentForm,
} from '@/features/learning/assessment/useLearningAssessment';
import type {
  LearningProgramDto,
  LearningModuleDto,
  LearningAssessmentFormDto,
  LearningAssessmentFormSummaryDto,
  LearningAssessmentPurpose,
  LearningAssessmentWorkspaceDto,
  LearningEvidenceDimension,
  LearningFollowUpRecommendationDto,
  SaveLearningAssessmentResponseRequestDto,
} from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';


const purposes: { id: LearningAssessmentPurpose; label: string; description: string }[] = [
  { id: 'practice', label: 'Practice', description: 'Short retrieval and application sessions.' },
  { id: 'checkpoint', label: 'Checkpoint', description: 'Pause at a useful point and check what is sticking.' },
  { id: 'module_test', label: 'Module', description: 'A broader check across this program section.' },
  { id: 'cumulative', label: 'Cumulative', description: 'Bring together ideas from across the program.' },
  { id: 'transfer', label: 'Transfer', description: 'Use the ideas in a new situation.' },
];

const dimensionLabels: Record<LearningEvidenceDimension, string> = {
  recall: 'Recall', explanation: 'Explanation', application: 'Application', transfer: 'Transfer',
};
const purposeNames: Record<LearningAssessmentPurpose, string> = {
  practice: 'Practice', checkpoint: 'Checkpoint', module_test: 'Module assessment', cumulative: 'Cumulative', transfer: 'Transfer',
};
const statusCopy = {
  deterministic: 'Deterministic result', provisional: 'Provisional feedback', uncertain: 'Uncertain result', needs_review: 'Needs review',
} as const;

function uuid() {
  return globalThis.crypto?.randomUUID?.() ?? '00000000-0000-4000-8000-000000000001';
}
function validDate(value?: number | null) {
  if (value == null) return 'Date unavailable';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'Date unavailable' : date.toLocaleString();
}
function unwrapError(error: unknown) { return error instanceof Error ? error.message : 'The request could not be completed.'; }
function isConflict(error: unknown) { return /changed|revision|conflict|reload and retry/i.test(unwrapError(error)); }
function hasResponse(item: LearningAssessmentFormDto['items'][number]) {
  if (item.format === 'multiple_choice') return Number.isInteger(item.selectedIndex);
  if (item.format === 'ordering') return item.orderedValues.length > 0;
  if (item.format === 'artifact') return Boolean(item.artifactJson && JSON.stringify(item.artifactJson) !== '{}');
  return Boolean(item.textResponse?.trim());
}
function responseFingerprint(value: SaveLearningAssessmentResponseRequestDto['response']) {
  return JSON.stringify({ selectedIndex: value.selectedIndex, textResponse: value.text, orderedValues: value.orderedValues, artifactJson: value.artifactJson });
}

function FormAnswers({ form, programId, onExit }: { form: LearningAssessmentFormDto; programId: string; onExit: () => void }) {
  const formQuery = useLearningAssessmentForm(form.id);
  const save = useSaveLearningAssessmentResponse();
  const interrupt = useInterruptLearningAssessmentForm();
  const submit = useSubmitLearningAssessmentForm();
  const [answers, setAnswers] = useState<Record<string, LearningAssessmentFormDto['items'][number]>>({});
  const [confirm, setConfirm] = useState<'submit' | 'interrupt' | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const revisionRef = useRef(form.revision);
  const answerRef = useRef(answers);
  const confirmedRef = useRef(new Map<string, string>());
  const retryRef = useRef<SaveLearningAssessmentResponseRequestDto | null>(null);
  const actionOperationIds = useRef(new Map<string, string>());
  const requestByFingerprintRef = useRef(new Map<string, SaveLearningAssessmentResponseRequestDto>());
  const flightRef = useRef<Promise<boolean> | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    revisionRef.current = form.revision;
    const initial: Record<string, LearningAssessmentFormDto['items'][number]> = {};
    for (const item of form.items) initial[item.id] = item;
    setAnswers(initial);
    answerRef.current = initial;
    confirmedRef.current = new Map(form.items.map((item) => [item.id, JSON.stringify({ selectedIndex: item.selectedIndex, textResponse: item.textResponse, orderedValues: item.orderedValues, artifactJson: item.artifactJson })]));
    retryRef.current = null;
    requestByFingerprintRef.current.clear();
  // A new immutable form is the only reason to rehydrate local response state.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [form.id]);

  const fingerprint = (item: LearningAssessmentFormDto['items'][number]) => JSON.stringify({ selectedIndex: item.selectedIndex, textResponse: item.textResponse, orderedValues: item.orderedValues, artifactJson: item.artifactJson });
  const flush = useCallback(async () => {
    if (form.status !== 'active') return true;
    if (flightRef.current) return flightRef.current;
    const flight = (async () => {
      for (const item of form.items) {
        while (true) {
          const current = answerRef.current[item.id] ?? item;
          const value = fingerprint(current);
          if (confirmedRef.current.get(item.id) === value) break;
          const opKey = `${form.id}:${item.id}:${value}`;
          let request = requestByFingerprintRef.current.get(opKey);
          if (!request) {
            const response = {
              itemId: item.id,
              selectedIndex: current.selectedIndex,
              text: current.textResponse,
              orderedValues: current.orderedValues,
              artifactJson: current.artifactJson,
            };
            request = { operationId: uuid(), programId, formId: form.id, expectedRevision: revisionRef.current, response, assistance: [] };
            requestByFingerprintRef.current.set(opKey, request);
          }
          retryRef.current = request;
          try {
            const saved = await save.mutateAsync(request);
            if (saved.revision !== request.expectedRevision + 1) {
              setConflict(true);
              setSaveError('This assessment changed elsewhere. Reload the saved form before continuing. Your current answers remain here.');
              return false;
            }
            revisionRef.current = saved.revision;
            confirmedRef.current.set(item.id, value);
            retryRef.current = null;
            setSaveError(null);
            setConflict(false);
            // Mark the new operation base revision for later, changed payloads.
            for (const [key, existing] of requestByFingerprintRef.current) {
              if (key.startsWith(`${form.id}:${item.id}:`) && existing !== request) requestByFingerprintRef.current.delete(key);
            }
          } catch (error) {
            setSaveError(unwrapError(error));
            setConflict(isConflict(error));
            return false;
          }
        }
      }
      return true;
    })();
    flightRef.current = flight;
    try { return await flight; }
    finally { if (flightRef.current === flight) flightRef.current = null; }
  }, [form, programId, save]);

  const flushRef = useRef(flush);
  flushRef.current = flush;
  useEffect(() => registerPendingSave(() => flushRef.current()), [form.id]);

  useEffect(() => {
    if (form.status !== 'active') return;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => { void flushRef.current(); }, 650);
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [answers, form.status, form.id]);

  const update = (itemId: string, patch: Partial<LearningAssessmentFormDto['items'][number]>) => {
    setAnswers((current) => {
      const next = { ...current, [itemId]: { ...current[itemId], ...patch } };
      answerRef.current = next;
      return next;
    });
    setSaveError(null);
  };
  const retrySave = async () => {
    const request = retryRef.current;
    if (!request) return flush();
    try {
      const saved = await save.mutateAsync(request);
      if (saved.revision !== request.expectedRevision + 1) throw new Error('Assessment changed; reload and retry.');
      revisionRef.current = saved.revision;
      confirmedRef.current.set(request.response.itemId, responseFingerprint(request.response));
      retryRef.current = null;
      setSaveError(null);
      setConflict(false);
      return true;
    } catch (error) { setSaveError(unwrapError(error)); setConflict(isConflict(error)); return false; }
  };
  const reloadBase = async () => {
    const refreshed = await formQuery.refetch();
    if (!refreshed.data) return;
    revisionRef.current = refreshed.data.revision;
    confirmedRef.current = new Map(refreshed.data.items.map((item) => [item.id, fingerprint(item)]));
    retryRef.current = null;
    requestByFingerprintRef.current.clear();
    setConflict(false);
    setSaveError('The latest revision is loaded. Your current answers are preserved; save again to apply them to this revision.');
  };
  const exit = async () => {
    if (await flushRef.current()) onExit();
  };
  const answered = form.items.filter((item) => hasResponse(answers[item.id] ?? item)).length;
  const pending = save.isPending || submit.isPending || interrupt.isPending;
  const actionId = (action: string) => {
    const key = `${form.id}:${action}:${revisionRef.current}`;
    let id = actionOperationIds.current.get(key);
    if (!id) { id = uuid(); actionOperationIds.current.set(key, id); }
    return id;
  };
  const doSubmit = async () => {
    if (!(await flushRef.current())) return;
    try {
      await submit.mutateAsync({ operationId: actionId('submit'), programId, formId: form.id, expectedRevision: revisionRef.current });
      setConfirm(null);
    } catch (error) { setSaveError(unwrapError(error)); setConflict(isConflict(error)); }
  };
  const doInterrupt = async () => {
    if (!(await flushRef.current())) return;
    try {
      await interrupt.mutateAsync({ operationId: actionId('interrupt'), programId, formId: form.id, expectedRevision: revisionRef.current });
      setConfirm(null);
      onExit();
    } catch (error) { setSaveError(unwrapError(error)); setConflict(isConflict(error)); }
  };

  if (formQuery.isError) return <section className="rounded-2xl border border-rose-500/25 bg-surface p-6"><h3 className="font-serif text-xl">Could not reopen this assessment</h3><p role="alert" className="mt-2 text-sm text-text-secondary">{unwrapError(formQuery.error)}</p><button type="button" onClick={() => void formQuery.refetch()} className="mt-4 rounded-full border px-4 py-2 text-sm">Retry</button></section>;

  return <section className="overflow-hidden rounded-2xl border border-border bg-surface shadow-sm">
    <header className="border-b border-border bg-background/55 px-5 py-5 sm:px-7"><div className="flex flex-wrap items-start justify-between gap-4"><div className="min-w-0"><button type="button" onClick={() => void exit()} disabled={pending} className="text-xs text-text-muted hover:text-accent">← Assessment hub</button><div className="mt-3 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[.15em] text-accent"><FileCheck2 size={14} /> {purposeNames[form.purpose]} <span className="text-text-muted">· Revision {form.blueprintRevision}</span></div><h2 className="mt-1 font-serif text-2xl leading-tight text-text-primary">{form.title}</h2><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">{form.instructions}</p></div><div className="rounded-xl border border-border bg-surface px-4 py-3 text-xs text-text-secondary"><div className="flex items-center gap-2"><Clock3 size={14} className="text-accent" /> {answered} of {form.items.length} answered</div><div role="status" aria-live="polite" className="mt-2 text-[10px] text-text-muted">{save.isPending ? 'Saving responses…' : saveError ? 'Changes need attention' : 'Autosaved as you go'}</div></div></div></header>
    {form.status === 'active' ? <div className="p-4 sm:p-7">
      <div className="space-y-4">{form.items.map((item, index) => {
        const value = answers[item.id] ?? item;
        return <article key={item.id} className="rounded-2xl border border-border bg-background/45 p-4 sm:p-6"><div className="flex flex-wrap items-center gap-2"><span className="rounded-full bg-accent/10 px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-accent">Item {index + 1} · {item.format.replace('_', ' ')}</span>{item.previouslyExposed && <span className="rounded-full bg-amber-500/10 px-2.5 py-1 text-[10px] font-medium text-amber-800 dark:text-amber-200">Exposed repeat</span>}</div><h3 className="mt-4 font-serif text-xl leading-7 text-text-primary">{item.prompt}</h3>
          {item.format === 'multiple_choice' ? <fieldset className="mt-4 space-y-2" disabled={pending}><legend className="sr-only">Select one answer</legend>{item.options.map((option, optionIndex) => <label key={`${item.id}-${optionIndex}`} className={`flex cursor-pointer gap-3 rounded-xl border px-4 py-3 text-sm leading-6 transition ${value.selectedIndex === optionIndex ? 'border-accent bg-accent/5' : 'border-border bg-surface hover:border-accent/45'}`}><input aria-label={option} type="radio" name={`answer-${item.id}`} checked={value.selectedIndex === optionIndex} onChange={() => update(item.id, { selectedIndex: optionIndex })} className="mt-1 accent-accent" /><span>{option}</span></label>)}</fieldset> : null}
          {item.format === 'short_answer' || item.format === 'explanation' ? <label className="mt-4 block"><span className="sr-only">Your {item.format === 'explanation' ? 'explanation' : 'answer'}</span><textarea aria-label={item.format === 'explanation' ? 'Your explanation' : 'Your answer'} disabled={pending} rows={item.format === 'explanation' ? 5 : 3} value={value.textResponse ?? ''} onChange={(event) => update(item.id, { textResponse: event.target.value })} placeholder={item.format === 'explanation' ? 'Explain your reasoning in your own words…' : 'Write a concise response…'} className="w-full resize-y rounded-xl border border-border bg-surface px-4 py-3 text-sm leading-6 text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/15 disabled:opacity-60" /></label> : null}
          {item.format === 'ordering' ? <div className="mt-4 space-y-2" aria-label="Order the items">{(value.orderedValues.length ? value.orderedValues : item.options).map((entry, entryIndex, array) => <div key={`${item.id}-${entry}`} className="flex items-center gap-3 rounded-xl border border-border bg-surface px-3 py-2.5"><span className="grid h-7 w-7 shrink-0 place-items-center rounded-full bg-accent/10 text-xs font-semibold text-accent">{entryIndex + 1}</span><span className="min-w-0 flex-1 text-sm text-text-secondary">{entry}</span><button type="button" aria-label={`Move ${entry} up`} disabled={pending || entryIndex === 0} onClick={() => { const ordered = [...array]; [ordered[entryIndex - 1], ordered[entryIndex]] = [ordered[entryIndex], ordered[entryIndex - 1]]; update(item.id, { orderedValues: ordered }); }} className="rounded p-1.5 text-text-muted hover:bg-background disabled:opacity-30"><ArrowUp size={15} /></button><button type="button" aria-label={`Move ${entry} down`} disabled={pending || entryIndex === array.length - 1} onClick={() => { const ordered = [...array]; [ordered[entryIndex], ordered[entryIndex + 1]] = [ordered[entryIndex + 1], ordered[entryIndex]]; update(item.id, { orderedValues: ordered }); }} className="rounded p-1.5 text-text-muted hover:bg-background disabled:opacity-30"><ArrowDown size={15} /></button></div>)}</div> : null}
          {item.format === 'artifact' ? <div className="mt-4"><label className="block text-xs font-medium text-text-secondary" htmlFor={`artifact-${item.id}`}>{item.artifactKind || 'Artifact'} · describe or paste your work</label><textarea id={`artifact-${item.id}`} disabled={pending} rows={7} value={typeof value.artifactJson === 'object' && value.artifactJson && 'text' in value.artifactJson ? String(value.artifactJson.text ?? '') : ''} onChange={(event) => update(item.id, { artifactJson: { text: event.target.value } })} placeholder="Describe your artifact or paste a text representation. The saved response is submitted for assessment." className="mt-2 w-full resize-y rounded-xl border border-border bg-surface px-4 py-3 font-mono text-xs leading-6 outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/15 disabled:opacity-60" /><p className="mt-2 text-[10px] leading-4 text-text-muted">Your artifact content is part of your response. Do not include secrets or private credentials.</p></div> : null}
          {item.rubric.length > 0 && <details className="mt-4 rounded-xl border border-border bg-surface px-4 py-3"><summary className="cursor-pointer text-xs font-medium text-text-secondary">What this item looks for</summary><ul className="mt-3 space-y-2">{item.rubric.map((criterion) => <li key={criterion.id} className="text-xs leading-5 text-text-muted"><span className="font-medium text-text-secondary">{criterion.title}</span> · {criterion.description} <span className="whitespace-nowrap">({criterion.maxPoints} pts)</span></li>)}</ul></details>}
        </article>;
      })}</div>
      <div className="mt-5 flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-border bg-surface p-4 sm:p-5"><p className="max-w-2xl text-xs leading-5 text-text-muted">Responses save as you work. Submission freezes this attempt; it cannot be edited afterward. Results may be provisional or uncertain for open responses.</p><div className="flex flex-wrap gap-2"><button type="button" disabled={pending} onClick={() => setConfirm('interrupt')} className="rounded-full border border-border px-4 py-2.5 text-xs font-medium text-text-secondary hover:bg-background disabled:opacity-50">Pause attempt</button><button type="button" disabled={pending || answered !== form.items.length} onClick={() => setConfirm('submit')} className="inline-flex items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-xs font-semibold text-accent-fg disabled:cursor-not-allowed disabled:opacity-45"><Send size={14} /> Submit assessment</button></div></div>
      {saveError && <div role="alert" className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-rose-500/20 bg-rose-500/5 p-4 text-sm text-rose-800 dark:text-rose-200"><span>{saveError}</span><div className="flex gap-2">{conflict ? <button type="button" onClick={() => void reloadBase()} className="rounded-full border border-current/25 px-3 py-2 text-xs">Reload revision · keep my answers</button> : <button type="button" onClick={() => void retrySave()} className="inline-flex items-center gap-1 rounded-full bg-accent px-3 py-2 text-xs font-semibold text-accent-fg"><RotateCcw size={12} /> Retry same save</button>}</div></div>}
    </div> : <SubmittedForm form={form} />}
    {confirm && <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4" role="presentation"><section role="alertdialog" aria-modal="true" aria-labelledby="assessment-confirm-title" className="w-full max-w-md rounded-2xl border border-border bg-surface p-6 shadow-2xl"><h3 id="assessment-confirm-title" className="font-serif text-xl text-text-primary">{confirm === 'submit' ? 'Submit this assessment?' : 'Pause this attempt?'}</h3><p className="mt-2 text-sm leading-6 text-text-secondary">{confirm === 'submit' ? 'Your responses will be frozen and graded. You can start a clearly labeled retake later.' : 'Your saved responses will remain available to resume. This attempt will not be graded.'}</p><div className="mt-5 flex justify-end gap-2"><button type="button" autoFocus onClick={() => setConfirm(null)} className="rounded-full border border-border px-4 py-2.5 text-sm">Keep working</button><button type="button" disabled={pending} onClick={() => void (confirm === 'submit' ? doSubmit() : doInterrupt())} className="rounded-full bg-accent px-4 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50">{pending ? 'Saving…' : confirm === 'submit' ? 'Submit now' : 'Pause attempt'}</button></div></section></div>}
  </section>;
}

function SubmittedForm({ form }: { form: LearningAssessmentFormDto }) {
  const result = form.submission;
  if (form.status === 'interrupted') return <div className="p-6"><p className="rounded-xl bg-amber-500/10 p-4 text-sm leading-6 text-text-secondary">This attempt was paused. It was not submitted for grading.</p></div>;
  if (!result) return <div className="p-6"><p className="text-sm text-text-muted">Submitted result is not available yet.</p></div>;
  return <div className="p-4 sm:p-7"><section className="rounded-2xl border border-accent/20 bg-accent/5 p-5 sm:p-7"><div className="flex flex-wrap items-start justify-between gap-4"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">{statusCopy[result.gradeStatus]}</div><h3 className="mt-1 font-serif text-3xl text-text-primary">{(result.score * 100).toFixed(0)}<span className="text-lg text-text-muted">%</span></h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">{result.feedback}</p></div><span className="rounded-full border border-border bg-surface px-3 py-1.5 text-xs text-text-secondary">Model-authored assessment</span></div>{result.graderDisagreement.length > 0 && <div className="mt-5 rounded-xl border border-amber-500/25 bg-amber-500/5 p-4"><h4 className="text-xs font-semibold text-text-primary">Review notes</h4><ul className="mt-2 list-disc space-y-1 pl-5 text-xs leading-5 text-text-secondary">{result.graderDisagreement.map((note, index) => <li key={`${index}-${note}`}>{note}</li>)}</ul></div>}</section><div className="mt-5 space-y-3">{form.items.map((item, index) => { const itemResult = result.itemResults.find((entry) => entry.itemId === item.id); return <article key={item.id} className="rounded-xl border border-border bg-background/40 p-4 sm:p-5"><div className="flex flex-wrap items-center justify-between gap-2"><span className="text-xs font-semibold text-text-primary">Item {index + 1}</span><span className="rounded-full bg-surface px-2.5 py-1 text-[10px] text-text-muted">{itemResult ? statusCopy[itemResult.gradeStatus] : 'Feedback unavailable'}</span></div><h4 className="mt-2 font-serif text-lg leading-6 text-text-primary">{item.prompt}</h4>{itemResult && <><p className="mt-3 text-sm leading-6 text-text-secondary">{itemResult.feedback}</p>{itemResult.criterionResults.length > 0 && <div className="mt-4 space-y-2">{itemResult.criterionResults.map((criterion) => <div key={criterion.criterionId} className="rounded-lg border border-border bg-surface p-3"><div className="flex flex-wrap justify-between gap-2 text-xs"><span className="font-medium text-text-primary">{form.rubric.find((r) => r.id === criterion.criterionId)?.title ?? 'Criterion'}</span><span className="text-text-muted">{criterion.score == null ? 'Not scored' : `${criterion.score} / ${criterion.maxPoints}`}</span></div><p className="mt-1 text-xs leading-5 text-text-secondary">{criterion.observation}</p>{criterion.artifactQuote && <blockquote className="mt-2 border-l-2 border-accent/50 pl-3 text-xs italic text-text-muted">{criterion.artifactQuote}</blockquote>}</div>)}</div>}{itemResult.artifactQuotes.map((quote, i) => <blockquote key={`${item.id}-${i}`} className="mt-3 border-l-2 border-accent/50 pl-3 text-xs italic text-text-muted">{quote}</blockquote>)}</>}</article>; })}</div><p className="mt-5 text-xs leading-5 text-text-muted">Results are evidence from this attempt, not a claim of mastery. Submitted {validDate(result.submittedAt)}.</p></div>;
}

function FollowUps({ workspace }: { workspace: LearningAssessmentWorkspaceDto }) {
  const accept = useAcceptLearningFollowUp();
  const dismiss = useDismissLearningFollowUp();
  const requestRef = useRef(new Map<string, { operationId: string; programId: string; followUpId: string }>());
  const [retryTarget, setRetryTarget] = useState<{ item: LearningFollowUpRecommendationDto; kind: 'accept' | 'dismiss' } | null>(null);
  const busyId = accept.isPending ? accept.variables?.followUpId : dismiss.variables?.followUpId;
  const pending = workspace.followUps.filter((item) => item.status === 'pending');
  const outcomeName = (id?: string | null) => workspace.outcomes.find((outcome) => outcome.id === id)?.title;
  const reasonText: Record<LearningFollowUpRecommendationDto['reasonCode'], string> = { missed_outcome: 'Missed outcome', assisted_success: 'Assisted success', low_transfer: 'Low transfer', stale_evidence: 'Stale evidence', uncertain_grade: 'Uncertain grade' };
  const actionText: Record<LearningFollowUpRecommendationDto['actionKind'], string> = { lesson: 'Review lesson', practice: 'Try practice', assessment: 'Take a check', recall: 'Recall review', practical: 'Apply it' };
  const decide = async (item: LearningFollowUpRecommendationDto, kind: 'accept' | 'dismiss') => {
    const key = `${kind}:${item.id}`;
    let request = requestRef.current.get(key);
    if (!request) { request = { operationId: uuid(), programId: workspace.programId, followUpId: item.id }; requestRef.current.set(key, request); }
    try {
      await (kind === 'accept' ? accept : dismiss).mutateAsync(request);
      requestRef.current.delete(key);
      setRetryTarget(null);
    } catch { setRetryTarget({ item, kind }); }
  };
      return <section className="rounded-2xl border border-border bg-surface p-5 sm:p-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Next steps</div><h3 className="mt-1 font-serif text-xl text-text-primary">Evidence-based follow-ups</h3></div><span className="rounded-full bg-background px-3 py-1.5 text-xs text-text-muted">{pending.length} pending</span></div>{pending.length === 0 ? <p className="mt-4 rounded-xl bg-background px-4 py-5 text-sm text-text-muted">No pending recommendations. New evidence may suggest a next step later.</p> : <div className="mt-4 space-y-3">{pending.map((item) => <article key={item.id} className="rounded-xl border border-border bg-background/45 p-4"><div className="flex flex-wrap items-center gap-2"><span className="rounded-full bg-accent/10 px-2.5 py-1 text-[10px] font-semibold text-accent">{reasonText[item.reasonCode]}</span>{outcomeName(item.outcomeId) && <span className="text-xs text-text-muted">{outcomeName(item.outcomeId)}</span>}</div><p className="mt-3 text-sm leading-6 text-text-secondary">{item.explanation}</p><div className="mt-4 flex flex-wrap items-center justify-between gap-3"><span className="text-xs font-medium text-text-primary">Suggested: {actionText[item.actionKind]}</span><div className="flex gap-2"><button type="button" disabled={Boolean(busyId)} onClick={() => void decide(item, 'dismiss')} className="rounded-full border border-border px-3.5 py-2 text-xs disabled:opacity-50">Dismiss</button><button type="button" disabled={Boolean(busyId)} onClick={() => void decide(item, 'accept')} className="rounded-full bg-accent px-3.5 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50">{busyId === item.id ? 'Saving…' : 'Accept suggestion'}</button></div></div></article>)}</div>}{retryTarget && <p role="alert" className="mt-3 flex flex-wrap items-center gap-2 text-sm text-rose-700">{unwrapError(accept.error ?? dismiss.error)}<button type="button" onClick={() => void decide(retryTarget.item, retryTarget.kind)} className="font-semibold underline">Retry same decision</button></p>}</section>;
}

export function AssessmentEvidencePanel({ programId, program, module, requestedFormId }: { programId: string; program?: LearningProgramDto; module?: LearningModuleDto; requestedFormId?: string | null }) {
  const workspace = useLearningAssessmentWorkspace(programId);
  const start = useStartLearningAssessmentForm();
  const [purpose, setPurpose] = useState<LearningAssessmentPurpose>('practice');
  const [selectedFormId, setSelectedFormId] = useState<string | null>(requestedFormId ?? null);
  useEffect(() => { if (requestedFormId) setSelectedFormId(requestedFormId); }, [requestedFormId]);
  const [startError, setStartError] = useState<string | null>(null);
  const [startRetryTarget, setStartRetryTarget] = useState<{ blueprintId: string; retakeOfFormId: string | null } | null>(null);
  const startRequestRef = useRef<{ key: string; request: Parameters<typeof start.mutateAsync>[0] } | null>(null);
  const currentForm = useLearningAssessmentForm(selectedFormId);
  const acceptedBlueprints = useMemo(() => (workspace.data?.blueprints ?? []).filter((blueprint) => blueprint.status === 'accepted'), [workspace.data?.blueprints]);
  const active = workspace.data?.forms.filter((form) => form.status === 'active') ?? [];
  const available = acceptedBlueprints.filter((blueprint) => blueprint.purpose === purpose);
  const purposeForms = (workspace.data?.forms ?? []).filter((form) => form.purpose === purpose);
  const startForm = async (blueprintId: string, retakeOfFormId: string | null = null) => {
    const blueprint = acceptedBlueprints.find((item) => item.id === blueprintId);
    if (!blueprint) return;
    setStartError(null);
    const key = `${blueprint.id}:${blueprint.revision}:${retakeOfFormId ?? 'new'}`;
    let request = startRequestRef.current?.key === key ? startRequestRef.current.request : null;
    if (!request) {
      request = { operationId: uuid(), formId: uuid(), programId, blueprintId: blueprint.id, blueprintRevision: blueprint.revision, retakeOfFormId };
      startRequestRef.current = { key, request };
    }
    try {
      const created = await start.mutateAsync(request);
      setSelectedFormId(created.id);
      startRequestRef.current = null;
      setStartRetryTarget(null);
    } catch (error) { setStartError(unwrapError(error)); setStartRetryTarget({ blueprintId, retakeOfFormId }); }
  };
  const followups = workspace.data ? <FollowUps workspace={workspace.data} /> : null;
  const events = workspace.data?.evidence ?? [];
  const outcomes = workspace.data?.outcomes ?? [];
  const timeline = (['recall', 'explanation', 'application', 'transfer'] as LearningEvidenceDimension[]).map((dimension) => ({ dimension, items: events.filter((event) => event.dimension === dimension).sort((a, b) => b.observedAt - a.observedAt) }));

  if (workspace.isLoading) return <div role="status" className="rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted"><LoaderCircle className="mr-2 inline animate-spin" size={16} />Loading assessments and evidence…</div>;
  if (workspace.isError || !workspace.data) return <div className="rounded-2xl border border-rose-500/20 bg-surface p-6"><h3 className="font-serif text-xl">Assessment hub unavailable</h3><p role="alert" className="mt-2 text-sm text-text-secondary">{unwrapError(workspace.error)}</p><button type="button" onClick={() => void workspace.refetch()} className="mt-4 rounded-full border px-4 py-2 text-sm">Retry</button></div>;

  if (selectedFormId && currentForm.data) return <div className="space-y-4"><FormAnswers form={currentForm.data} programId={programId} onExit={() => setSelectedFormId(null)} /></div>;

  return <div className="space-y-5" data-testid="assessment-evidence-panel">
    {program && module && <ModuleCheckpoint key={module.id} program={program} module={module} workspace={workspace.data} onCreated={() => setPurpose('module_test')} />}
    {active.length > 0 && <section className="rounded-2xl border border-accent/25 bg-accent/5 p-5"><div className="flex flex-wrap items-center justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Continue where you left off</div><h2 className="mt-1 font-serif text-xl text-text-primary">Active assessments</h2></div><span className="rounded-full bg-surface px-3 py-1.5 text-xs text-text-muted">{active.length} in progress</span></div><div className="mt-4 grid gap-3 sm:grid-cols-2">{active.map((form) => <button key={form.id} type="button" onClick={() => setSelectedFormId(form.id)} className="rounded-xl border border-border bg-surface p-4 text-left transition hover:border-accent/45"><span className="text-[10px] font-semibold uppercase tracking-[.14em] text-accent">{purposeNames[form.purpose]} · Autosaved</span><span className="mt-1 block font-serif text-lg text-text-primary">{form.title}</span><span className="mt-2 block text-xs text-text-muted">Revision {form.revision} · Started {validDate(form.createdAt)}</span><span className="mt-3 inline-flex items-center gap-1 text-xs font-semibold text-accent">Resume <Check size={13} /></span></button>)}</div></section>}

    <section className="overflow-hidden rounded-2xl border border-border bg-surface shadow-sm"><header className="relative overflow-hidden border-b border-border bg-[#eee8df] px-5 py-6 dark:bg-[#28251f] sm:px-7 sm:py-8"><div className="pointer-events-none absolute -right-12 -top-20 h-48 w-48 rounded-full border border-accent/20" /><div className="relative max-w-3xl"><div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[.17em] text-accent"><ShieldCheck size={14} /> Assess &amp; evidence</div><h2 className="mt-2 font-serif text-3xl leading-tight text-text-primary">A clearer picture than one score</h2><p className="mt-3 text-sm leading-6 text-text-secondary">Choose a purpose, resume a saved form, or review the evidence gathered across different kinds of work. Open responses may receive provisional or uncertain feedback.</p></div></header>
      <div className="grid gap-5 p-4 sm:p-6 xl:grid-cols-[minmax(0,1.2fr)_minmax(270px,.8fr)]"><div className="min-w-0"><div className="flex flex-wrap gap-2" role="group" aria-label="Assessment purpose">{purposes.map((item) => <button type="button" key={item.id} aria-pressed={purpose === item.id} onClick={() => setPurpose(item.id)} className={`rounded-full border px-3.5 py-2 text-xs font-medium transition ${purpose === item.id ? 'border-accent bg-accent text-accent-fg' : 'border-border bg-background text-text-secondary hover:border-accent/40'}`}>{item.label}</button>)}</div><p className="mt-3 text-xs leading-5 text-text-muted">{purposes.find((item) => item.id === purpose)?.description}</p>
        <div className="mt-5 rounded-2xl border border-border bg-background/40 p-4 sm:p-5"><div className="flex flex-wrap items-center justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Available forms</div><h3 className="mt-1 font-serif text-xl text-text-primary">{purposes.find((item) => item.id === purpose)?.label} assessments</h3></div><span className="rounded-full bg-surface px-3 py-1 text-[10px] text-text-muted">{purposeForms.length} past</span></div>
          {available.length === 0 ? <div className="mt-4 rounded-xl border border-dashed border-border bg-surface px-4 py-5"><CircleHelp size={17} className="text-text-muted" /><p className="mt-2 text-sm font-medium text-text-primary">No accepted {purposeNames[purpose].toLowerCase()} form yet</p><p className="mt-1 text-xs leading-5 text-text-muted">When an assessment blueprint is available, you can start a saved attempt here. No sample form is substituted.</p></div> : <div className="mt-4 space-y-3">{available.map((blueprint) => <article key={`${blueprint.id}:${blueprint.revision}`} className="rounded-xl border border-border bg-surface p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.12em] text-accent">Blueprint revision {blueprint.revision}</div><h4 className="mt-1 font-serif text-lg text-text-primary">{blueprint.title}</h4><p className="mt-1 text-xs leading-5 text-text-secondary">{blueprint.instructions}</p><span className="mt-3 inline-flex items-center gap-1 text-[10px] text-text-muted"><Clock3 size={12} /> {blueprint.requirements.reduce((sum, req) => sum + req.count, 0)} planned items</span></div><button type="button" disabled={start.isPending || active.length > 0} onClick={() => void startForm(blueprint.id)} className="inline-flex shrink-0 items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:cursor-not-allowed disabled:opacity-45"><Sparkles size={13} /> {start.isPending ? 'Starting…' : 'Start'}</button></div></article>)}</div>}
          {startError && <div role="alert" className="mt-3 rounded-lg bg-rose-500/10 p-3 text-xs text-rose-800 dark:text-rose-200">{startError}{startRetryTarget && <button type="button" onClick={() => void startForm(startRetryTarget.blueprintId, startRetryTarget.retakeOfFormId)} className="ml-2 font-semibold underline">Retry same start</button>}</div>}
          {purposeForms.length > 0 && <div className="mt-5 border-t border-border pt-4"><h4 className="text-xs font-semibold uppercase tracking-[.13em] text-text-muted">History</h4><div className="mt-2 divide-y divide-border">{purposeForms.slice().sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0)).map((form: LearningAssessmentFormSummaryDto) => { const acceptedBlueprint = acceptedBlueprints.find((item) => item.id === form.blueprintId); return <div key={form.id} className="flex flex-wrap items-center justify-between gap-3 py-3"><div className="min-w-0"><div className="truncate text-sm font-medium text-text-primary">{form.title}</div><div className="mt-1 text-[10px] text-text-muted">{form.status} · {form.gradeStatus ? statusCopy[form.gradeStatus] : 'Not graded'}{form.retakeOfFormId && ' · Retake'} · {validDate(form.submittedAt ?? form.createdAt)}</div></div><div className="flex items-center gap-2">{form.score != null && <span className="font-serif text-lg text-text-primary">{(form.score * 100).toFixed(0)}%</span>}{form.status === 'interrupted' && acceptedBlueprint && <button type="button" disabled={start.isPending || active.length > 0} onClick={() => void startForm(form.blueprintId, form.id)} className="rounded-full border border-border px-3 py-2 text-[10px] font-semibold text-text-secondary disabled:opacity-40">Start fresh attempt</button>}{form.status === 'submitted' && acceptedBlueprint && <button type="button" disabled={active.length > 0 || start.isPending} onClick={() => void startForm(form.blueprintId, form.id)} className="rounded-full border border-border px-3 py-2 text-[10px] font-semibold text-text-secondary disabled:opacity-40">Retake · exposed items</button>}</div></div>; })}</div></div>}
        </div>
      </div><div className="space-y-4"><section className="rounded-2xl border border-border bg-background/45 p-4 sm:p-5"><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Outcome evidence</div><h3 className="mt-1 font-serif text-xl text-text-primary">Four ways to observe learning</h3><p className="mt-2 text-xs leading-5 text-text-muted">Signals are recorded separately. Empty dimensions mean no evidence has been recorded, not that an outcome was missed.</p><div className="mt-4 space-y-3">{timeline.map(({ dimension, items }) => <div key={dimension} className="rounded-xl border border-border bg-surface p-3"><div className="flex items-baseline justify-between gap-2"><h4 className="text-xs font-semibold text-text-primary">{dimensionLabels[dimension]}</h4><span className="text-[10px] tabular-nums text-text-muted">{items.length} signal{items.length === 1 ? '' : 's'}</span></div>{items.length === 0 ? <p className="mt-1 text-[10px] text-text-muted">No recorded evidence yet</p> : <div className="mt-2 space-y-2">{items.slice(0, 3).map((event) => { const outcome = outcomes.find((entry) => entry.id === event.outcomeId); return <article key={event.id} className="border-t border-border pt-2 first:border-0 first:pt-0"><div className="flex flex-wrap justify-between gap-2 text-[10px]"><span className="font-medium text-text-secondary">{outcome?.title ?? event.sourceKind.replace('_', ' ')}</span><span className="capitalize text-text-muted">{event.result.replace('_', ' ')}</span></div><p className="mt-1 text-[10px] leading-4 text-text-muted">{event.observation || 'Evidence recorded'} · {validDate(event.observedAt)}</p>{event.evidenceQuote && <blockquote className="mt-1 border-l border-accent/50 pl-2 text-[10px] italic text-text-muted">{event.evidenceQuote}</blockquote>}</article>; })}</div>}</div>)}</div></section></div></div>
    </section>
    {followups}
    {selectedFormId && !currentForm.data && <div role="status" className="rounded-xl border border-border bg-surface p-5 text-sm text-text-muted">Loading saved assessment…</div>}
  </div>;
}
