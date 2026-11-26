import { useEffect, useMemo, useRef, useState } from 'react';

import { ArrowUp, Check, Plus, RotateCcw, Sparkles, X } from 'lucide-react';

import { PlainError } from '@/components/PlainError';
import { TiptapViewer } from '@/components/TiptapEditor';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { NumberStepper } from '@/components/ui/number-stepper';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import {
  useAcceptLearningCurriculumRevision,
  useCancelLearningGenerationJob,
  useDiscardLearningCurriculumRevision,
  useLearningPlan,
  usePreviewLearningCurriculumRevision,
  useRetryLearningGenerationJob,
  useSkipLearningDiagnostic,
  useStartLearningDiagnostic,
  useSubmitLearningDiagnostic,
} from '@/features/learning/curriculum/useLearningPlan';
import { shownPhase } from '@/features/learning/lessons/lessonPreparationActivity';
import { LessonPreparationStatus } from '@/features/learning/lessons/LessonPreparationProgress';
import { CheckerAccuracy } from '@/features/learning/lessons/review/CheckerAccuracy';
import { VerifierAuditSwitch } from '@/features/learning/lessons/review/VerifierAuditSwitch';
import type {
  LearningCurriculumLesson,
  LearningCurriculumOperation,
  LearningGenerationJob,
  LearningPlanDto,
} from '@/lib/bindings';
import { jobErrorToError } from '@/lib/errorHandling';
import { openEditor } from '@/shared/edits/editorStore';


function uuid() { return globalThis.crypto?.randomUUID?.() ?? '00000000-0000-4000-8000-000000000002'; }
function message(error: unknown) { return error instanceof Error ? error.message : 'The change could not be saved.'; }
function dateLabel(value?: number | null) { if (value == null) return 'Time unavailable'; const date = new Date(value); return Number.isNaN(date.getTime()) ? 'Time unavailable' : date.toLocaleString(); }
function lessonState(state: LearningCurriculumLesson['state']) { return state.replace('_', ' '); }
function operationName(operation: LearningCurriculumOperation) {
  const names: Partial<Record<LearningCurriculumOperation['kind'], string>> = { add_lesson: 'Add lesson', move_lesson: 'Move lesson', skip_lesson: 'Skip lesson', replace_lesson: 'Replace lesson', challenge_prerequisite: 'Challenge prerequisite' };
  return names[operation.kind] ?? operation.kind;
}
function findLesson(plan: LearningPlanDto, lessonId: string) {
  const revision = plan.draftRevision ?? plan.acceptedRevision;
  for (const module of revision?.modules ?? []) {
    const lesson = module.lessons.find((item) => item.id === lessonId);
    if (lesson) return { module, lesson };
  }
  return undefined;
}

