import { useEffect, useRef, useState } from 'react';

import type { SubmitLearningDiagnosticRequestDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';

import { useLearningPlan, useSkipLearningDiagnostic, useStartLearningDiagnostic, useSubmitLearningDiagnostic } from './useLearningPlan';
import { useLearningProgram } from './useLearningStudio';
import { MarkdownViewer } from '../Chat/viewers/MarkdownViewer';

export function StartingPointPanel({ programId, onStudy, onChallenge }: { programId: string; onStudy?: (lessonId: string) => void; onChallenge?: (lessonId: string) => void }) {
  const plan = useLearningPlan(programId);
  const program = useLearningProgram(programId);
  const start = useStartLearningDiagnostic();
  const submit = useSubmitLearningDiagnostic();
  const skip = useSkipLearningDiagnostic();
  const diagnostic = plan.data?.latestDiagnostic;
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [error, setError] = useState('');
  const [conflict, setConflict] = useState(false);
  const [saving, setSaving] = useState(false);
  const values = useRef(answers);
  const saved = useRef('');
  const revision = useRef(0);
  const hydrated = useRef('');
  const flight = useRef<Promise<boolean> | null>(null);
  const retry = useRef<SubmitLearningDiagnosticRequestDto | null>(null);
  const startId = useRef(crypto.randomUUID());
  const skipId = useRef(crypto.randomUUID());
  const finalRequest = useRef<SubmitLearningDiagnosticRequestDto | null>(null);
  const active = diagnostic?.status === 'active';
  const expectedRevision = program.data?.summary.revision;

  useEffect(() => {
    if (!diagnostic || hydrated.current === diagnostic.id) return;
    const initial = Object.fromEntries(diagnostic.responses.map((r) => [r.promptId, r.response]));
    hydrated.current = diagnostic.id;
    revision.current = diagnostic.revision ?? 0;
    saved.current = JSON.stringify(initial);
    values.current = initial;
    setAnswers(initial);
  }, [diagnostic]);

  const flush = async (): Promise<boolean> => {
    if (!active || !diagnostic || expectedRevision === undefined) return true;
    if (conflict) return false;
    if (flight.current) return flight.current;
    const work = (async () => {
      setSaving(true);
      try {
        while (JSON.stringify(values.current) !== saved.current) {
          const snapshot = { ...values.current };
          const request = retry.current ?? {
            operationId: crypto.randomUUID(), programId, diagnosticId: diagnostic.id, expectedRevision,
            expectedDiagnosticRevision: revision.current, saveOnly: true,
            responses: Object.entries(snapshot).map(([promptId, response]) => ({ promptId, response })),
          };
          retry.current = request;
          const result = await submit.mutateAsync(request);
          revision.current = result.revision ?? revision.current + 1;
          saved.current = JSON.stringify(Object.fromEntries(request.responses.map((r) => [r.promptId, r.response])));
          retry.current = null;
          setError('');
        }
        return true;
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        setError(message);
        setConflict(/changed elsewhere|reload|revision/i.test(message));
        return false;
      } finally { setSaving(false); }
    })();
    flight.current = work;
    try { return await work; } finally { flight.current = null; }
  };
  const flushRef = useRef(flush);
  flushRef.current = flush;
  useEffect(() => registerPendingSave(() => flushRef.current()), [diagnostic?.id]);
  useEffect(() => {
    if (!active || error) return;
    const timer = setTimeout(() => { void flushRef.current(); }, 700);
    return () => clearTimeout(timer);
  }, [answers, active, error]);

  const finish = async () => {
    if (!diagnostic || expectedRevision === undefined || !await flush()) return;
    finalRequest.current ??= {
      operationId: crypto.randomUUID(), programId, diagnosticId: diagnostic.id, expectedRevision,
      expectedDiagnosticRevision: revision.current, saveOnly: false,
      responses: diagnostic.prompts.map((p) => ({ promptId: p.id, response: values.current[p.id] ?? '' })),
    };
    try { await submit.mutateAsync(finalRequest.current); finalRequest.current = null; setError(''); }
    catch (cause) { const message = cause instanceof Error ? cause.message : String(cause); setError(message); setConflict(/changed|reload|revision/i.test(message)); }
  };
  const reload = async (keep: boolean) => {
    const [refreshed] = await Promise.all([plan.refetch(), program.refetch()]);
    const latest = refreshed.data?.latestDiagnostic;
    if (!latest || latest.id !== diagnostic?.id) return;
    revision.current = latest.revision ?? 0;
    const persisted = Object.fromEntries(latest.responses.map((r) => [r.promptId, r.response]));
    saved.current = JSON.stringify(persisted);
    if (!keep) { values.current = persisted; setAnswers(persisted); }
    retry.current = null; finalRequest.current = null; setConflict(false); setError('');
  };

  if (plan.isLoading || program.isLoading) return <p role="status">Loading your starting point…</p>;
  if (plan.isError || program.isError || !program.data) return <div role="alert"><p>Could not load your course.</p><button type="button" onClick={() => { void plan.refetch(); void program.refetch(); }} className="min-h-11 text-accent">Retry starting point</button></div>;
  const busy = start.isPending || skip.isPending || submit.isPending;
  return <section className="rounded-2xl border border-border bg-surface p-5 sm:p-7" aria-label="Starting-point assessment">
    <p className="text-xs font-semibold uppercase tracking-wide text-accent">Your starting point · optional</p><h3 className="mt-2 font-serif text-2xl text-text-primary">Begin at the right level</h3>
    <p className="mt-3 max-w-3xl text-sm leading-6 text-text-secondary">Try a few short tasks from across the course. Show your reasoning or say where you get stuck. You’ll get a suggested starting point and specific practice to work on.</p>
    {!diagnostic || diagnostic.status === 'skipped' ? <div className="mt-5 flex flex-wrap gap-3"><button type="button" disabled={busy} onClick={() => start.mutate({ operationId: startId.current, programId, expectedRevision: expectedRevision! })} className="min-h-11 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg disabled:opacity-50">{start.isPending ? 'Preparing short tasks…' : 'Check my starting point'}</button>{!diagnostic && <button type="button" disabled={busy} onClick={() => skip.mutate({ operationId: skipId.current, programId, expectedRevision: expectedRevision! })} className="min-h-11 rounded-full border border-border px-4 text-sm">Start from the foundations</button>}</div> : active ? <>
      <div className="mt-6 space-y-5">{diagnostic.prompts.map((prompt, index) => <div key={prompt.id} className="rounded-xl border border-border bg-background/50 p-4"><p className="mb-2 text-xs font-semibold text-accent">Task {index + 1} · {prompt.outcomeTitle}</p><div className="prose prose-sm max-w-none dark:prose-invert"><MarkdownViewer content={prompt.prompt} /></div><label htmlFor={`placement-${prompt.id}`} className="mt-4 block text-sm font-medium">Your reasoning</label><textarea id={`placement-${prompt.id}`} rows={4} maxLength={4000} value={answers[prompt.id] ?? ''} disabled={busy && !saving} onChange={(event) => { const next = { ...values.current, [prompt.id]: event.target.value }; values.current = next; setAnswers(next); finalRequest.current = null; }} className="mt-2 w-full rounded-xl border border-border bg-surface p-3 text-sm leading-6 outline-none focus:border-accent" /><button type="button" disabled={busy && !saving} onClick={() => { const next = { ...values.current, [prompt.id]: 'I am not sure how to solve this yet.' }; values.current = next; setAnswers(next); finalRequest.current = null; }} className="min-h-9 text-xs text-accent">I’m not sure yet</button></div>)}</div>
      <div className="mt-5 flex flex-wrap items-center justify-between gap-3"><span role="status" className="text-xs text-text-muted">{saving ? 'Saving your answers…' : 'Answers save as you work.'}</span><button type="button" disabled={busy || diagnostic.prompts.some((p) => !answers[p.id]?.trim())} onClick={() => void finish()} className="min-h-11 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg disabled:opacity-50">{busy && !saving ? 'Reviewing your reasoning…' : 'Get my starting-point feedback'}</button></div>
    </> : <div className="mt-6 space-y-4"><p className="text-xs leading-5 text-text-muted">{diagnostic.interpretation}</p>{diagnostic.findings?.length ? diagnostic.findings.map((finding) => {
      const outcome = diagnostic.prompts.find((p) => p.id === finding.promptId);
      const module = plan.data?.acceptedRevision?.modules.find((m) => m.outcomeIds.includes(finding.outcomeId));
      const lessonId = module?.lessons[0]?.id;
      return <article key={finding.promptId} className="rounded-xl border border-border bg-background/50 p-4"><p className="text-xs font-semibold text-accent">{finding.signal === 'needs_practice' ? 'Worth practicing' : finding.signal === 'ready_for_challenge' ? 'Try a challenge' : 'Need more evidence'}</p><h4 className="mt-2 font-medium">{outcome?.outcomeTitle}</h4><p className="mt-2 text-sm leading-6 text-text-secondary">{finding.feedback}</p>{finding.evidenceQuote && <blockquote className="mt-3 border-l-2 border-accent/40 pl-3 text-sm italic text-text-muted">“{finding.evidenceQuote}”</blockquote>}{lessonId && onStudy && <button type="button" onClick={() => finding.signal === 'ready_for_challenge' && onChallenge ? onChallenge(lessonId) : onStudy(lessonId)} className="mt-3 min-h-11 rounded-full border border-accent/40 px-4 text-sm font-semibold text-accent">{finding.signal === 'ready_for_challenge' && onChallenge ? 'Try the module checkpoint' : 'Study this topic'}</button>}</article>;
    }) : <p className="text-sm text-text-secondary">This older diagnostic saved a self-inventory. Future starting-point checks use performance tasks.</p>}</div>}
    {(error || start.error || skip.error) && <div role="alert" className="mt-4 rounded-xl bg-rose-500/10 p-4 text-sm text-rose-700"><p>{error || String(start.error ?? skip.error)}</p>{conflict ? <div className="mt-2 flex gap-4"><button type="button" onClick={() => void reload(false)} className="underline">Load saved answers</button><button type="button" onClick={() => void reload(true)} className="underline">Keep my answers and retry</button></div> : error && active && <button type="button" onClick={() => void (finalRequest.current ? finish() : flush())} className="mt-2 font-semibold underline">Retry with my saved answers</button>}</div>}
  </section>;
}
