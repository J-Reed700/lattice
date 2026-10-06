import { Suspense, lazy, useEffect, useRef, useState } from 'react';

import { ArrowLeft, ArrowRight, Check, CheckCircle2, Clock3, ExternalLink, FileText, Layers3, Play } from 'lucide-react';

import { MarkdownViewer } from '@/features/chat/components/viewers/MarkdownViewer';
import { AssessmentEvidencePanel } from '@/features/learning/assessment/AssessmentEvidencePanel';
import { AssessmentPanel, AttemptReview, type FormSnapshot } from '@/features/learning/assessment/AssessmentPanel';
import { CourseCoach } from '@/features/learning/curriculum/CourseCoach';
import { type CourseStep } from '@/features/learning/curriculum/courseProgress';
import { CourseSyllabus } from '@/features/learning/curriculum/CourseSyllabus';
import { OutlineReviewPanel, type OutlineOperation } from '@/features/learning/curriculum/OutlineReviewPanel';
import { PlanPanel } from '@/features/learning/curriculum/PlanPanel';
import { StartingPointPanel } from '@/features/learning/curriculum/StartingPointPanel';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { LessonEvidencePanel } from '@/features/learning/lessons/LessonEvidencePanel';
import { LessonPreparationProgress } from '@/features/learning/lessons/LessonPreparationProgress';
import { NotebookPanel, RecallPanel } from '@/features/learning/memory/MemoryPanels';
import { useLearningMemory } from '@/features/learning/memory/useLearningMemory';
import { PortabilityPanel } from '@/features/learning/portability/PortabilityPanel';
import { PracticalWorkbenchPanel } from '@/features/learning/practice/practical/PracticalWorkbenchPanel';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { RecallEvolutionPanel } from '@/features/learning/recall/RecallEvolutionPanel';
import { SourceMaintenancePanel } from '@/features/learning/sources/SourceMaintenancePanel';
import { SourcesPanel } from '@/features/learning/sources/SourcesPanel';
import { useRepairLearningOutline } from '@/features/learning/workspace/useLearningStudio';
import { useEffectiveTheme } from '@/hooks/useApplyTheme';
import type { LearningBlockKind, LearningAssessmentKind, LearningLessonDto, LearningModuleDto, LearningProgramDto, LearningSourceDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';
import type { StudyDestination, StudyTab } from '@/lib/studyActivity';

const LazyCanvasPanel = lazy(() => import('@/features/learning/canvas/CanvasPanel'));

type Tab = StudyTab;
type NotebookDraft = { title: string; content: string };

function safeHref(value?: string | null) {
  if (!value) return undefined;
  try { const url = new URL(value); return ['https:', 'http:'].includes(url.protocol) ? url.href : undefined; }
  catch { return undefined; }
}

function formatDateTime(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'date unavailable' : date.toLocaleString();
}

function SourceLine({ ids, sources }: { ids: string[]; sources: LearningSourceDto[] }) {
  const entries = ids.map((id) => sources.find((source) => source.id === id)).filter((item): item is LearningSourceDto => Boolean(item));
  if (!entries.length) return null;
  return <div className="mt-4 space-y-2">{entries.map((source) => { const href = safeHref(source.url); return <details key={source.id} className="group rounded-xl border border-border/70 bg-background"><summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2.5 text-xs text-text-secondary"><FileText size={14} className="shrink-0 text-accent" /><span className="flex-1 truncate">{source.title}</span>{href && <a href={href} target="_blank" rel="noreferrer" aria-label={`Open ${source.title}`} onClick={(e) => e.stopPropagation()} className="text-accent hover:underline"><ExternalLink size={13} /></a>}<span className="text-text-muted">Source</span></summary><div className="border-t border-border px-4 py-3"><p className="whitespace-pre-wrap text-xs leading-5 text-text-secondary">{source.excerpt}</p><p className="mt-2 text-xs text-text-muted">Acquired {formatDateTime(source.acquiredAt)}{href && <> · <a href={href} target="_blank" rel="noreferrer" className="text-accent hover:underline">Open reference</a></>}</p></div></details>; })}</div>;
}

const blockLabels: Record<LearningBlockKind, string> = {
  explanation: 'Understand the idea', worked_example: 'Worked example', guided_practice: 'Try it with guidance',
  independent_practice: 'Your assignment', reflection: 'Reflect & connect', recap: 'Key takeaways',
};

function LessonCard({ lesson, program, isCurrent, selected, pending, onPrepare, onComplete, onSelect, onPractice }: {
  lesson: LearningLessonDto; program: LearningProgramDto; isCurrent: boolean; selected: boolean; pending: boolean;
  onPrepare: () => void; onComplete: () => void; onSelect: () => void; onPractice: () => void;
}) {
  const [expanded, setExpanded] = useState(isCurrent && lesson.preparation === 'ready');
  const sources = program.sources;
  useEffect(() => { if (selected) setExpanded(true); }, [selected, lesson.preparation]);
  return <article id={`lesson-${lesson.id}`} className={`overflow-hidden rounded-2xl border bg-surface transition ${isCurrent ? 'border-accent/50 shadow-[0_8px_34px_-24px_hsl(var(--accent))]' : 'border-border/70 shadow-sm'}`}>
    <button type="button" onClick={() => { onSelect(); setExpanded((v) => !v); }} className="flex w-full items-start gap-4 p-5 text-left sm:p-6"><span className={`mt-0.5 grid h-9 w-9 shrink-0 place-items-center rounded-full ${lesson.completed ? 'bg-emerald-500/15 text-emerald-700' : lesson.preparation === 'ready' ? 'bg-accent/10 text-accent' : 'bg-background text-text-muted'}`}>{lesson.completed ? <Check size={16} /> : <span className="font-serif text-base">{lesson.preparation === 'ready' ? <Play size={14} /> : '·'}</span>}</span><span className="min-w-0 flex-1"><span className="flex flex-wrap items-center gap-2"><span className="font-serif text-xl text-text-primary">{lesson.title}</span>{isCurrent && <span className="rounded-full bg-accent/10 px-2 py-0.5 text-xs font-semibold uppercase tracking-wider text-accent">Resume here</span>}</span><span className="mt-1 block text-sm leading-6 text-text-secondary">{lesson.objective || (lesson.preparation === 'outline' ? 'A lesson in the program outline. Prepare it when you are ready.' : 'A focused lesson with explanation and worked example.')}</span><span className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-text-muted"><span className="inline-flex shrink-0 whitespace-nowrap items-center gap-1.5"><Clock3 size={13} />About {lesson.estimatedMinutes} min</span><span className="shrink-0 whitespace-nowrap rounded-full bg-background px-2 py-0.5">{lesson.preparation === 'ready' ? 'Ready' : 'Outline'}</span><span className="shrink-0 whitespace-nowrap">{lesson.completed ? 'Self-reported complete' : 'Not marked complete'}</span></span></span><span className="mt-2 shrink-0 text-xs text-text-muted">{expanded ? 'Close' : 'Open'}</span></button>
    {expanded && <div className="border-t border-border bg-background/45 px-5 py-6 sm:px-9">
      {lesson.preparation === 'outline' ? <div className="flex flex-wrap items-center justify-between gap-4"><div><h4 className="font-medium text-text-primary">Ready when you are</h4><p className="mt-1 text-sm text-text-secondary">This prepares this lesson only. The accepted outline stays in place if preparation fails.</p></div><button type="button" disabled={pending || program.summary.status !== 'active'} onClick={onPrepare} className="rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50">{program.summary.status !== 'active' ? 'Accept the course to begin' : pending ? 'Preparing…' : 'Prepare lesson'}</button></div> : <>
        <LessonEvidencePanel key={lesson.id} programId={program.summary.id} lessonId={lesson.id} />
        <nav aria-label={`Sections in ${lesson.title}`} className="mb-5 flex flex-wrap gap-2">{lesson.blocks.map((block, index) => <a key={index} href={`#lesson-${lesson.id}-section-${index}`} className="rounded-full border border-border bg-surface px-3 py-2 text-xs text-text-secondary hover:border-accent">{index + 1}. {block.title}</a>)}</nav>
        <div className="space-y-4">{lesson.blocks.map((block, i) => <section id={`lesson-${lesson.id}-section-${i}`} key={`${lesson.id}-${i}`} className="rounded-xl border border-border/70 bg-surface p-5"><div className="text-xs font-semibold uppercase tracking-[.12em] text-accent">{blockLabels[block.kind]}</div><h4 className="mt-1 font-serif text-lg text-text-primary">{block.title}</h4><div className="prose prose-sm mt-3 max-w-none text-text-secondary dark:prose-invert"><MarkdownViewer content={block.body} /></div><SourceLine ids={block.sourceIds} sources={sources} />{block.kind === 'guided_practice' && selected && program.summary.status === 'active' && <div id={`guided-${lesson.id}`}><PracticeWorkbenchPanel key={lesson.id} program={program} lesson={lesson} taskKind="guided" embedded onContinue={onPractice} /></div>}{block.kind === 'independent_practice' && <button type="button" onClick={onPractice} className="mt-5 inline-flex min-h-11 items-center gap-2 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg">Work on this assignment <ArrowRight size={15} /></button>}</section>)}</div>
        <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-5"><button type="button" onClick={onPractice} className="inline-flex min-h-11 items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-sm font-semibold text-accent-fg hover:brightness-95">Practice this lesson <ArrowRight size={15} /></button><div className="flex flex-wrap items-center gap-3"><p className="max-w-lg text-xs leading-5 text-text-muted">Reading the lesson does not count as completion.</p><button type="button" disabled={lesson.completed || pending} onClick={onComplete} className="inline-flex min-h-11 items-center gap-2 rounded-full border border-accent/40 px-4 py-2.5 text-sm font-medium text-accent hover:bg-accent/5 disabled:opacity-45">{lesson.completed ? <><CheckCircle2 size={15} /> Marked complete</> : pending ? 'Saving…' : <>Mark lesson complete <Check size={15} /></>}</button></div></div>
      </>}
    </div>}
  </article>;
}

export function ProgramWorkspace({ program, destination, outlineGeneration, onBack, onAccept, acceptPending, acceptError, onPrepare, preparePending, prepareError, onComplete, completePending, completeError, formFor, setAnswer, onSubmitAttempt, submitPending, submitError }: {
  destination?: StudyDestination;
  outlineGeneration?: OutlineOperation;
  program: LearningProgramDto; onBack: () => void; onAccept: (title: string) => void; acceptPending: boolean; acceptError?: string;
  onPrepare: (lesson: LearningLessonDto) => void; preparePending: boolean; prepareError?: string;
  onComplete: (lesson: LearningLessonDto) => void; completePending: boolean; completeError?: string;
  formFor: (key: string, questions: { id: string }[]) => FormSnapshot; setAnswer: (key: string, questionId: string, value: number) => void;
  onSubmitAttempt: (key: string, kind: LearningAssessmentKind, module: LearningModuleDto, lesson: LearningLessonDto | null, questions: LearningLessonDto['questions'], form: FormSnapshot, attemptId: string) => void;
  submitPending: boolean; submitError?: string;
}) {
  const [moduleId, setModuleId] = useState(() => program.modules.find((m) => m.lessons.some((l) => l.id === program.summary.currentLessonId))?.id ?? program.modules[0]?.id ?? '');
  const [lessonId, setLessonId] = useState(program.summary.currentLessonId ?? program.modules[0]?.lessons[0]?.id ?? '');
  const [tab, setTab] = useState<Tab>('lessons');
  const [lastPracticeTab, setLastPracticeTab] = useState<Tab>('workbench');
  const [moreOpen, setMoreOpen] = useState(false);
  const moreButtonRef = useRef<HTMLButtonElement>(null);
  const moreMenuRef = useRef<HTMLDivElement>(null);
  const [quickKind, setQuickKind] = useState<LearningAssessmentKind>('practice');
  const [showQuickResults, setShowQuickResults] = useState(false);
  const [canvasOpened, setCanvasOpened] = useState(false);
  const [practicalOpened, setPracticalOpened] = useState(false);
  const [requestedFormId, setRequestedFormId] = useState<string | null>(null);
  const [projectRequest, setProjectRequest] = useState<{ id: string; brief: string } | null>(null);
  const [writtenOpened, setWrittenOpened] = useState(false);
  const openedDestination = useRef<StudyDestination | undefined>(undefined);
  useEffect(() => {
    if (!destination?.tab || openedDestination.current === destination) return;
    openedDestination.current = destination;
    const next = destination.tab;
    const targetModule = program.modules.find(module => destination.lessonId ? module.lessons.some(lesson => lesson.id === destination.lessonId) : module.id === destination.moduleId);
    if (targetModule) {
      setModuleId(targetModule.id);
      setLessonId(destination.lessonId ?? targetModule.lessons[0]?.id ?? '');
    }
    if (destination.formId) setRequestedFormId(destination.formId);
    setTab(next);
    setMoreOpen(false);
    if (next === 'canvas') setCanvasOpened(true);
    if (next === 'practical') setPracticalOpened(true);
    if (next === 'workbench') setWrittenOpened(true);
    if (['workbench', 'practical', 'quick-checks', 'assess-evidence'].includes(next)) setLastPracticeTab(next);
  }, [destination, program.modules]);
  const [title, setTitle] = useState(program.summary.title);
  const [notebookDrafts, setNotebookDrafts] = useState<Record<string, NotebookDraft>>({});
  const [savingBeforeMove, setSavingBeforeMove] = useState(false);
  const repair = useRepairLearningOutline(program.summary.id);
  const outlineBlocked = repair.isPending || outlineGeneration?.isPending || Boolean(program.outlineReview && program.outlineReview.status !== 'passed');
  const activeProgram = program.summary.status === 'active';
  const preparationPlan = useLearningPlan(activeProgram ? program.summary.id : '', preparePending);
  const preparationJobs = preparationPlan.data?.jobs?.filter((job) => job.kind === 'lesson_preparation').sort((a, b) => b.createdAt - a.createdAt) ?? [];
  const preparationJob = preparationJobs.find((job) => job.status === 'running') ?? preparationJobs.find((job) => job.status === 'pending') ?? preparationJobs[0];
  const preparingLesson = preparePending || Boolean(preparationPlan.data?.jobs?.some((job) => ['pending', 'running'].includes(job.status)));
  const memoryQuery = useLearningMemory(program.summary.id, activeProgram);
  const theme = useEffectiveTheme();
  const currentModule = program.modules.find((m) => m.id === moduleId) ?? program.modules[0];
  const currentLesson = lessonId ? (currentModule?.lessons.find((l) => l.id === lessonId) ?? currentModule?.lessons[0]) : undefined;
  const allLessons = program.modules.flatMap((m) => m.lessons.map((lesson) => ({ module: m, lesson })));
  const completedCount = allLessons.filter(({ lesson }) => lesson.completed).length;
  const readyCount = allLessons.filter(({ lesson }) => lesson.preparation === 'ready').length;
  const practiceCount = program.attempts.filter((a) => a.kind === 'practice').length;
  const demonstratedCount = program.attempts.filter((a) => a.kind === 'test').length;
  const moduleTestQuestions = currentModule?.lessons.flatMap((lesson) => lesson.questions.filter((q) => q.kind === 'test')) ?? [];
  const testReady = Boolean(currentModule?.lessons.length && currentModule.lessons.every((lesson) => lesson.preparation === 'ready'));
  const moduleAttempts = program.attempts.filter((a) => a.moduleId === currentModule?.id);
  const resume = allLessons.find(({ lesson }) => lesson.id === program.summary.currentLessonId) ?? allLessons.find(({ lesson }) => !lesson.completed);
  const primary = [{ id: 'lessons' as const, label: 'Lessons' }, { id: 'workbench' as const, label: 'Practice' }, { id: 'recall' as const, label: 'Recall' }, { id: 'notebook' as const, label: 'Notebook' }, { id: 'sources' as const, label: 'Sources' }];
  const activateTab = (next: Tab) => {
    if (next === 'canvas') setCanvasOpened(true);
    if (next === 'practical') setPracticalOpened(true);
    if (next === 'workbench') setWrittenOpened(true);
    if (next === 'workbench' || next === 'practical' || next === 'quick-checks' || next === 'assess-evidence') setLastPracticeTab(next);
    setMoreOpen(false);
    setTab(next);
  };
  const selectTab = (next: Tab) => {
    void afterNotebookSave(() => activateTab(next));
  };
  const selectPracticeGroup = () => selectTab(lastPracticeTab);
  const selectMoreTab = (next: Tab) => {
    void afterNotebookSave(() => {
      activateTab(next);
      requestAnimationFrame(() => document.getElementById('workspace-panel')?.focus());
    });
  };
  const practiceTab = tab === 'workbench' || tab === 'practical' || tab === 'quick-checks' || tab === 'assess-evidence';
  useEffect(() => {
    if (!moreOpen) return;
    const first = moreMenuRef.current?.querySelector<HTMLElement>('[role="menuitem"]');
    first?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setMoreOpen(false);
        moreButtonRef.current?.focus();
      }
    };
    document.addEventListener('keydown', closeOnEscape);
    return () => document.removeEventListener('keydown', closeOnEscape);
  }, [moreOpen]);

  const goToStep = (step: CourseStep) => {
    void afterNotebookSave(() => {
      const target = allLessons.find(({ lesson }) => lesson.id === step.lessonId);
      if (target) { setModuleId(target.module.id); setLessonId(target.lesson.id); }
      if (step.kind === 'starting_point') activateTab('starting_point');
      else if (step.kind === 'assessment') { setRequestedFormId(step.formId ?? null); activateTab('assess-evidence'); }
      else if (step.kind === 'assignment') activateTab('workbench');
      else if (step.kind === 'recall') activateTab('recall');
      else {
        activateTab('lessons');
        if (target?.lesson.preparation === 'outline' && !preparingLesson) onPrepare(target.lesson);
        if (step.kind === 'guided' && target) requestAnimationFrame(() => document.getElementById(`guided-${target.lesson.id}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' }));
      }
    });
  };

  const questionBundle = (kind: LearningAssessmentKind) => {
    if (kind === 'test') return moduleTestQuestions;
    return currentLesson?.questions.filter((question) => question.kind === kind) ?? [];
  };
  const formKey = (kind: LearningAssessmentKind) => `${program.summary.id}:${currentModule?.id}:${kind === 'test' ? 'module' : currentLesson?.id ?? 'none'}:${kind}`;
  const chosenQuestions = questionBundle(quickKind);
  const assessmentKey = formKey(quickKind);
  const form = formFor(assessmentKey, chosenQuestions);
  const afterNotebookSave = async (move: () => void) => {
    if (savingBeforeMove) return;
    setSavingBeforeMove(true);
    const saved = await flushPendingSaves();
    setSavingBeforeMove(false);
    if (!saved) return;
    move();
  };

  return <div className="mx-auto max-w-[1500px] pb-12">
    <div className="flex items-center justify-between gap-3"><button type="button" disabled={savingBeforeMove} onClick={() => void afterNotebookSave(onBack)} className="mb-2 inline-flex min-h-10 items-center gap-2 text-sm text-text-muted hover:text-text-primary disabled:opacity-50"><ArrowLeft size={16} /> {savingBeforeMove ? 'Saving your work…' : 'All programs'}</button>{activeProgram && <CourseSyllabus compact program={program} moduleId={moduleId} onSelect={(module, selectedLessonId) => void afterNotebookSave(() => { setModuleId(module.id); setLessonId(selectedLessonId); setTab('lessons'); requestAnimationFrame(() => document.getElementById(`lesson-${selectedLessonId}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })); })} />}</div>
    <header className="relative overflow-hidden rounded-2xl border border-[#d7c9b8] bg-[#eee8df] px-4 py-3 sm:px-6 dark:border-white/10 dark:bg-[#28251f]"><div className="relative flex flex-wrap items-center justify-between gap-x-6 gap-y-2"><div className="min-w-0 flex-1 basis-full sm:basis-0"><div className="flex items-center gap-2 text-xs font-medium text-accent"><Layers3 size={15} /> {program.summary.status === 'draft' ? 'Draft outline' : 'Active program'} <span className="text-text-muted">· Revision {program.summary.revision}</span></div>{program.summary.status === 'draft' ? <><h1 className="sr-only">{title || 'Untitled program'}</h1><label className="sr-only" htmlFor="program-title">Program title</label><input id="program-title" aria-label="Program title" value={title} onChange={(e) => setTitle(e.target.value)} className="mt-1 w-full max-w-2xl border-b border-accent/35 bg-transparent font-serif text-2xl leading-tight text-text-primary outline-none focus:border-accent" /></> : <h1 className="mt-1 line-clamp-2 break-words font-serif text-2xl leading-tight text-text-primary sm:text-[1.75rem]">{program.summary.title}</h1>}<p className="mt-1 line-clamp-1 max-w-3xl text-sm leading-5 text-text-secondary">{program.summary.goal}</p></div><div className="w-full shrink-0 sm:w-[150px]"><div className="flex items-center justify-between gap-2 text-xs"><span className="text-text-secondary">Lessons complete</span><span className="shrink-0 whitespace-nowrap font-semibold tabular-nums text-text-primary">{completedCount}<span className="font-normal text-text-muted"> / {allLessons.length}</span></span></div><div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-black/10 dark:bg-white/10"><div className="h-full rounded-full bg-accent transition-all" style={{ width: `${allLessons.length ? completedCount / allLessons.length * 100 : 0}%` }} /></div></div></div>
      {program.summary.status === 'draft' && <div className="relative mt-2 flex flex-wrap items-center justify-between gap-2 border-t border-black/10 pt-2 dark:border-white/10"><p className="text-sm text-text-secondary">{outlineBlocked ? 'Resolve the outline findings before accepting this course.' : 'Accept to prepare lessons individually and track progress.'}</p><div className="flex items-center gap-3">{acceptError && <span role="alert" className="max-w-xs text-sm text-rose-700">{acceptError}</span>}<button type="button" disabled={acceptPending || outlineBlocked || !title.trim()} onClick={() => onAccept(title.trim())} className="min-h-11 rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50">{acceptPending ? 'Saving…' : 'Accept program'}</button></div></div>}
    </header>
    <OutlineReviewPanel program={program} generation={outlineGeneration} repair={repair} />

    {!program.sources.length && <p className="mt-3 text-xs leading-5 text-text-muted">Topic-based course · AI-authored from general knowledge. Add references in Sources to ground future lessons.</p>}
    {!activeProgram && <CourseSyllabus program={program} moduleId={moduleId} onSelect={(module, selectedLessonId) => void afterNotebookSave(() => { setModuleId(module.id); setLessonId(selectedLessonId); setTab('lessons'); requestAnimationFrame(() => document.getElementById(`lesson-${selectedLessonId}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })); })} />}
    <div className="mt-2 grid grid-cols-1 gap-2">
      <div className="flex flex-wrap items-center gap-3 rounded-xl border border-border/70 bg-surface px-3 py-2"><div className="flex min-w-[210px] flex-1 items-center gap-2"><label htmlFor="module-select" className="inline-flex shrink-0 items-center gap-2 text-sm font-medium text-text-secondary"><Layers3 size={16} className="text-accent" /> Module</label><select id="module-select" aria-label="Module" value={currentModule?.id ?? ''} disabled={savingBeforeMove} onChange={(event) => { const next = program.modules.find((item) => item.id === event.target.value); if (!next) return; void afterNotebookSave(() => { setModuleId(next.id); setLessonId(next.lessons.find((lesson) => !lesson.completed)?.id ?? next.lessons[0]?.id ?? ''); setTab('lessons'); }); }} className="min-h-11 min-w-0 flex-1 overflow-hidden rounded-lg border border-border bg-background px-3 text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">{program.modules.map((module, index) => <option key={module.id} value={module.id}>Module {index + 1}: {module.title} ({module.lessons.filter((item) => item.completed).length}/{module.lessons.length} complete)</option>)}</select></div><div className="flex min-w-[210px] flex-1 items-center gap-2"><label htmlFor="lesson-select" className="shrink-0 text-sm font-medium text-text-secondary">Lesson</label><select id="lesson-select" aria-label="Lesson context" value={currentLesson?.id ?? ''} disabled={savingBeforeMove || !currentModule?.lessons.length} onChange={(event) => { const nextLessonId = event.target.value; void afterNotebookSave(() => setLessonId(nextLessonId)); }} className="min-h-11 min-w-0 flex-1 overflow-hidden rounded-lg border border-border bg-background px-3 text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"><option value="">No lesson selected</option>{currentModule?.lessons.map((lesson, index) => <option key={lesson.id} value={lesson.id}>{index + 1}. {lesson.title}</option>)}</select></div>{activeProgram ? <CourseCoach program={program} dueCount={memoryQuery.data?.dueCount ?? 0} disabled={savingBeforeMove || preparePending} onStep={goToStep} /> : resume && <button type="button" aria-label={`Continue lesson: ${resume.lesson.title}`} title={resume.lesson.title} disabled={savingBeforeMove} onClick={() => void afterNotebookSave(() => { setModuleId(resume.module.id); setLessonId(resume.lesson.id); setTab('lessons'); })} className="inline-flex min-h-11 shrink-0 items-center gap-2 whitespace-nowrap rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg hover:brightness-95 disabled:opacity-50">Continue lesson <ArrowRight size={15} /></button>}</div>

      <main className="min-w-0"><section id="workspace-panel" tabIndex={-1} aria-label="Program workspace content" className="rounded-2xl border border-border bg-surface p-3 sm:p-4">
        <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-1"><div className="min-w-0"><h2 className="truncate font-serif text-xl text-text-primary">{currentModule?.title}</h2><p className="mt-0.5 line-clamp-1 text-sm leading-5 text-text-secondary">{currentModule?.summary}</p></div><details className="group relative shrink-0 text-sm"><summary className="min-h-10 cursor-pointer list-none rounded-full border border-border px-3 py-2 text-text-secondary hover:border-accent/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">Module details <span className="text-accent">⌄</span></summary><div className="absolute right-0 z-20 mt-2 w-[min(24rem,calc(100vw-2rem))] rounded-xl border border-border bg-surface p-4 shadow-xl"><h3 className="font-medium text-text-primary">Outcomes</h3><ul className="mt-2 list-disc space-y-1 pl-5 text-sm leading-5 text-text-secondary">{currentModule?.outcomes.map((outcome) => <li key={outcome}>{outcome}</li>)}</ul></div></details></div>
        <nav aria-label="Module workspace" className="mt-2 border-y border-border"><div role="tablist" aria-label="Program workspace" className="flex flex-wrap items-center gap-1 py-1">{primary.filter((item) => activeProgram || item.id === 'lessons' || item.id === 'sources').map((item) => <button key={item.id} id={`workspace-tab-${item.id}`} aria-controls="workspace-panel" role="tab" aria-selected={tab === item.id || (item.id === 'workbench' && practiceTab)} tabIndex={tab === item.id || (item.id === 'workbench' && practiceTab) ? 0 : -1} type="button" disabled={savingBeforeMove} onClick={() => item.id === 'workbench' ? selectPracticeGroup() : selectTab(item.id)} onKeyDown={(event) => { if (!['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(event.key)) return; event.preventDefault(); const buttons = Array.from(event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? []); const index = buttons.indexOf(event.currentTarget); const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : buttons.length - 1)) % buttons.length; buttons[next]?.focus(); }} className={`relative min-h-11 rounded-lg px-3 text-sm font-medium transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 ${tab === item.id || (item.id === 'workbench' && practiceTab) ? 'bg-accent/10 text-accent' : 'text-text-secondary hover:bg-background hover:text-text-primary'}`}>{item.label}{item.id === 'recall' && memoryQuery.data?.dueCount ? <span className="ml-1.5 rounded-full bg-accent/10 px-1.5 py-0.5 text-xs text-accent">{memoryQuery.data.dueCount}</span> : null}</button>)}{activeProgram && <div className="relative ml-auto"><button ref={moreButtonRef} type="button" aria-haspopup="menu" aria-expanded={moreOpen} aria-current={['plan', 'canvas', 'portability'].includes(tab) ? 'page' : undefined} aria-controls="workspace-more-menu" onClick={() => setMoreOpen((value) => !value)} className="min-h-11 rounded-lg px-3 text-sm font-medium text-text-secondary hover:bg-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">More <span aria-hidden="true">⌄</span></button>{moreOpen && <div id="workspace-more-menu" ref={moreMenuRef} role="menu" aria-label="More workspace tools" onKeyDown={(event) => { if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return; event.preventDefault(); const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')); const index = items.indexOf(document.activeElement as HTMLButtonElement); const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length; items[next]?.focus(); }} className="absolute right-0 top-full z-30 mt-1 w-52 rounded-xl border border-border bg-surface p-1.5 shadow-xl">{([{ id: 'plan' as const, label: 'Plan' }, { id: 'canvas' as const, label: 'Canvas' }, { id: 'portability' as const, label: 'Import & export' }]).map((item) => <button key={item.id} type="button" role="menuitem" aria-current={tab === item.id ? 'page' : undefined} onClick={() => selectMoreTab(item.id)} className="flex min-h-11 w-full items-center rounded-lg px-3 text-left text-sm text-text-primary hover:bg-background focus-visible:bg-background focus-visible:outline-none">{item.label}</button>)}</div>}</div>}</div></nav>
        {practiceTab && <><div className="mt-3 flex flex-wrap items-center justify-between gap-2"><div className="text-sm text-text-secondary"><span className="font-medium text-text-primary">{currentModule?.title}</span>{currentLesson && <> <span aria-hidden="true">›</span> {currentLesson.title}</>}</div><div role="tablist" aria-label="Practice activities" className="flex flex-wrap gap-1 rounded-xl bg-background p-1">{([{ id: 'workbench' as const, label: 'Written practice' }, { id: 'practical' as const, label: 'Code & simulations' }, { id: 'quick-checks' as const, label: 'Quick checks' }, { id: 'assess-evidence' as const, label: 'Assessments' }]).map((item) => <button key={item.id} id={`practice-tab-${item.id}`} aria-controls="practice-content" role="tab" aria-selected={tab === item.id} tabIndex={tab === item.id ? 0 : -1} type="button" disabled={savingBeforeMove} onClick={() => selectTab(item.id)} onKeyDown={(event) => { if (!['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(event.key)) return; event.preventDefault(); const buttons = Array.from(event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? []); const index = buttons.indexOf(event.currentTarget); const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : buttons.length - 1)) % buttons.length; buttons[next]?.focus(); }} className={`min-h-10 rounded-lg px-3 text-sm font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${tab === item.id ? 'bg-surface text-accent shadow-sm' : 'text-text-secondary hover:text-text-primary'}`}>{item.label}</button>)}</div></div></>}

        {activeProgram && tab === 'starting_point' && <div className="mt-4"><StartingPointPanel programId={program.summary.id} onStudy={(lessonId) => goToStep({ kind: 'lesson', title: '', reason: '', lessonId })} onChallenge={(lessonId) => goToStep({ kind: 'assessment', title: '', reason: '', lessonId })} /></div>}
        {activeProgram && <LessonPreparationProgress job={preparationJob} pending={preparePending} programId={program.summary.id} revision={program.summary.revision} error={prepareError} />}
        {activeProgram && preparationPlan.isError && preparePending && <p role="alert" className="my-3 text-sm text-rose-700">Progress could not be loaded. <button type="button" className="underline" onClick={() => void preparationPlan.refetch()}>Retry progress</button></p>}
        {tab === 'lessons' && <div className="mt-3 space-y-3">{!currentModule?.lessons.length ? <p className="rounded-xl bg-background p-6 text-sm text-text-muted">This module has no lessons in its outline.</p> : currentModule.lessons.map((lesson) => <div key={lesson.id} className="relative"><div className="absolute -left-0.5 top-11 hidden h-[calc(100%+12px)] w-px bg-border last:hidden sm:block" /><div className="absolute -left-[3px] top-8 hidden h-1.5 w-1.5 rounded-full bg-accent sm:block" /><div className="sm:pl-5"><LessonCard lesson={lesson} program={program} selected={lesson.id === lessonId} isCurrent={lesson.id === program.summary.currentLessonId || (lesson.id === resume?.lesson.id && !program.summary.currentLessonId)} pending={completePending || (lesson.preparation === 'outline' && preparingLesson)} onPrepare={() => onPrepare(lesson)} onComplete={() => onComplete(lesson)} onSelect={() => void afterNotebookSave(() => setLessonId(lesson.id))} onPractice={() => void afterNotebookSave(() => { setLessonId(lesson.id); setTab('workbench'); setWrittenOpened(true); setLastPracticeTab('workbench'); })} /></div></div>)}</div>}
        {activeProgram && tab === 'lessons' && <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label="Module milestones"><div className="rounded-xl border border-border bg-background/50 p-4"><span className="text-xs font-semibold uppercase tracking-wide text-accent">01 · Learn</span><p className="mt-2 text-sm text-text-primary">{currentModule?.lessons.filter((item) => item.completed).length ?? 0} of {currentModule?.lessons.length ?? 0} lessons complete</p><p className="mt-1 text-xs leading-5 text-text-muted">Build understanding, work through examples, and complete each assignment.</p></div><button type="button" onClick={() => selectTab('workbench')} className="rounded-xl border border-border bg-background/50 p-4 text-left hover:border-accent/40"><span className="text-xs font-semibold uppercase tracking-wide text-accent">02 · Apply</span><span className="mt-2 block text-sm text-text-primary">Practice with feedback →</span><span className="mt-1 block text-xs leading-5 text-text-muted">Submit your assignment, get criterion feedback, and refine your reasoning.</span></button><button type="button" onClick={() => selectTab('assess-evidence')} className="rounded-xl border border-border bg-background/50 p-4 text-left hover:border-accent/40"><span className="text-xs font-semibold uppercase tracking-wide text-accent">03 · Check</span><span className="mt-2 block text-sm text-text-primary">Module assessment →</span><span className="mt-1 block text-xs leading-5 text-text-muted">{testReady ? 'Explain and apply the module outcomes in a written checkpoint.' : 'Study first, or challenge the module if you already know the material.'}</span></button><button type="button" disabled={!currentModule?.lessons.some((item) => item.preparation === 'ready')} onClick={() => void afterNotebookSave(() => {
          if (!currentModule) return;
          const anchor = [...currentModule.lessons].reverse().find((item) => item.preparation === 'ready');
          if (!anchor) return;
          setLessonId(anchor.id);
          const finalModule = currentModule.id === program.modules.at(-1)?.id;
          const scope = finalModule ? program.modules : [currentModule];
          setProjectRequest({ id: crypto.randomUUID(), brief: `Create a ${finalModule ? 'final capstone integrating the complete course' : 'module project'} for ${program.summary.goal}. Project progression: ${scope.map((item) => item.project ? `${item.project.title}: ${item.project.brief} Deliverables: ${item.project.deliverables.join('; ')}. Success criteria: ${item.project.successCriteria.join('; ')}` : item.summary).join(' | ')}. Each milestone must extend the same project artifact and include a revision after feedback. Outcomes: ${scope.flatMap((item) => item.outcomes).join('; ')}. Require a concrete artifact, milestones, constraints, an explicit grading rubric, and a reflection on tradeoffs. Build on these lessons: ${scope.flatMap((item) => item.lessons.map((lesson) => lesson.title)).join('; ')}.`.slice(0, 4000) });
          activateTab('practical');
        })} className="rounded-xl border border-border bg-background/50 p-4 text-left hover:border-accent/40 disabled:opacity-50"><span className="text-xs font-semibold uppercase tracking-wide text-accent">04 · Build</span><span className="mt-2 block text-sm text-text-primary">{currentModule?.id === program.modules.at(-1)?.id ? 'Create your capstone →' : 'Create a module project →'}</span><span className="mt-1 block text-xs leading-5 text-text-muted">A project brief shaped by these outcomes, with deliverables and a rubric. Prepare a lesson to begin.</span></button></div>}
        {activeProgram && tab === 'plan' && <div className="mt-5"><PlanPanel programId={program.summary.id} onStartingPoint={() => selectTab('starting_point')} /></div>}
        {activeProgram && tab === 'portability' && <div className="mt-5"><PortabilityPanel programId={program.summary.id} programTitle={program.summary.title} /></div>}
        {activeProgram && (practiceTab || writtenOpened || practicalOpened) && <div id="practice-content" role="tabpanel" tabIndex={0} aria-labelledby={`practice-tab-${practiceTab ? tab : lastPracticeTab}`} hidden={!practiceTab} className="mt-3">
        {activeProgram && tab === 'quick-checks' && <div className="mt-5"><div className="mb-4 flex flex-wrap items-center justify-between gap-3"><div className="flex flex-wrap gap-2" role="group" aria-label="Quick check type">{(['practice', 'quiz', 'test'] as LearningAssessmentKind[]).map((kind) => <button key={kind} type="button" aria-pressed={!showQuickResults && quickKind === kind} onClick={() => { setShowQuickResults(false); setQuickKind(kind); }} className={`rounded-full border px-3 py-2 text-xs font-medium capitalize ${!showQuickResults && quickKind === kind ? 'border-accent bg-accent text-accent-fg' : 'border-border text-text-secondary'}`}>{kind === 'test' ? 'Module test' : kind}</button>)}<button type="button" aria-pressed={showQuickResults} onClick={() => setShowQuickResults(true)} className={`rounded-full border px-3 py-2 text-xs font-medium ${showQuickResults ? 'border-accent bg-accent text-accent-fg' : 'border-border text-text-secondary'}`}>History ({moduleAttempts.length})</button></div><p className="text-sm text-text-muted">Quick checks draw on lesson knowledge and show answer keys after submission.</p></div>{showQuickResults ? <AttemptReview attempts={moduleAttempts} sources={program.sources} /> : <>{quickKind === 'test' && !testReady ? <div className="mb-4 rounded-xl border border-amber-500/25 bg-amber-500/5 p-4 text-sm leading-6 text-text-secondary">Prepare all {currentModule?.lessons.length ?? 0} lessons in this module to unlock its test. Browsing the syllabus never changes progress.</div> : null}<AssessmentPanel key={`${assessmentKey}:${chosenQuestions.map((q) => q.id).join(',')}`} kind={quickKind} questions={quickKind === 'test' && !testReady ? [] : chosenQuestions} sources={program.sources} attempt={form} pending={submitPending} error={submitError} onAnswer={(id, value) => setAnswer(assessmentKey, id, value)} onSubmit={(attemptId) => currentModule && onSubmitAttempt(assessmentKey, quickKind, currentModule, quickKind === 'test' ? null : currentLesson ?? null, chosenQuestions, form, attemptId)} onReview={() => setTab('lessons')} />{submitError && <span className="sr-only" role="status">Answers remain saved in this session.</span>}</>}</div>}
        {activeProgram && tab === 'assess-evidence' && <div className="mt-5"><AssessmentEvidencePanel requestedFormId={requestedFormId} programId={program.summary.id} program={program} module={currentModule} /></div>}
        {activeProgram && writtenOpened && <div hidden={tab !== 'workbench'} className="mt-5"><PracticeWorkbenchPanel key={currentLesson?.id} program={program} lesson={currentLesson} /></div>}
        {activeProgram && practicalOpened && <div hidden={tab !== 'practical'} className="mt-5"><PracticalWorkbenchPanel program={program} lesson={currentLesson ?? null} projectRequest={projectRequest} /></div>}
        </div>}
        {tab === 'sources' && <div className="mt-5">{program.summary.status === 'active' ? <><SourcesPanel programId={program.summary.id} /><SourceMaintenancePanel programId={program.summary.id} /></> : <div className="rounded-xl border border-dashed border-border bg-background p-6"><h3 className="font-serif text-xl text-text-primary">Accept this outline to open its source library</h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">The outline’s original material is shown in each citation. After acceptance, you can add program-scoped snapshots, read saved versions, search their text, and review changes here.</p></div>}</div>}
        {activeProgram && tab === 'notebook' && <div className="mt-5"><NotebookPanel enabled program={program} lessonId={currentLesson?.id} memory={memoryQuery.data} memoryLoading={memoryQuery.isLoading} memoryError={memoryQuery.error?.message} onRetryMemory={() => void memoryQuery.refetch()} drafts={notebookDrafts} setDraft={(key, draft) => setNotebookDrafts((current) => ({ ...current, [key]: draft }))} /></div>}
        {activeProgram && canvasOpened && tab === 'canvas' && <div className="mt-5"><Suspense fallback={<div role="status" className="rounded-xl border border-border bg-surface p-8 text-sm text-text-muted">Opening Canvas…</div>}><LazyCanvasPanel programId={program.summary.id} lessonId={currentLesson?.id} lessonTitle={currentLesson?.title ?? 'Learning canvas'} theme={theme} enabled /></Suspense></div>}
        {activeProgram && tab === 'recall' && <div className="mt-5"><RecallPanel program={program} lessonId={currentLesson?.id} memory={memoryQuery.data} memoryLoading={memoryQuery.isLoading} memoryError={memoryQuery.error?.message} onRetryMemory={() => void memoryQuery.refetch()} /><RecallEvolutionPanel programId={program.summary.id} active /></div>}
        {(prepareError || completeError) && tab === 'lessons' && <p role="alert" className="mt-4 rounded-xl bg-rose-500/10 p-4 text-sm text-rose-700">{prepareError ? `Lesson preparation failed: ${prepareError}` : `Completion could not be saved: ${completeError}`}</p>}
      </section></main>

      <details className="rounded-xl border border-border bg-surface px-4"><summary className="min-h-11 cursor-pointer py-3 text-sm font-semibold text-text-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">Progress, source materials & authorship</summary><div className="grid gap-3 pb-4 md:grid-cols-2 xl:grid-cols-3"><section className="rounded-xl border border-border/70 bg-background p-4"><div className="text-sm font-medium text-text-muted">Outcome evidence</div><h3 className="mt-1 font-serif text-lg text-text-primary">Where you are</h3><p className="mt-1 text-sm leading-5 text-text-muted">Different signals answer different questions.</p><div className="mt-3 space-y-3">{[{ label: 'Encountered', value: `${readyCount} of ${allLessons.length}`, detail: 'Lessons prepared' }, { label: 'Practiced', value: `${practiceCount}`, detail: 'Practice attempts' }, { label: 'Demonstrated', value: `${demonstratedCount}`, detail: 'Module test attempts' }, { label: 'Due for recheck', value: !activeProgram ? 'After acceptance' : memoryQuery.isError ? 'Unavailable' : memoryQuery.data ? `${memoryQuery.data.dueCount} due` : 'Loading', detail: !activeProgram ? 'Review scheduling starts with accepted work' : memoryQuery.isError ? 'Could not load the review schedule' : memoryQuery.data?.schedulerVersion ?? 'Review schedule' }].map((row) => <div key={row.label} className="border-t border-border pt-2 first:border-0 first:pt-0"><div className="flex items-baseline justify-between gap-2"><span className="text-sm font-medium text-text-secondary">{row.label}</span><span className="text-sm font-semibold tabular-nums text-text-primary">{row.value}</span></div><div className="mt-0.5 text-sm text-text-muted">{row.detail}</div></div>)}</div><p className="mt-3 rounded-lg bg-surface px-3 py-2 text-sm leading-5 text-text-secondary">A matched answer key records a quiz result; it does not establish mastery or job readiness.</p></section>
      <section className="rounded-xl border border-border/70 bg-background p-4"><h3 className="text-sm font-medium text-text-primary">Source materials</h3><p className="mt-1 text-sm leading-5 text-text-secondary">{program.sources.length ? `${program.sources.length} original ${program.sources.length === 1 ? 'source snapshot' : 'source snapshots'} grounding this program.` : 'No source snapshots are attached.'}</p>{program.summary.status === 'active' && <button type="button" disabled={savingBeforeMove} onClick={() => selectTab('sources')} className="mt-3 inline-flex min-h-11 items-center text-sm font-semibold text-accent hover:underline disabled:opacity-50">Open source library <ArrowRight size={15} className="ml-1" /></button>}{program.sources.length > 0 && <details className="mt-3"><summary className="min-h-11 cursor-pointer py-2 text-sm text-text-muted">Original outline excerpts</summary><SourceLine ids={program.sources.map((item) => item.id)} sources={program.sources} /></details>}</section>
      <section className="rounded-xl border border-border/70 bg-background p-4"><h3 className="text-sm font-medium text-text-primary">Course authorship</h3><p className="mt-2 text-sm leading-5 text-text-secondary">Generated with {program.modelName || 'the selected model'}{program.sources.length ? ' using your chosen sources' : ' from general knowledge'}. The outline is saved before review. Corrections are rechecked and unresolved findings remain available for repair. Lesson material receives its own review and source checks. Lesson answer keys are model-authored and shown after submission.</p></section></div></details>
    </div>
  </div>;
}