function DiagnosticCard({ plan, programId }: { plan: LearningPlanDto; programId: string }) {
  const diagnostic = plan.latestDiagnostic;
  const [responses, setResponses] = useState<Record<string, string>>({});
  const start = useStartLearningDiagnostic();
  const submit = useSubmitLearningDiagnostic();
  const skip = useSkipLearningDiagnostic();
  const busy = start.isPending || submit.isPending || skip.isPending;
  const expectedRevision = plan.programRevision;
  if (!diagnostic) return <section className="rounded-xl border border-border-subtle bg-surface p-5"><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Optional diagnostic</div><h3 className="mt-1 font-serif text-xl text-text-primary">Find source coverage gaps</h3><p className="mt-2 max-w-2xl text-xs leading-5 text-text-secondary">A short diagnostic can identify which outcomes lack source coverage. It does not award a mastery score or replace assessments.</p><Button type="button" variant="outline" disabled={busy} onClick={() => start.mutate({ operationId: uuid(), programId, expectedRevision })} className="mt-4"><Sparkles size={13} /> {start.isPending ? 'Starting…' : 'Start diagnostic'}</Button>{start.error && <p role="alert" className="mt-3 text-xs text-danger-fg">{message(start.error)} <button className="font-semibold underline" onClick={() => start.mutate({ operationId: uuid(), programId, expectedRevision })}>Try again</button></p>}</section>;
  const submitted = diagnostic.status !== 'active';
  const run = (kind: 'submit' | 'skip') => {
    const requestBase = { operationId: uuid(), programId, expectedRevision };
    if (kind === 'skip') skip.mutate(requestBase);
    else submit.mutate({ ...requestBase, diagnosticId: diagnostic.id, expectedDiagnosticRevision: diagnostic.revision ?? 0, responses: diagnostic.prompts.map((prompt) => ({ promptId: prompt.id, response: responses[prompt.id] ?? diagnostic.responses.find((answer) => answer.promptId === prompt.id)?.response ?? '' })) });
  };
  return <section className="rounded-xl border border-border-subtle bg-surface p-5"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Optional diagnostic</div><h3 className="mt-1 font-serif text-xl text-text-primary">Source coverage, not a score</h3></div><span className="rounded-full bg-bg px-3 py-1 text-xs capitalize text-text-muted">{diagnostic.status}</span></div>{submitted ? <div className="mt-4 rounded-xl bg-bg p-4"><p className="text-sm leading-6 text-text-secondary">{diagnostic.status === 'submitted' ? 'Your responses were saved. The diagnostic records source coverage gaps; it makes no mastery claim.' : 'This diagnostic was skipped.'}</p>{diagnostic.sourceCoverageGaps.length > 0 && <div className="mt-4"><h4 className="text-xs font-semibold text-text-primary">Outcomes without enough source coverage</h4><ul className="mt-2 space-y-1 text-xs text-text-secondary">{diagnostic.sourceCoverageGaps.map((gap) => <li key={gap.outcomeId}>• {gap.outcomeTitle}</li>)}</ul></div>}</div> : <div className="mt-4 space-y-3">{diagnostic.prompts.map((prompt) => <div key={prompt.id} className="rounded-xl border border-border-subtle bg-bg/50 p-4"><div className="text-sm font-medium text-text-primary"><TiptapViewer content={prompt.prompt} /></div><Textarea aria-label={`Response: ${prompt.prompt}`} rows={3} value={responses[prompt.id] ?? diagnostic.responses.find((answer) => answer.promptId === prompt.id)?.response ?? ''} disabled={busy} onChange={(event) => setResponses((current) => ({ ...current, [prompt.id]: event.target.value }))} wrapperClassName="mt-3" /></div>)}<div className="flex flex-wrap justify-end gap-2"><Button type="button" variant="outline" disabled={busy} onClick={() => run('skip')}>Skip diagnostic</Button><Button type="button" disabled={busy || diagnostic.prompts.some((prompt) => !(responses[prompt.id] ?? diagnostic.responses.find((answer) => answer.promptId === prompt.id)?.response ?? '').trim())} onClick={() => run('submit')}>{busy ? 'Saving…' : 'Submit diagnostic'}</Button></div></div>}{(submit.error || skip.error) && <p role="alert" className="mt-3 text-xs text-danger-fg">{message(submit.error ?? skip.error)} · Your responses remain available.</p>}</section>;
}

const jobTitle = (kind: LearningGenerationJob['kind']) => (kind === 'course_plan' ? 'Planning topics' : kind.replace('_', ' '));

