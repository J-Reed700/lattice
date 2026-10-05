import { useEffect, useMemo, useRef, useState } from 'react';

import { AlertCircle, ArrowDownRight, BookOpen, Check, ChevronRight, CircleHelp, Lightbulb, MessageCircle, Send, ShieldCheck, Sparkles, Unlock, X } from 'lucide-react';

import { MarkdownViewer } from '@/features/chat/components/viewers/MarkdownViewer';
import {
  useAcceptLearningPracticeProposal,
  useChangeLearningPracticeMode,
  useLearningPracticeSession,
  useLearningPracticeWorkspace,
  useOpenLearningPracticeSource,
  useRejectLearningPracticeProposal,
  useRequestLearningTutorResponse,
  useRevealLearningPracticeSolution,
  useSaveLearningPracticeArtifact,
  useStartLearningPracticeSession,
  useSubmitLearningPracticeAttempt,
} from '@/features/learning/practice/useLearningPractice';
import { useLearningSourceVersion, useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import type { LearningLessonDto, LearningPracticeHintLevel, LearningPracticeTaskKind, LearningPracticeMode, LearningPracticeSessionDto, LearningProgramDto, LearningSourceLibraryItemDto, SaveLearningPracticeArtifactRequestDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';


type Mode = LearningPracticeMode;
type RetryAction = { label: string; run: () => Promise<void> };

const MODES: { id: Mode; title: string; aid: string; description: string }[] = [
  { id: 'explore', title: 'Explore', aid: 'Open aid', description: 'Ask questions, use any hint, and reveal the worked solution after a deliberate confirmation.' },
  { id: 'practice', title: 'Practice', aid: 'Guided aid', description: 'Ask questions and use the first three scaffolded hints. Worked explanations and the solution stay closed.' },
  { id: 'demonstrate', title: 'Demonstrate', aid: 'Independent', description: 'Write and submit independently. Source opening, tutor hints, and solutions are closed during the attempt.' },
];
const HINTS: { id: LearningPracticeHintLevel; title: string; prompt: string }[] = [
  { id: 'orienting_question', title: 'Orienting question', prompt: 'Give me an orienting question that helps me notice where to begin, without giving away the answer.' },
  { id: 'concept_or_source', title: 'Concept or source', prompt: 'Point me toward a relevant concept or exact saved source, without solving the task.' },
  { id: 'partial_strategy', title: 'Partial strategy', prompt: 'Offer one partial strategy step, leaving the key reasoning for me.' },
  { id: 'worked_explanation', title: 'Worked explanation', prompt: 'Give a worked explanation. Clearly label it as substantial assistance.' },
];

function newId() {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => { const r = Math.random() * 16 | 0; return (c === 'x' ? r : (r & 0x3 | 0x8)).toString(16); });
}
function isConflict(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  return /changed|conflict|revision|reload/i.test(message);
}
function dateLabel(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'Time unavailable' : date.toLocaleString();
}
function dimensionName(value: string) { return value.charAt(0).toUpperCase() + value.slice(1); }

function FrozenSourceRail({ programId, versionIds, onOpen, disabledReason }: { programId: string; versionIds: string[]; onOpen: (source: LearningSourceLibraryItemDto, versionId: string) => void; disabledReason?: string }) {
  const workspace = useLearningSourceWorkspace(programId);
  const frozen = useMemo(() => {
    const versionSet = new Set(versionIds);
    return (workspace.data?.sources ?? []).flatMap((source) => source.versions.filter((version) => versionSet.has(version.id)).map((version) => ({ source, version })));
  }, [workspace.data, versionIds]);
  return <section aria-labelledby="frozen-sources-heading" className="rounded-2xl border border-border/70 bg-surface p-5">
    <div className="flex items-center gap-2 text-accent"><BookOpen size={15} /><span className="text-[10px] font-semibold uppercase tracking-[.15em]">Frozen source context</span></div>
    <h3 id="frozen-sources-heading" className="mt-1 font-serif text-xl text-text-primary">Saved versions for this attempt</h3>
    <p className="mt-1 text-[11px] leading-5 text-text-muted">This attempt keeps its original versions even if the library changes later.</p>
    {disabledReason && <p className="mt-3 rounded-lg bg-amber-500/5 p-3 text-[10px] leading-5 text-text-secondary">{disabledReason}</p>}
    {workspace.isLoading ? <p role="status" className="mt-4 text-xs text-text-muted">Loading frozen sources…</p> : workspace.isError ? <div className="mt-4"><p role="alert" className="text-xs text-rose-700">{workspace.error.message}</p><button type="button" onClick={() => void workspace.refetch()} className="mt-2 text-xs font-semibold text-accent">Retry source list</button></div> : frozen.length ? <div className="mt-4 space-y-2">{frozen.map(({ source, version }) => <button key={version.id} type="button" disabled={Boolean(disabledReason)} onClick={() => onOpen(source, version.id)} className="w-full rounded-xl border border-border/70 bg-background p-3 text-left transition hover:border-accent/40 focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-55"><span className="flex items-start justify-between gap-2"><span className="min-w-0"><span className="block truncate text-xs font-semibold text-text-primary">{version.title}</span><span className="mt-1 block text-[10px] text-text-muted">Version {version.versionNumber} · saved {dateLabel(version.acquiredAt)}</span></span><ChevronRight size={14} className="mt-1 shrink-0 text-text-muted" /></span><span className="mt-2 line-clamp-3 block text-[11px] leading-5 text-text-secondary">{version.excerpt}</span></button>)}</div> : <p className="mt-4 rounded-lg bg-background p-3 text-xs text-text-muted">No frozen source versions are attached to this session.</p>}
  </section>;
}

function PreviousFeedback({ session }: { session: LearningPracticeSessionDto }) {
  const inherited = session.assistance.map((event) => event.details as Record<string, unknown> | null).find((details) => details?.revisesSessionId);
  const feedback = inherited?.previousFeedback;
  if (!Array.isArray(feedback)) return null;
  return <details open className="rounded-xl border border-accent/20 bg-accent/5 p-4"><summary className="cursor-pointer text-sm font-semibold">Feedback to use in this revision</summary><ul className="mt-3 space-y-3">{feedback.map((item: { criterionId?: string; observation?: string; evidenceQuote?: string }, index) => <li key={item.criterionId ?? index} className="text-sm leading-6"><p>{item.observation}</p>{item.evidenceQuote && <blockquote className="mt-1 border-l-2 border-accent/30 pl-3 text-xs text-text-muted">{item.evidenceQuote}</blockquote>}</li>)}</ul><p className="mt-3 text-xs text-text-muted">Your original submission and feedback stay in the attempt history.</p></details>;
}

export function PracticeWorkbenchPanel({ program, lesson, taskKind = 'independent', embedded = false, onContinue }: { program: LearningProgramDto; lesson: LearningLessonDto | undefined; taskKind?: LearningPracticeTaskKind; embedded?: boolean; onContinue?: () => void }) {
  const programId = program.summary.id;
  const lessonId = lesson?.id ?? '';
  const workspace = useLearningPracticeWorkspace(programId);
  const sourceWorkspace = useLearningSourceWorkspace(programId);
  const start = useStartLearningPracticeSession();
  const save = useSaveLearningPracticeArtifact();
  const changeMode = useChangeLearningPracticeMode();
  const openSource = useOpenLearningPracticeSource();
  const tutor = useRequestLearningTutorResponse();
  const reveal = useRevealLearningPracticeSolution();
  const submit = useSubmitLearningPracticeAttempt();
  const acceptProposal = useAcceptLearningPracticeProposal();
  const rejectProposal = useRejectLearningPracticeProposal();
  const [sessionId, setSessionId] = useState<string | null>(null);
  const sessionQuery = useLearningPracticeSession(sessionId);
  const session = sessionQuery.data;
  const [modeChoice, setModeChoice] = useState<Mode>(embedded ? 'practice' : 'explore');
  const [text, setText] = useState('');
  const [savedText, setSavedText] = useState('');
  const [composer, setComposer] = useState('');
  const [retryAction, setRetryAction] = useState<RetryAction | null>(null);
  const [actionError, setActionError] = useState('');
  const [busyLabel, setBusyLabel] = useState('');
  const [conflict, setConflict] = useState(false);
  const conflictRef = useRef(false);
  const [solutionConfirm, setSolutionConfirm] = useState(false);
  const [selectedSource, setSelectedSource] = useState<{ sourceId: string; versionId: string } | null>(null);
  const solutionTriggerRef = useRef<HTMLButtonElement>(null);
  const [hintUsed, setHintUsed] = useState<LearningPracticeHintLevel[]>([]);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saveFlightRef = useRef<Promise<boolean> | null>(null);
  const saveRequestRef = useRef<{ fingerprint: string; request: SaveLearningPracticeArtifactRequestDto } | null>(null);
  const textRef = useRef(text);
  const savedRef = useRef(savedText);
  const hydratedSessionRef = useRef<string | null>(null);
  const hydratedRevisionRef = useRef<number>(-1);
  const revisionRef = useRef(0);
  const summary = session?.summary;
  const active = summary?.status === 'active';
  const mode = summary?.mode ?? modeChoice;
  const canTutor = active && mode !== 'demonstrate';
  const solutionAllowed = active && mode === 'explore';
  const sourceOpeningAllowed = !active || mode !== 'demonstrate';
  const sourceRequest = selectedSource ? { programId, sourceId: selectedSource.sourceId, versionId: selectedSource.versionId } : null;
  const sourceVersion = useLearningSourceVersion(sourceRequest);

  useEffect(() => { textRef.current = text; }, [text]);
  useEffect(() => { savedRef.current = savedText; }, [savedText]);
  useEffect(() => {
    const candidates = workspace.data?.sessions.filter((item) => item.lessonId === lessonId && (item.taskKind ?? 'independent') === taskKind) ?? [];
    const latest = [...candidates].sort((a, b) => Number(b.status === 'active') - Number(a.status === 'active') || b.updatedAt - a.updatedAt)[0];
    if (sessionId && sessionQuery.data?.summary.lessonId === lessonId && (sessionQuery.data?.summary.taskKind ?? 'independent') === taskKind) return;
    if (latest?.id !== sessionId) setSessionId(latest?.id ?? null);
  }, [lessonId, sessionId, sessionQuery.data?.summary.lessonId, sessionQuery.data?.summary.taskKind, workspace.data, taskKind]);
  useEffect(() => {
    if (!session) return;
    if (session.summary.id !== sessionId) return;
    revisionRef.current = session.summary.revision;
    if (hydratedSessionRef.current !== session.summary.id) {
      hydratedSessionRef.current = session.summary.id;
      hydratedRevisionRef.current = session.artifact.revision;
      setText(session.artifact.text);
      setSavedText(session.artifact.text);
      textRef.current = session.artifact.text;
      savedRef.current = session.artifact.text;
      saveRequestRef.current = null;
    } else if (session.artifact.revision > hydratedRevisionRef.current && textRef.current === savedRef.current) {
      hydratedRevisionRef.current = session.artifact.revision;
      setText(session.artifact.text);
      textRef.current = session.artifact.text;
      setSavedText(session.artifact.text);
      savedRef.current = session.artifact.text;
    }
    setHintUsed(session.assistance.filter((event) => event.kind === 'hint').map((event) => {
      const val = (event.details as Record<string, unknown> | null)?.hintLevel;
      return typeof val === 'string' ? val as LearningPracticeHintLevel : undefined;
    }).filter((value): value is LearningPracticeHintLevel => Boolean(value)));
  // Reset only when switching session. Query refreshes must not overwrite edits.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.summary.id, session?.summary.revision, session?.artifact.revision, sessionId]);

  const perform = async (label: string, run: () => Promise<unknown>) => {
    setBusyLabel(label); setActionError(''); setRetryAction(null); setConflict(false);
    try { await run(); conflictRef.current = false; return true; }
    catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      conflictRef.current = isConflict(error);
      setActionError(message); setConflict(conflictRef.current);
      return false;
    } finally { setBusyLabel(''); }
  };

  const saveNow = async (): Promise<boolean> => {
    if (!summary || !active || textRef.current === savedRef.current) return true;
    if (saveFlightRef.current) return saveFlightRef.current;
    const payload = textRef.current;
    const expectedRevision = revisionRef.current || summary.revision;
    const fingerprint = `${summary.id}:${expectedRevision}:${payload}`;
    if (saveRequestRef.current?.fingerprint !== fingerprint) {
      saveRequestRef.current = { fingerprint, request: { operationId: newId(), programId, sessionId: summary.id, expectedRevision, text: payload } };
    }
    const request = saveRequestRef.current.request;
    const flight = (async () => {
      const ok = await perform('Saving response', async () => {
        const updated = await save.mutateAsync(request);
        const updatedSummary = updated.sessions.find((item) => item.id === request.sessionId);
        if (updatedSummary) {
          revisionRef.current = updatedSummary.revision;
          hydratedRevisionRef.current = updatedSummary.artifactRevision;
        }
        await sessionQuery.refetch();
        setSavedText(payload); savedRef.current = payload;
        saveRequestRef.current = null;
      });
      if (!ok) {
        if (!conflictRef.current) {
          const retry: RetryAction = { label: 'Retry save', run: async () => { await saveNow(); } };
          setRetryAction(() => retry);
        }
        return false;
      }
      return true;
    })();
    saveFlightRef.current = flight;
    try {
      const ok = await flight;
      saveFlightRef.current = null;
      if (ok && textRef.current !== payload) return await saveNow();
      return ok;
    } finally { if (saveFlightRef.current === flight) saveFlightRef.current = null; }
  };
  const saveNowRef = useRef(saveNow);
  saveNowRef.current = saveNow;
  useEffect(() => registerPendingSave(() => saveNowRef.current()), [sessionId]);

  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (!active || text === savedText) return;
    timerRef.current = setTimeout(() => { void saveNowRef.current(); }, 650);
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [text, savedText, active, sessionId]);

  const runRetryable = async <T,>(label: string, invoke: (request: T) => Promise<unknown>, request: T) => {
    const stableRun = async () => invoke(request);
    const execute = async () => {
      const ok = await perform(label, stableRun);
      if (ok) {
        setBusyLabel(label);
        try { if (sessionId) await sessionQuery.refetch(); await workspace.refetch(); }
        finally { setBusyLabel(''); }
      }
      else if (!conflictRef.current) {
        const retry: RetryAction = { label: `Retry ${label.toLowerCase()}`, run: async () => { await execute(); } };
        setRetryAction(() => retry);
      }
      return ok;
    };
    const ok = await execute();
    return ok;
  };

  const startSession = async () => {
    if (lesson?.preparation !== 'ready') return;
    const id = newId();
    const request = { operationId: newId(), sessionId: id, programId, lessonId: lesson.id, expectedProgramRevision: program.summary.revision, mode: session?.summary.status === 'submitted' ? 'practice' as const : session?.summary.mode ?? modeChoice, taskKind, revisesSessionId: session?.summary.status === 'submitted' ? session.summary.id : null };
    await runRetryable('Start attempt', async (payload) => { await start.mutateAsync(payload); setSessionId(id); }, request);
  };

  const switchMode = async (next: Mode) => {
    if (!summary || !active || next === mode) return;
    if (!await saveNow()) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = { operationId: newId(), programId, sessionId: latest.id, expectedRevision: revisionRef.current || latest.revision, mode: next };
    if (await runRetryable('Change mode', (payload) => changeMode.mutateAsync(payload), request)) setModeChoice(next);
  };

  const sendTutor = async (requestKind: 'hint' | 'question' | 'critique', prompt: string, hintLevel?: LearningPracticeHintLevel) => {
    if (!summary || !active || !canTutor || !prompt.trim()) return;
    if (!await saveNow()) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = { operationId: newId(), programId, sessionId: latest.id, expectedRevision: revisionRef.current || latest.revision, requestKind, prompt: prompt.trim(), hintLevel: hintLevel ?? null };
    const ok = await runRetryable(requestKind === 'hint' ? 'Request hint' : requestKind === 'critique' ? 'Request critique' : 'Ask tutor', (payload) => tutor.mutateAsync(payload), request);
    if (ok) { setComposer(''); if (hintLevel) setHintUsed((current) => current.includes(hintLevel) ? current : [...current, hintLevel]); }
  };

  const doOpenSource = async (source: LearningSourceLibraryItemDto, versionId: string) => {
    if (!summary || busyLabel) return;
    if (!active) { setSelectedSource({ sourceId: source.id, versionId }); return; }
    if (mode === 'demonstrate') return;
    if (!await saveNow()) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = { operationId: newId(), programId, sessionId: latest.id, expectedRevision: revisionRef.current || latest.revision, sourceId: source.id, versionId };
    if (await runRetryable('Open saved source', (payload) => openSource.mutateAsync(payload), request)) setSelectedSource({ sourceId: source.id, versionId });
  };

  const doReveal = async () => {
    if (!summary || !solutionAllowed) return;
    if (!await saveNow()) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = { operationId: newId(), programId, sessionId: latest.id, expectedRevision: revisionRef.current || latest.revision };
    if (await runRetryable('Reveal solution', (payload) => reveal.mutateAsync(payload), request)) setSolutionConfirm(false);
  };
  const doSubmit = async () => {
    if (!summary || !active || !text.trim()) return;
    if (!await saveNow()) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = { operationId: newId(), programId, sessionId: latest.id, expectedRevision: revisionRef.current || latest.revision };
    await runRetryable('Submit response', (payload) => submit.mutateAsync(payload), request);
  };
  const decide = async (proposalId: string, accepted: boolean) => {
    if (!summary || !active) return;
    if (!await saveNow()) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = { operationId: newId(), programId, sessionId: latest.id, expectedRevision: revisionRef.current || latest.revision, proposalId };
    await runRetryable(accepted ? 'Accept proposal' : 'Reject proposal', (payload) => accepted ? acceptProposal.mutateAsync(payload) : rejectProposal.mutateAsync(payload), request);
  };

  const sessionList = workspace.data?.sessions ?? [];
  if (workspace.isLoading) return <div role="status" className="rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted">Opening your practice workbench…</div>;
  if (workspace.isError) return <div className="rounded-2xl border border-rose-500/30 bg-rose-500/5 p-6"><p role="alert" className="text-sm text-rose-700">{workspace.error.message}</p><button type="button" onClick={() => void workspace.refetch()} className="mt-3 rounded-full border border-border px-4 py-2 text-xs font-semibold">Retry workbench</button></div>;

  if (embedded) {
    const nextHint = HINTS.slice(0, 3).find((hint) => !hintUsed.includes(hint.id));
    const busy = Boolean(busyLabel);
    return <div className="mt-5 space-y-4 border-t border-border pt-5" aria-label="Guided exercise">
      {!sessionId ? <div><p className="text-sm leading-6 text-text-secondary">Try the exercise in your own words. Your work saves as you go; request one hint at a time when you need it.</p><button type="button" disabled={busy || start.isPending} onClick={() => void startSession()} className="mt-3 min-h-11 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg disabled:opacity-50">{busy ? 'Starting…' : 'Start guided exercise'}</button></div> : sessionQuery.isError ? <div role="alert">{sessionQuery.error.message}<button type="button" onClick={() => void sessionQuery.refetch()} className="ml-3 text-accent">Retry exercise</button></div> : !session ? <p role="status">Opening your saved exercise…</p> : <>
        <div className="flex flex-wrap items-center justify-between gap-2"><label htmlFor={`guided-answer-${lessonId}`} className="text-sm font-semibold text-text-primary">Your working</label><span role="status" className="text-xs text-text-muted">{busyLabel || (text === savedText ? 'Saved' : 'Saving…')}</span></div>
        <textarea id={`guided-answer-${lessonId}`} rows={6} value={text} maxLength={24000} disabled={!active || busy} onChange={(event) => { setText(event.target.value); textRef.current = event.target.value; }} placeholder="Show your first step, explain your reasoning, or paste your code…" className="w-full rounded-xl border border-border bg-background p-4 text-sm leading-6 outline-none focus:border-accent disabled:opacity-70" />
        {active && <div className="flex flex-wrap gap-2"><button type="button" disabled={busy || !text.trim()} onClick={() => void sendTutor('critique', 'Check my current reasoning against the exercise. Identify the first specific gap and ask a useful question. Do not reveal the final answer.')} className="min-h-11 rounded-full border border-accent/40 px-4 text-sm text-accent disabled:opacity-50">Check my reasoning</button>{nextHint && <button type="button" disabled={busy} onClick={() => void sendTutor('hint', nextHint.prompt, nextHint.id)} className="min-h-11 rounded-full border border-border px-4 text-sm disabled:opacity-50">{hintUsed.length ? 'Next hint' : 'Give me a hint'}</button>}<button type="button" disabled={busy || !text.trim()} onClick={() => void doSubmit()} className="min-h-11 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg disabled:opacity-50">Submit guided response</button></div>}
        {session.tutorTurns.length > 0 && <div aria-live="polite" className="space-y-3">{session.tutorTurns.map((turn, index) => <article key={turn.id} className="rounded-xl bg-accent/5 p-4"><p className="mb-2 text-xs font-semibold text-accent">{turn.requestKind === 'hint' ? `Hint · ${HINTS.find((hint) => hint.id === turn.hintLevel)?.title ?? index + 1}` : 'Feedback on your reasoning'}</p><div className="prose prose-sm max-w-none dark:prose-invert"><MarkdownViewer content={turn.response} /></div></article>)}</div>}
        <details open={Boolean(session.result)} className="rounded-xl border border-border"><summary className="cursor-pointer p-3 text-sm font-medium">{session.result ? 'Your feedback and next revision' : 'What good work looks like'}</summary><RubricPanel rubric={session.rubric} result={session.result} /></details>
        {session.result && <div className="flex flex-wrap gap-2"><button type="button" disabled={busy} onClick={() => void startSession()} className="min-h-11 rounded-full border border-accent/40 px-4 text-sm text-accent">Revise this response</button>{onContinue && <button type="button" onClick={onContinue} className="min-h-11 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg">Continue to independent assignment</button>}</div>}
        {session.summary.revisesSessionId && <PreviousFeedback session={session} />}
      </>}
      {actionError && <div role="alert" className="rounded-xl bg-rose-500/10 p-3 text-sm text-rose-700"><p>{actionError}</p>{retryAction && <button type="button" disabled={busy} onClick={() => void retryAction.run()} className="mt-2 font-semibold underline">{retryAction.label}</button>}{conflict && <button type="button" onClick={() => void sessionQuery.refetch()} className="mt-2 font-semibold underline">Reload saved exercise</button>}</div>}
    </div>;
  }

  return <section className="min-w-0 space-y-5" aria-label="Grounded Practice Workbench">
    <header className="rounded-2xl border border-[#d7c9b8] bg-[#eee8df] p-5 sm:p-7 dark:border-white/10 dark:bg-[#28251f]">
      <div className="flex flex-wrap items-start justify-between gap-4"><div><div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[.16em] text-accent"><Sparkles size={14} /> Grounded Practice Workbench</div><h3 className="mt-2 font-serif text-2xl leading-tight text-text-primary">Make your thinking visible</h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Work on the lesson assignment, get specific feedback, and revise your response. Tutor assistance is recorded with the attempt; feedback is provisional evidence, not a mastery score.</p></div>{sessionList.length > 0 && <label className="min-w-0 max-w-full text-xs text-text-muted">Attempt history<select aria-label="Select an attempt" value={sessionId ?? ''} onChange={(event) => { void saveNow().then((ok) => { if (ok) setSessionId(event.target.value || null); }); }} className="mt-1 block w-full max-w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm text-text-primary"><option value="">Choose an attempt</option>{sessionList.filter((item) => item.lessonId === lessonId && (item.taskKind ?? 'independent') === taskKind).map((item) => <option key={item.id} value={item.id}>{item.status === 'submitted' ? 'Submitted' : 'In progress'} · {item.mode} · {dateLabel(item.createdAt)}</option>)}</select></label>}</div>
      {!lesson ? <p className="mt-5 rounded-xl bg-surface/70 p-4 text-sm text-text-muted">Choose a lesson to begin a grounded attempt.</p> : lesson.preparation !== 'ready' ? <div className="mt-5 rounded-xl border border-amber-500/25 bg-amber-500/5 p-4"><p className="text-sm font-medium text-text-primary">Prepare this lesson first</p><p className="mt-1 text-xs leading-5 text-text-secondary">The workbench uses the lesson’s prepared prompt and frozen source versions. Preparing a lesson does not create an attempt.</p></div> : !sessionId ? <div className="mt-5 grid gap-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-end"><fieldset className="min-w-0"><legend className="text-xs font-semibold text-text-primary">Choose your aid contract</legend><div className="mt-2 grid gap-2 sm:grid-cols-3">{MODES.filter((item) => !session?.summary.revisesSessionId || item.id !== 'demonstrate').map((item) => <label key={item.id} className={`cursor-pointer rounded-xl border p-3 transition ${modeChoice === item.id ? 'border-accent/50 bg-accent/5' : 'border-border bg-surface'}`}><span className="flex items-center gap-2"><input type="radio" name="new-session-mode" value={item.id} checked={modeChoice === item.id} onChange={() => setModeChoice(item.id)} className="accent-[hsl(var(--accent))]" /><span className="text-sm font-semibold text-text-primary">{item.title}</span></span><span className="mt-1 block text-[10px] font-semibold uppercase tracking-wider text-accent">{item.aid}</span><span className="mt-1 block text-xs leading-5 text-text-secondary">{item.description}</span></label>)}</div></fieldset><button type="button" disabled={Boolean(busyLabel) || start.isPending} onClick={() => void startSession()} className="rounded-full bg-accent px-5 py-3 text-sm font-semibold text-accent-fg disabled:opacity-50">{busyLabel === 'Start attempt' ? 'Starting…' : 'Start an attempt'}</button></div> : sessionQuery.isLoading ? <p role="status" className="mt-5 text-xs text-text-muted">Loading the selected attempt…</p> : session ? <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-black/10 pt-4 dark:border-white/10"><div><span className="text-xs font-semibold text-text-primary">{lesson.title}</span><span className="ml-2 rounded-full bg-surface px-2 py-1 text-[10px] uppercase tracking-wider text-text-muted">{summary?.status === 'submitted' ? 'Submitted' : 'In progress'}</span></div>{summary?.status === 'submitted' && <button type="button" disabled={Boolean(busyLabel)} onClick={() => void startSession()} className="rounded-full border border-accent/40 px-4 py-2 text-xs font-semibold text-accent">Revise in a new attempt</button>}</div> : null}
    </header>

    {lesson?.preparation === 'ready' && sessionId && (sessionQuery.isLoading ? <div role="status" className="rounded-2xl border border-border bg-surface p-7 text-sm text-text-muted">Loading saved attempt…</div> : sessionQuery.isError ? <div className="rounded-2xl border border-rose-500/30 bg-surface p-5"><p role="alert" className="text-sm text-rose-700">Could not load attempt: {sessionQuery.error.message}</p><button type="button" onClick={() => void sessionQuery.refetch()} className="mt-3 text-xs font-semibold text-accent">Retry</button></div> : session ? <div className="grid min-w-0 gap-5 xl:grid-cols-[minmax(0,1fr)_330px]">
      <div className="min-w-0 space-y-5">
        <section className="rounded-2xl border border-border bg-surface p-5 sm:p-7"><div className="flex flex-wrap items-center justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Task prompt</div><h4 className="mt-1 font-serif text-xl text-text-primary">Respond to the problem</h4></div><span className="rounded-full bg-background px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-text-muted">{summary?.mode} mode · aid is recorded</span></div><p className="mt-4 whitespace-pre-wrap rounded-xl bg-background p-4 text-sm leading-7 text-text-secondary">{session.taskPrompt}</p><div className="mt-3 rounded-lg border-l-2 border-accent/50 pl-3"><span className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">Lesson objective</span><p className="mt-1 text-xs leading-5 text-text-secondary">{session.lessonObjective}</p></div>
          <div className="mt-5 flex flex-wrap gap-2" role="group" aria-label="Session mode">{MODES.filter((item) => !session?.summary.revisesSessionId || item.id !== 'demonstrate').map((item) => <button key={item.id} type="button" aria-pressed={mode === item.id} disabled={!active || Boolean(busyLabel)} onClick={() => void switchMode(item.id)} className={`rounded-full border px-3 py-2 text-xs font-semibold transition disabled:cursor-default disabled:opacity-70 ${mode === item.id ? 'border-accent bg-accent text-accent-fg' : 'border-border text-text-secondary hover:border-accent/40'}`}>{item.title}</button>)}</div><p className="mt-2 text-[10px] leading-5 text-text-muted">Aid contract: {MODES.find((item) => item.id === mode)?.description}</p>
        </section>

        <section className="overflow-hidden rounded-2xl border border-border bg-surface"><div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-5 py-4"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Your artifact</div><h4 className="mt-1 font-serif text-xl text-text-primary">Work through the reasoning</h4></div><div className="flex items-center gap-2 text-[11px] text-text-muted" role="status" aria-live="polite">{busyLabel === 'Saving response' || save.isPending ? <><span className="h-2 w-2 animate-pulse rounded-full bg-amber-500" /> Saving…</> : actionError && retryAction?.label === 'Retry save' ? <><AlertCircle size={13} className="text-rose-600" /> Save needs attention</> : text === savedText ? <><Check size={13} className="text-emerald-700" /> Saved</> : 'Unsaved changes'}</div></div><label htmlFor="workbench-artifact" className="sr-only">Your response</label><textarea id="workbench-artifact" aria-label="Your response" value={text} disabled={!active} onChange={(event) => { setText(event.target.value); textRef.current = event.target.value; saveRequestRef.current = null; setActionError(''); setRetryAction(null); }} placeholder="Write your response, reasoning, or working here…" className="min-h-[250px] w-full resize-y bg-transparent px-5 py-5 text-sm leading-7 text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-2 focus:ring-inset focus:ring-accent/40 disabled:opacity-70 sm:min-h-[330px] sm:px-7" /><div className="flex flex-wrap items-center justify-between gap-3 border-t border-border bg-background/50 px-5 py-3"><p className="text-[10px] leading-5 text-text-muted">Your writing autosaves to this attempt. Submitting freezes this version for feedback.</p><div className="flex flex-wrap gap-2">{conflict && <button type="button" onClick={async () => { await sessionQuery.refetch(); setConflict(false); conflictRef.current = false; saveRequestRef.current = null; }} className="rounded-full border border-amber-500/40 px-3 py-2 text-xs font-semibold text-amber-800">Reload current revision · keep my text</button>}{retryAction && <button type="button" onClick={() => void retryAction.run()} className="rounded-full border border-rose-500/40 px-3 py-2 text-xs font-semibold text-rose-700">{retryAction.label}</button>}{active && <button type="button" disabled={Boolean(busyLabel) || !text.trim()} onClick={() => void doSubmit()} className="rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:opacity-45">{busyLabel === 'Submit response' ? 'Submitting…' : 'Submit response'}</button>}</div></div>{actionError && <p role="alert" className="px-5 pb-4 text-xs text-rose-700">{actionError}</p>}</section>

        {session.result && <ResultPanel result={session.result} />}

        {session.summary.revisesSessionId && <PreviousFeedback session={session} />}<section className="rounded-2xl border border-border bg-surface p-5 sm:p-6"><div className="flex items-center gap-2 text-accent"><MessageCircle size={15} /><h4 className="font-serif text-xl text-text-primary">Tutor conversation</h4></div><div className="mt-4 space-y-3">{session.tutorTurns.map((turn) => <article key={turn.id} className="rounded-xl border border-border/70 bg-background p-4"><div className="flex flex-wrap items-center justify-between gap-2"><span className="text-[10px] font-semibold uppercase tracking-wider text-accent">{turn.hintLevel ? HINTS.find((item) => item.id === turn.hintLevel)?.title ?? 'Hint' : turn.requestKind === 'critique' ? 'Critique' : 'Tutor response'}</span><span className="text-[10px] text-text-muted">{turn.modelName} · {dateLabel(turn.createdAt)}</span></div><p className="mt-2 text-xs leading-6 text-text-muted">You: {turn.prompt}</p><p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-text-secondary">{turn.response}</p>{turn.citations.length > 0 && <CitationCards disabled={!sourceOpeningAllowed || Boolean(busyLabel)} citations={turn.citations} onOpen={(citation) => { const source = sourceForVersion(sourceWorkspace.data?.sources ?? [], citation.sourceId, citation.versionId); if (source) void doOpenSource(source, citation.versionId); }} />}</article>)}</div>
          {active && <div className="mt-4 space-y-3"><label htmlFor="tutor-message" className="text-xs font-semibold text-text-primary">Ask a question or request critique</label><textarea id="tutor-message" value={composer} onChange={(event) => setComposer(event.target.value)} disabled={!canTutor || Boolean(busyLabel)} placeholder={canTutor ? 'Ask about a step, assumption, or source…' : 'Tutor assistance is closed in Demonstrate mode.'} className="min-h-20 w-full resize-y rounded-xl border border-border bg-background p-3 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-2 focus:ring-accent/40 disabled:opacity-60" /><div className="flex flex-wrap justify-end gap-2"><button type="button" disabled={!canTutor || !composer.trim() || Boolean(busyLabel)} onClick={() => void sendTutor('critique', composer)} className="rounded-full border border-border px-3 py-2 text-xs font-semibold text-text-secondary disabled:opacity-40">Request critique</button><button type="button" disabled={!canTutor || !composer.trim() || Boolean(busyLabel)} onClick={() => void sendTutor('question', composer)} className="inline-flex items-center gap-1.5 rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:opacity-40"><Send size={12} /> Ask tutor</button></div>
          <div className="rounded-xl border border-border/70 bg-background p-4"><div className="flex items-center gap-2"><Lightbulb size={14} className="text-accent" /><span className="text-xs font-semibold text-text-primary">Hint ladder</span><span className="text-[10px] text-text-muted">Each rung is a recorded aid</span></div><div className="mt-3 grid gap-2 sm:grid-cols-2">{HINTS.map((hint, index) => { const locked = mode === 'practice' && hint.id === 'worked_explanation'; return <button key={hint.id} type="button" disabled={!canTutor || Boolean(busyLabel) || hintUsed.includes(hint.id) || locked} onClick={() => void sendTutor('hint', hint.prompt, hint.id)} className="flex min-w-0 items-center gap-2 rounded-lg border border-border bg-surface px-3 py-2.5 text-left text-xs text-text-secondary hover:border-accent/40 disabled:opacity-45"><span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent/10 text-[10px] font-semibold text-accent">{index + 1}</span><span className="min-w-0 flex-1">{hint.title}{locked && <span className="block text-[10px] text-text-muted">Available in Explore</span>}</span>{hintUsed.includes(hint.id) && <Check size={13} className="shrink-0 text-emerald-700" />}</button>; })}</div></div>
          </div>}
        </section>

        {session.revealedSolution && <section className="rounded-2xl border border-violet-500/25 bg-violet-500/5 p-5"><div className="flex items-center gap-2 text-violet-800 dark:text-violet-200"><Unlock size={15} /><h4 className="font-serif text-xl">Worked solution revealed</h4></div><p className="mt-3 whitespace-pre-wrap text-sm leading-7 text-text-secondary">{session.revealedSolution}</p>{session.revealedSolutionCitations.length > 0 && <CitationCards disabled={!sourceOpeningAllowed || Boolean(busyLabel)} citations={session.revealedSolutionCitations} onOpen={(citation) => { const source = sourceForVersion(sourceWorkspace.data?.sources ?? [], citation.sourceId, citation.versionId); if (source) void doOpenSource(source, citation.versionId); }} />}</section>}

        {session.proposals.length > 0 && <section className="rounded-2xl border border-border bg-surface p-5"><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Follow-up ideas</div><h4 className="mt-1 font-serif text-xl text-text-primary">Tutor proposals</h4><div className="mt-4 space-y-3">{session.proposals.map((proposal) => <article key={proposal.id} className="rounded-xl border border-border/70 bg-background p-4"><div className="flex flex-wrap items-center justify-between gap-2"><span className="rounded-full bg-accent/10 px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-accent">{proposal.kind === 'follow_up' ? 'Follow-up' : 'Possible misconception'}</span><span className="text-[10px] text-text-muted">{proposal.status}</span></div><p className="mt-2 text-sm leading-6 text-text-secondary">{proposal.text}</p>{proposal.evidenceQuote && <blockquote className="mt-3 border-l-2 border-accent/40 pl-3 text-xs italic leading-5 text-text-muted">“{proposal.evidenceQuote}”</blockquote>}{active && proposal.status === 'pending' && <div className="mt-3 flex gap-2"><button type="button" disabled={Boolean(busyLabel)} onClick={() => void decide(proposal.id, true)} className="rounded-full bg-accent px-3 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50">Accept idea</button><button type="button" disabled={Boolean(busyLabel)} onClick={() => void decide(proposal.id, false)} className="rounded-full border border-border px-3 py-2 text-xs font-semibold text-text-secondary disabled:opacity-50">Not useful</button></div>}</article>)}</div></section>}
      </div>

      <aside className="min-w-0 space-y-5">
        <FrozenSourceRail programId={programId} versionIds={session.summary.sourceVersionIds} disabledReason={!sourceOpeningAllowed ? 'Source opening is closed during an independent Demonstrate attempt.' : busyLabel ? 'Another workbench action is in progress.' : undefined} onOpen={(source, versionId) => void doOpenSource(source, versionId)} />
        {selectedSource && <ExactSourceReader query={sourceVersion} onClose={() => setSelectedSource(null)} />}
        <RubricPanel rubric={session.rubric} result={session.result} />
        <section className="rounded-2xl border border-border/70 bg-surface p-5"><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Assistance ledger</div><h4 className="mt-1 font-serif text-lg text-text-primary">Help used in this attempt</h4>{session.assistance.length ? <ol className="mt-3 space-y-3">{session.assistance.map((event) => <li key={event.id} className="flex gap-2 border-t border-border pt-3 first:border-0 first:pt-0"><span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent/10 text-accent"><ArrowDownRight size={12} /></span><span className="min-w-0"><span className="block text-xs font-medium text-text-primary">{assistanceLabel(event.kind)}</span><span className="mt-0.5 block text-[10px] leading-4 text-text-muted">{event.mode} · response revision {event.artifactRevision} · {dateLabel(event.createdAt)}</span></span></li>)}</ol> : <p className="mt-3 text-xs leading-5 text-text-muted">No assistance recorded yet. Source openings, tutor turns, mode changes, and solution reveals are listed here.</p>}<div className="mt-4 rounded-lg bg-background px-3 py-2 text-[10px] leading-4 text-text-muted"><ShieldCheck size={12} className="mr-1 inline text-accent" /> Results are interpreted alongside the aid used.</div></section>
        {active && mode === 'explore' && !session.revealedSolution && <section className="rounded-2xl border border-violet-500/25 bg-violet-500/5 p-5"><div className="flex items-center gap-2 text-violet-800 dark:text-violet-200"><CircleHelp size={15} /><h4 className="font-serif text-lg">Worked solution</h4></div><p className="mt-2 text-xs leading-5 text-text-secondary">Revealing it records substantial assistance and adds it to your ledger. You can keep writing first.</p><button ref={solutionTriggerRef} type="button" disabled={Boolean(busyLabel)} onClick={() => setSolutionConfirm(true)} className="mt-3 rounded-full border border-violet-500/40 px-4 py-2 text-xs font-semibold text-violet-800 dark:text-violet-200">Review reveal choice</button></section>}
      </aside>
    </div> : null)}

    {solutionConfirm && <ConfirmDialog title="Reveal the worked solution?" description="This will add substantial assistance to the attempt ledger. Your current response is saved first, and the solution will remain attached to this attempt." confirmLabel="Reveal solution" pending={Boolean(busyLabel)} onCancel={() => { setSolutionConfirm(false); window.requestAnimationFrame(() => solutionTriggerRef.current?.focus()); }} onConfirm={() => void doReveal()} />}
  </section>;
}

// Keep the rail mapping exact: a source is usable only when this attempt froze
// that version ID. Citations never fall forward to the library's active version.
function sourceForVersion(sources: LearningSourceLibraryItemDto[], sourceId: string, versionId: string) { return sources.find((source) => source.id === sourceId && source.versions.some((version) => version.id === versionId)); }

function CitationCards({ citations, onOpen, disabled = false }: { citations: { sourceId: string; versionId: string; quote: string }[]; onOpen: (citation: { sourceId: string; versionId: string; quote: string }) => void; disabled?: boolean }) {
  return <div className="mt-3 grid gap-2">{citations.map((citation, index) => <button key={`${citation.sourceId}-${citation.versionId}-${index}`} type="button" disabled={disabled} onClick={() => onOpen(citation)} className="rounded-lg border border-accent/20 bg-accent/5 p-3 text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-55"><span className="text-[10px] font-semibold uppercase tracking-wider text-accent">Saved source quote · exact version</span><span className="mt-1 block text-xs leading-5 text-text-secondary">“{citation.quote}”</span><span className="mt-2 block text-[10px] text-text-muted">{disabled ? 'Unavailable during this action' : 'Open frozen source'}</span></button>)}</div>;
}
function ExactSourceReader({ query, onClose }: { query: ReturnType<typeof useLearningSourceVersion>; onClose: () => void }) {
  return <section aria-labelledby="exact-source-title" className="rounded-2xl border border-accent/30 bg-surface p-5"><div className="flex items-start justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Immutable source version</div><h4 id="exact-source-title" className="mt-1 font-serif text-lg text-text-primary">{query.data?.version.title ?? 'Saved source'}</h4></div><button type="button" onClick={onClose} aria-label="Close saved source reader" className="rounded-full p-1.5 text-text-muted hover:bg-background"><X size={15} /></button></div>{query.isLoading ? <p role="status" className="mt-3 text-xs text-text-muted">Opening exact saved version…</p> : query.isError ? <div className="mt-3"><p role="alert" className="text-xs text-rose-700">{query.error.message}</p><button type="button" onClick={() => void query.refetch()} className="mt-2 text-xs font-semibold text-accent">Retry</button></div> : query.data && <><p className="mt-1 text-[10px] text-text-muted">Version {query.data.version.versionNumber} · saved {dateLabel(query.data.version.acquiredAt)} · {query.data.version.wordCount} words</p><div className="mt-3 max-h-72 overflow-y-auto rounded-xl bg-background p-3"><p className="whitespace-pre-wrap text-xs leading-6 text-text-secondary">{query.data.fullText}</p></div><p className="mt-2 text-[10px] leading-4 text-text-muted">This reader displays the locally saved version. It does not fetch the reference URL.</p></>}</section>;
}
function RubricPanel({ rubric, result }: { rubric: LearningPracticeSessionDto['rubric']; result: LearningPracticeSessionDto['result'] }) {
  return <section className="rounded-2xl border border-border/70 bg-surface p-5"><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Visible rubric</div><h4 className="mt-1 font-serif text-lg text-text-primary">What the response considers</h4><ol className="mt-3 space-y-3">{rubric.map((criterion) => { const feedback = result?.criteria.find((item) => item.criterionId === criterion.id); return <li key={criterion.id} className="border-t border-border pt-3 first:border-0 first:pt-0"><div className="flex items-start justify-between gap-2"><span className="text-xs font-semibold text-text-primary">{criterion.title}</span><span className="shrink-0 rounded-full bg-background px-2 py-1 text-[9px] uppercase tracking-wider text-text-muted">{dimensionName(criterion.dimension)}</span></div><p className="mt-1 text-[11px] leading-5 text-text-secondary">{criterion.description}</p>{feedback && <div className="mt-2 rounded-lg bg-accent/5 p-2.5"><p className="text-xs leading-5 text-text-secondary">{feedback.observation}</p>{feedback.score !== null && <p className="mt-1 text-[10px] text-text-muted">Provisional criterion signal: {feedback.score} / {feedback.maxPoints}</p>}{feedback.evidenceQuote && <blockquote className="mt-2 border-l border-accent/40 pl-2 text-[10px] italic leading-4 text-text-muted">“{feedback.evidenceQuote}”</blockquote>}</div>}</li>; })}</ol>{result && <p className="mt-3 rounded-lg bg-amber-500/5 p-3 text-[10px] leading-5 text-text-secondary">{result.gradeStatus === 'uncertain' ? 'Feedback is uncertain; the response may not support a reliable interpretation.' : 'Feedback is provisional and criterion-specific; it is not a mastery claim.'} Model: {result.graderModel}.</p>}</section>;
}
function ResultPanel({ result }: { result: NonNullable<LearningPracticeSessionDto['result']> }) {
  return <section aria-labelledby="workbench-result-heading" className="rounded-2xl border border-emerald-700/20 bg-emerald-700/[.035] p-5 sm:p-6"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-emerald-800 dark:text-emerald-300">Response evidence</div><h4 id="workbench-result-heading" className="mt-1 font-serif text-xl text-text-primary">Feedback on revision {result.artifactRevision}</h4></div><span className="rounded-full bg-background px-3 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-text-muted">{result.gradeStatus}</span></div><p className="mt-3 whitespace-pre-wrap rounded-xl bg-background p-4 text-xs leading-6 text-text-secondary">{result.artifactText}</p><div className="mt-4 grid gap-2 sm:grid-cols-2">{result.evidence.map((item, i) => <article key={`${item.dimension}-${i}`} className="rounded-xl border border-border/70 bg-surface p-3"><div className="flex items-center justify-between gap-2"><span className="text-xs font-semibold text-text-primary">{dimensionName(item.dimension)}</span><span className={`rounded-full px-2 py-1 text-[9px] uppercase tracking-wider ${item.observed ? 'bg-emerald-700/10 text-emerald-800' : 'bg-background text-text-muted'}`}>{item.observed ? 'Observed' : 'Not observed'}</span></div><p className="mt-2 text-xs leading-5 text-text-secondary">{item.observation}</p>{item.evidenceQuote && <blockquote className="mt-2 border-l border-accent/40 pl-2 text-[10px] italic leading-4 text-text-muted">“{item.evidenceQuote}”</blockquote>}</article>)}</div><p className="mt-3 text-[10px] leading-5 text-text-muted">Evidence interpretation is {result.gradeStatus}; assistance and submission mode are part of the record. This is not a mastery score.</p></section>;
}
function assistanceLabel(kind: string) { return ({ source_opened: 'Opened a frozen source', hint: 'Requested a tutor hint', solution_revealed: 'Revealed worked solution', mode_changed: 'Changed aid contract' } as Record<string, string>)[kind] ?? kind; }
function ConfirmDialog({ title, description, confirmLabel, pending, onCancel, onConfirm }: { title: string; description: string; confirmLabel: string; pending: boolean; onCancel: () => void; onConfirm: () => void }) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { cancelRef.current?.focus(); const listener = (event: KeyboardEvent) => { if (event.key === 'Escape' && !pending) onCancel(); }; window.addEventListener('keydown', listener); return () => window.removeEventListener('keydown', listener); }, [onCancel, pending]);
  return <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4" role="presentation"><section role="dialog" aria-modal="true" aria-labelledby="solution-confirm-title" aria-describedby="solution-confirm-description" onKeyDown={(event) => { if (event.key !== 'Tab') return; const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled)')); if (!focusable.length) return; const first = focusable[0]; const last = focusable[focusable.length - 1]; if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); } else if (!event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); } else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); } }} className="w-full max-w-md rounded-2xl border border-border bg-surface p-6 shadow-2xl"><h3 id="solution-confirm-title" className="font-serif text-2xl text-text-primary">{title}</h3><p id="solution-confirm-description" className="mt-3 text-sm leading-6 text-text-secondary">{description}</p><div className="mt-5 flex flex-wrap justify-end gap-2"><button ref={cancelRef} type="button" disabled={pending} onClick={onCancel} className="rounded-full border border-border px-4 py-2.5 text-sm font-medium text-text-secondary disabled:opacity-50">Keep working</button><button type="button" disabled={pending} onClick={onConfirm} className="rounded-full bg-accent px-4 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50">{pending ? 'Saving…' : confirmLabel}</button></div></section></div>;
}