function OtherJobCard({ job, programId, revision }: { job: LearningGenerationJob; programId: string; revision: number }) {
  const cancel = useCancelLearningGenerationJob();
  const retry = useRetryLearningGenerationJob();
  const pending = cancel.isPending || retry.isPending;
  const active = ['pending', 'running', 'interrupted'].includes(job.status);
  const percent = job.progressTotal > 0 ? Math.min(100, Math.round(job.progressCompleted / job.progressTotal * 100)) : 0;
  const request = () => ({ operationId: uuid(), programId, jobId: job.id, expectedRevision: revision });
  return <article className="rounded-xl border border-border bg-bg/40 p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="flex flex-wrap items-center gap-2"><h4 className="text-sm font-medium capitalize text-text-primary">{jobTitle(job.kind)}</h4><span className="rounded-full bg-surface px-2.5 py-1 text-[10px] capitalize text-text-muted">{job.status}</span></div><p className="mt-1 text-[10px] text-text-muted">Created {dateLabel(job.createdAt)}</p></div><div className="flex gap-2">{active && <button type="button" disabled={pending} onClick={() => cancel.mutate(request())} className="rounded-full border border-border px-3 py-2 text-[10px] disabled:opacity-50">Cancel</button>}{['failed', 'interrupted', 'cancelled'].includes(job.status) && <button type="button" disabled={pending} onClick={() => retry.mutate(request())} className="inline-flex items-center gap-1 rounded-full border border-accent/30 px-3 py-2 text-[10px] font-semibold text-accent disabled:opacity-50"><RotateCcw size={12} /> Retry</button>}</div></div>{job.status === 'running' || job.status === 'pending' ? <><div className="mt-3 flex items-center justify-between text-[10px] text-text-muted"><span>{job.progressMessage || 'Working on this request…'}</span><span>{job.progressCompleted}/{job.progressTotal || '—'}</span></div><div className="mt-2 h-1.5 overflow-hidden rounded-full bg-border"><div className="h-full rounded-full bg-accent transition-all" style={{ width: `${percent}%` }} /></div></> : job.error && <PlainError error={jobErrorToError(job)} className="mt-3 rounded-lg bg-rose-500/10 p-3 text-xs text-rose-700" />}{(cancel.error || retry.error) && <p role="alert" className="mt-2 text-xs text-rose-700">{message(cancel.error ?? retry.error)}</p>}</article>;
}

const ATTEMPT_OUTCOMES: Record<LearningGenerationJob['status'], string> = {
  pending: 'Waiting', running: 'Running', completed: 'Completed', failed: 'Stopped', cancelled: 'Paused', interrupted: 'Interrupted',
};

/** One line for an earlier attempt: how and where it ended, when, and why. */
function attemptOutcome(job: LearningGenerationJob): string {
  if (job.endedByUpdate) return `Ended by a Lattice update · ${dateLabel(job.finishedAt ?? job.createdAt)}`;
  const phase = job.kind === 'lesson_preparation' && job.activity ? shownPhase(job.activity).title : null;
  return [
    phase ? `${ATTEMPT_OUTCOMES[job.status]} at ${phase}` : ATTEMPT_OUTCOMES[job.status],
    dateLabel(job.finishedAt ?? job.createdAt),
    job.error,
  ].filter(Boolean).join(' · ');
}

/** The jobs of the activity list, newest first; a retry supersedes its parent, and an update ends an attempt, so both are earlier attempts. */
function jobAttempts(jobs: LearningGenerationJob[]) {
  const superseded = new Set(jobs.map((job) => job.retryOfJobId).filter(Boolean));
  const sorted = jobs.slice().sort((a, b) => b.createdAt - a.createdAt);
  return { current: sorted.filter((job) => !superseded.has(job.id) && !job.endedByUpdate), earlier: sorted.filter((job) => superseded.has(job.id) || job.endedByUpdate) };
}

function EarlierAttempts({ jobs }: { jobs: LearningGenerationJob[] }) {
  if (jobs.length === 0) return null;
  return <details className="rounded-xl border border-border bg-bg/40 px-4 py-3 text-xs">
    <summary className="cursor-pointer font-medium text-text-primary">Earlier attempts ({jobs.length})</summary>
    <ol className="mt-3 space-y-2 text-text-secondary">{jobs.map((job) => <li key={job.id} className="truncate" title={attemptOutcome(job)}>{attemptOutcome(job)}</li>)}</ol>
  </details>;
}

function JobCard({ job, programId, revision }: { job: LearningGenerationJob; programId: string; revision: number }) {
  return job.kind === 'lesson_preparation' && job.status !== 'completed'
    ? <LessonPreparationStatus job={job} pending={false} programId={programId} revision={revision} />
    : <OtherJobCard job={job} programId={programId} revision={revision} />;
}

export function PlanPanel({ programId, onStartingPoint }: { programId: string; onStartingPoint?: () => void }) {
  const planQuery = useLearningPlan(programId);
  const preview = usePreviewLearningCurriculumRevision();
  const accept = useAcceptLearningCurriculumRevision();
  const discard = useDiscardLearningCurriculumRevision();
  const [reason, setReason] = useState('Adapt the curriculum to current goals and evidence.');
  const [operations, setOperations] = useState<LearningCurriculumOperation[]>([]);
  const [title, setTitle] = useState('');
  const [objective, setObjective] = useState('');
  const [minutes, setMinutes] = useState(20);
  const [moduleId, setModuleId] = useState('');
  const [lessonId, setLessonId] = useState('');
  const [editMode, setEditMode] = useState<'add' | 'replace' | 'move' | null>(null);
  const editorTriggerRef = useRef<HTMLButtonElement | null>(null);
  const previousEditModeRef = useRef<typeof editMode>(null);
  const plan = planQuery.data;
  const revision = plan?.acceptedRevision;
  const draft = plan?.draftRevision;
  const expectedRevision = plan?.programRevision ?? 0;
  const current = draft ?? revision;
  const busy = preview.isPending || accept.isPending || discard.isPending;
  useEffect(() => {
    if (previousEditModeRef.current && !editMode) editorTriggerRef.current?.focus();
    previousEditModeRef.current = editMode;
  }, [editMode]);
  const attempts = useMemo(() => jobAttempts(plan?.jobs ?? []), [plan?.jobs]);
  const allLessons = useMemo(() => current?.modules.flatMap((module) => module.lessons.map((lesson) => ({ module, lesson }))) ?? [], [current]);
  const draftActions = (next: LearningCurriculumOperation[]) => setOperations(next);
  const previewRequest = () => preview.mutate({ operationId: uuid(), programId, expectedRevision, reason, operations });
  const queueLessonOperation = (kind: 'skip_lesson' | 'challenge_prerequisite', id: string) => draftActions([...operations, kind === 'skip_lesson' ? { kind, lesson_id: id } : { kind, lesson_id: id }]);
  const openEdit = (kind: 'replace' | 'move', id: string, trigger: HTMLButtonElement) => {
    const entry = findLesson(plan!, id);
    if (!entry) return;
    editorTriggerRef.current = trigger;
    setLessonId(id); setTitle(entry.lesson.title); setObjective(entry.lesson.objective); setMinutes(entry.lesson.estimatedMinutes); setModuleId(entry.module.id); setEditMode(kind);
  };
  const closeEditor = () => setEditMode(null);
  const openAddEditor = (trigger: HTMLButtonElement) => {
    editorTriggerRef.current = trigger;
    setEditMode('add'); setTitle(''); setObjective(''); setModuleId(current?.modules[0]?.id ?? ''); setMinutes(20);
  };
  const saveLessonOperation = () => {
    const existing = findLesson(plan!, lessonId);
    if (editMode === 'add') {
      const lesson: LearningCurriculumLesson = { id: uuid(), title: title.trim(), objective: objective.trim(), estimatedMinutes: Math.max(1, minutes), state: 'outline', assessmentStarted: false, replacementLessonId: null };
      draftActions([...operations, { kind: 'add_lesson', module_id: moduleId, after_lesson_id: null, lesson }]);
    } else if (editMode === 'replace' && existing) {
      const replacement: LearningCurriculumLesson = { id: uuid(), title: title.trim(), objective: objective.trim(), estimatedMinutes: Math.max(1, minutes), state: 'outline', assessmentStarted: false, replacementLessonId: null };
      draftActions([...operations, { kind: 'replace_lesson', lesson_id: lessonId, replacement }]);
    } else if (editMode === 'move') {
      draftActions([...operations, { kind: 'move_lesson', lesson_id: lessonId, target_module_id: moduleId, after_lesson_id: null }]);
    }
    setEditMode(null);
  };
  const onAccept = () => { if (!draft) return; accept.mutate({ operationId: uuid(), programId, revisionId: draft.id, expectedRevision }); };
  const onDiscard = () => { if (!draft) return; discard.mutate({ operationId: uuid(), programId, revisionId: draft.id, expectedRevision }); };

  if (planQuery.isLoading) return <div role="status" className="rounded-xl border border-border-subtle bg-surface p-8 text-sm text-text-muted">Loading curriculum plan…</div>;
  if (planQuery.isError || !plan) return <section className="rounded-xl border border-danger/25 bg-surface p-6"><h3 className="font-serif text-xl text-text-primary">Plan unavailable</h3><p role="alert" className="mt-2 text-sm text-text-secondary">{message(planQuery.error)}</p><Button type="button" variant="outline" onClick={() => void planQuery.refetch()} className="mt-4">Retry</Button></section>;

  return <div className="space-y-5" data-testid="learning-plan-panel"><header className="overflow-hidden rounded-xl border border-border-subtle bg-surface-raised/60 p-5 sm:p-6"><div className="flex flex-wrap items-start justify-between gap-5"><div><div className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Curriculum plan</div><h2 className="mt-1 font-serif text-3xl text-text-primary">Revise with the work in view</h2><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Changes are previewed as a new revision. Completed lessons and work with started assessments stay protected; the learner’s resume point remains visible.</p></div><div className="rounded-xl border border-border-subtle bg-surface px-4 py-3 text-right"><div className="text-[10px] font-semibold uppercase tracking-[.12em] text-text-muted">Required lesson count</div><div className="mt-1 font-serif text-2xl tabular-nums text-text-primary">{plan.requiredLessonCountBefore}<span className="px-2 text-sm text-text-muted">→</span>{plan.requiredLessonCountAfter}</div><div className="text-[10px] text-text-muted">Current plan to draft preview</div></div></div><div className="mt-5 flex flex-wrap gap-2 border-t border-border-subtle pt-4 text-xs"><span className="rounded-full border border-border-subtle bg-surface px-3 py-1 text-text-secondary">Accepted revision {revision?.revisionNumber ?? '—'}</span><span className="rounded-full border border-border-subtle bg-surface px-3 py-1 text-text-secondary">{draft ? `Draft revision ${draft.revisionNumber}` : 'No pending revision'}</span><span className="rounded-full border border-border-subtle bg-surface px-3 py-1 text-text-secondary">Resume: {plan.resumeLessonId ? allLessons.find((entry) => entry.lesson.id === plan.resumeLessonId)?.lesson.title ?? 'saved lesson' : 'no eligible lesson'}</span></div></header>

    <VerifierAuditSwitch programId={programId} />
    <section aria-label="Checker accuracy" className="rounded-xl border border-border-subtle bg-surface px-5 pb-4 pt-1"><CheckerAccuracy programId={programId} /></section>
    {onStartingPoint ? <section className="rounded-xl border border-border-subtle bg-surface p-5"><h3 className="font-serif text-xl">Check your starting point</h3><p className="mt-2 text-sm text-text-secondary">Try short performance tasks and get feedback on where to begin.</p><Button type="button" variant="outline" onClick={onStartingPoint} className="mt-3">Open starting-point check</Button></section> : <DiagnosticCard plan={plan} programId={programId} />}

    <section className="rounded-xl border border-border-subtle bg-surface p-5 sm:p-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Revision editor</div><h3 className="mt-1 font-serif text-2xl text-text-primary">Shape the next version</h3></div><div className="flex gap-2"><Button type="button" variant="outline" disabled={!current?.modules.length || busy} onClick={(event) => openAddEditor(event.currentTarget)}><Plus size={14} /> Add lesson</Button><Button type="button" disabled={busy || operations.length === 0 || !reason.trim()} onClick={previewRequest}><Sparkles size={13} /> Preview changes</Button></div></div>
      <div className="mt-4">
        <Input label="Why revise this curriculum?" value={reason} onChange={(event) => setReason(event.target.value)} />
      </div>
      {!current?.modules.length ? <p className="mt-4 rounded-xl border border-dashed border-border-default p-5 text-sm text-text-muted">No curriculum revision is available to edit.</p> : <div className="mt-5 space-y-4">{current.modules.map((module, moduleIndex) => <section key={module.id} className="rounded-xl border border-border-subtle bg-bg/40 p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><span className="text-[10px] font-semibold uppercase tracking-[.14em] text-text-muted">Module {String(moduleIndex + 1).padStart(2, '0')}</span><h4 className="mt-1 font-serif text-lg text-text-primary">{module.title}</h4><p className="mt-1 text-xs leading-5 text-text-secondary">{module.purpose}</p></div><span className="rounded-full border border-border-subtle bg-surface px-2.5 py-0.5 text-xs text-text-muted">{module.lessons.length} lessons</span></div><div className="mt-3 divide-y divide-border-subtle">{module.lessons.map((lesson) => <article key={lesson.id} className="flex flex-wrap items-center justify-between gap-3 py-3 first:pt-0 last:pb-0"><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><span className="text-sm font-medium text-text-primary">{lesson.title}</span><span className="rounded-full border border-border-subtle bg-surface px-2 py-0.5 text-[10px] capitalize text-text-muted">{lessonState(lesson.state)}</span>{lesson.assessmentStarted && <span className="text-[10px] text-warning-fg">Assessment started · protected</span>}</div><p className="mt-1 line-clamp-2 text-xs leading-5 text-text-secondary">{lesson.objective}</p><span className="mt-1 inline-block text-xs text-text-muted">{lesson.estimatedMinutes} min</span></div><div className="flex flex-wrap justify-end gap-1.5"><Button type="button" variant="outline" size="sm" onClick={(event) => openEditor({ content: { kind: 'learning.lesson', key: lesson.id }, path: null, fragment: null }, { returnFocusTo: event.currentTarget })}>Edit</Button><Button type="button" variant="outline" size="sm" disabled={busy || lesson.state === 'completed' || lesson.assessmentStarted} onClick={(event) => openEdit('move', lesson.id, event.currentTarget)} aria-label={`Move ${lesson.title}`}><ArrowUp size={12} /> Move</Button><Button type="button" variant="outline" size="sm" disabled={busy || lesson.state === 'completed' || lesson.assessmentStarted} onClick={() => queueLessonOperation('skip_lesson', lesson.id)}>Skip</Button><Button type="button" variant="outline" size="sm" disabled={busy || lesson.state === 'completed' || lesson.assessmentStarted} onClick={(event) => openEdit('replace', lesson.id, event.currentTarget)}>Replace</Button><Button type="button" variant="outline" size="sm" disabled={busy} onClick={() => queueLessonOperation('challenge_prerequisite', lesson.id)}>Challenge prereq</Button></div></article>)}</div></section>)}</div>}
      {operations.length > 0 && <div className="mt-4 rounded-xl border border-accent/25 bg-accent-muted/40 p-4"><div className="flex items-center justify-between gap-3"><h4 className="text-xs font-semibold text-text-primary">Proposed operations</h4><button type="button" onClick={() => setOperations([])} disabled={busy} className="text-xs text-text-muted underline">Clear all</button></div><ol className="mt-2 space-y-1 text-xs text-text-secondary">{operations.map((operation, index) => <li key={`${index}-${operation.kind}`} className="flex items-center justify-between gap-2"><span>{index + 1}. {operationName(operation)}{operation.kind !== 'add_lesson' ? ` · ${findLesson(plan, operation.lesson_id)?.lesson.title ?? operation.lesson_id.slice(0, 8)}` : ` · ${operation.lesson.title}`}</span><button type="button" aria-label={`Remove ${operationName(operation)} operation ${index + 1}`} disabled={busy} onClick={() => setOperations((current) => current.filter((_, i) => i !== index))} className="rounded p-1 text-text-muted hover:bg-surface"><X size={13} /></button></li>)}</ol></div>}
      <Dialog open={editMode !== null} onOpenChange={(open) => { if (!open) closeEditor(); }}>{editMode && <DialogContent hideClose aria-describedby={undefined} overlayClassName="z-40 bg-black/45" className="z-40 block max-w-lg space-y-3.5 rounded-2xl border border-border-default bg-surface p-5 shadow-2xl" onInteractOutside={(event) => event.preventDefault()}><DialogTitle asChild><h4 className="font-serif text-xl text-text-primary">{editMode === 'add' ? 'Add a lesson' : editMode === 'replace' ? 'Replace lesson' : 'Move lesson'}</h4></DialogTitle>{editMode !== 'move' ? <><Input autoFocus label="Title" value={title} onChange={(event) => setTitle(event.target.value)} /><Textarea label="Objective" value={objective} onChange={(event) => setObjective(event.target.value)} rows={3} /><NumberStepper label="Estimated minutes" min={1} max={240} step={5} suffix="min" value={minutes} onValueChange={setMinutes} /></> : <p className="text-sm text-text-secondary">Choose the destination module. A lesson with completed work or a started assessment is protected.</p>}<div><label htmlFor="plan-editor-module" className="mb-1.5 block text-xs font-medium text-text-secondary">Module</label><Select value={moduleId} onValueChange={setModuleId}><SelectTrigger id="plan-editor-module" aria-label="Module" autoFocus={editMode === 'move'}><SelectValue placeholder="Choose a module" /></SelectTrigger><SelectContent>{current?.modules.map((module) => <SelectItem key={module.id} value={module.id}>{module.title}</SelectItem>)}</SelectContent></Select></div><div className="flex justify-end gap-2 pt-2"><Button type="button" variant="outline" onClick={closeEditor}>Cancel</Button><Button type="button" disabled={!moduleId || (editMode !== 'move' && (!title.trim() || !objective.trim()))} onClick={saveLessonOperation}>Add to preview</Button></div></DialogContent>}</Dialog>
      {preview.isPending && <div role="status" className="mt-4 rounded-xl bg-bg p-4 text-xs text-text-muted">Building a semantic revision preview…</div>}
      {preview.error && <p role="alert" className="mt-4 rounded-xl bg-danger-muted/50 p-4 text-xs text-danger-fg">{message(preview.error)} <button type="button" onClick={previewRequest} className="ml-2 font-semibold underline">Retry preview</button></p>}
      {preview.data?.draftRevision && <section className="mt-5 rounded-xl border border-accent/25 bg-accent-muted/35 p-4 sm:p-5"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Semantic diff · revision {preview.data.draftRevision.revisionNumber}</div><h4 className="mt-1 font-serif text-xl text-text-primary">Review before accepting</h4></div><span className="rounded-full border border-border-subtle bg-surface px-3 py-1 text-xs text-text-muted">{preview.data.previewChanges.length} changes</span></div><ul className="mt-4 space-y-2">{preview.data.previewChanges.map((change, index) => <li key={`${change.lessonId}:${index}`} className="rounded-xl border border-border-subtle bg-surface p-3"><div className="flex flex-wrap items-center gap-2"><span className="rounded-full bg-accent-muted px-2 py-0.5 text-[10px] font-semibold uppercase text-accent">{change.kind}</span><span className="text-sm text-text-primary">{change.description}</span></div></li>)}</ul><p className="mt-4 text-xs text-text-secondary">Required lesson count: {preview.data.requiredLessonCountBefore} → {preview.data.requiredLessonCountAfter}. Resume lesson: {preview.data.resumeLessonId ? findLesson(preview.data, preview.data.resumeLessonId)?.lesson.title ?? 'Preserved by ID' : 'No eligible resume lesson'}.</p><div className="mt-4 flex flex-wrap justify-end gap-2"><Button type="button" variant="outline" disabled={busy} onClick={onDiscard}><X size={13} /> Discard draft</Button><Button type="button" disabled={busy} onClick={onAccept}><Check size={13} /> Accept revision</Button></div></section>}
      {(accept.error || discard.error) && <p role="alert" className="mt-4 text-xs text-danger-fg">{message(accept.error ?? discard.error)} · The current revision remains in place.</p>}
    </section>

    <section className="rounded-xl border border-border-subtle bg-surface p-5 sm:p-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Durable generation</div><h3 className="mt-1 font-serif text-xl text-text-primary">Job activity</h3></div><span className="text-xs text-text-muted">Progress is reported by the job itself</span></div>{plan.jobs.length === 0 ? <p className="mt-4 rounded-xl bg-bg p-4 text-sm text-text-muted">No generation jobs have been recorded.</p> : <div className="mt-4 space-y-3">{attempts.current.map((job) => <JobCard key={job.id} job={job} programId={programId} revision={expectedRevision} />)}<EarlierAttempts jobs={attempts.earlier} /></div>}</section>
  </div>;
}
