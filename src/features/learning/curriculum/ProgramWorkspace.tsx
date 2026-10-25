import { Suspense, lazy, useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, ArrowRight, Layers3, PanelRightClose, PanelRightOpen } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { NavigationTabs } from '@/components/ui/NavigationTabs';
import { useLearningOutlineEvidence } from '@/features/learning/api/evidenceQueries';
import { learningQuizOverviewKey } from '@/features/learning/api/learningQueryKeys';
import { CourseCoach } from '@/features/learning/curriculum/CourseCoach';
import { CourseNavigator } from '@/features/learning/curriculum/CourseNavigator';
import { type CourseStep } from '@/features/learning/curriculum/courseProgress';
import { CourseSyllabus } from '@/features/learning/curriculum/CourseSyllabus';
import { OutlineCitation, OutlineEvidenceStatus } from '@/features/learning/curriculum/OutlineEvidence';
import { OutlineReviewPanel, type OutlineOperation } from '@/features/learning/curriculum/OutlineReviewPanel';
import { PlanPanel } from '@/features/learning/curriculum/PlanPanel';
import { StartingPointPanel } from '@/features/learning/curriculum/StartingPointPanel';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { StudyHabitsBar, type FrameworkAction } from '@/features/learning/habits';
import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { LessonPreparationLine } from '@/features/learning/lessons/LessonPreparationLine';
import { LessonPreparationStatus } from '@/features/learning/lessons/LessonPreparationProgress';
import { currentPreparationJobs, heldLessonIds } from '@/features/learning/lessons/review/claimReviewPresentation';
import { NotebookPanel, RecallPanel } from '@/features/learning/memory/MemoryPanels';
import { useLearningMemory, useSaveLearningCardDraft } from '@/features/learning/memory/useLearningMemory';
import { findOutlineCitation } from '@/features/learning/model/evidencePresentation';
import { PortabilityPanel } from '@/features/learning/portability/PortabilityPanel';
import { FeynmanTechniquePanel } from '@/features/learning/practice/FeynmanTechniquePanel';
import { PracticalWorkbenchPanel } from '@/features/learning/practice/practical/PracticalWorkbenchPanel';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { LessonQuizCallout } from '@/features/learning/quizzes/LessonQuizCallout';
import { ModuleTestCallout } from '@/features/learning/quizzes/ModuleTestCallout';
import { QuizzesOverview } from '@/features/learning/quizzes/QuizzesOverview';
import { useQuizOverview } from '@/features/learning/quizzes/useQuizzes';
import { ActiveRecallBrainDump } from '@/features/learning/recall/ActiveRecallBrainDump';
import { RecallEvolutionPanel } from '@/features/learning/recall/RecallEvolutionPanel';
import { CourseIndexNotice } from '@/features/learning/sources/CourseIndexNotice';
import { SourceLine } from '@/features/learning/sources/SourceLine';
import { SourceMaintenancePanel } from '@/features/learning/sources/SourceMaintenancePanel';
import { SourcesPanel } from '@/features/learning/sources/SourcesPanel';
import { useCourseIndexOpen, useEvidenceWaitsForIndex } from '@/features/learning/sources/useCourseIndexStatus';
import { ProgramTangents, ProgramTangentsTrigger } from '@/features/learning/tangents/ProgramTangents';
import { useRepairLearningOutline } from '@/features/learning/workspace/useLearningStudio';
import { CitationVisibilityToggle } from '@/features/reading/components/CitationVisibilityToggle';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useEffectiveTheme } from '@/hooks/useApplyTheme';
import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';
import type { StudyDestination, StudyTab } from '@/lib/studyActivity';


const LazyCanvasPanel = lazy(() => import('@/features/learning/canvas/CanvasPanel'));

type Tab = StudyTab;
type NotebookDraft = { title: string; content: string };

export function ProgramWorkspace({ program, destination, outlineGeneration, initialTangentId, onBack, onOpenProgram, onAccept, acceptPending, acceptError, onPrepare, preparePending, prepareError, onComplete, completePending, completeError, onReview }: {
  destination?: StudyDestination;
  outlineGeneration?: OutlineOperation;
  initialTangentId?: string | null;
  program: LearningProgramDto; onBack: () => void; onAccept: (title: string) => void; acceptPending: boolean; acceptError?: string;
  /** Opens another course, e.g. the copy an import just created. */
  onOpenProgram: (programId: string) => void;
  onPrepare: (lesson: LearningLessonDto) => void; preparePending: boolean; prepareError?: string;
  onComplete: (lesson: LearningLessonDto) => void; completePending: boolean; completeError?: string;
  /** Opens a lesson's claim review screen (UI design §2.3). */
  onReview?: (programId: string, lessonId: string) => void;
}) {
  const [moduleId, setModuleId] = useState(() => program.modules.find((m) => m.lessons.some((l) => l.id === program.summary.currentLessonId))?.id ?? program.modules[0]?.id ?? '');
  const [lessonId, setLessonId] = useState(program.summary.currentLessonId ?? program.modules[0]?.lessons[0]?.id ?? '');
  const [tab, setTab] = useState<Tab>('lessons');
  const [lastPracticeTab, setLastPracticeTab] = useState<Tab>('workbench');
  const [preparationExpanded, setPreparationExpanded] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [contextVisible, setContextVisible] = useState(true);
  const moreButtonRef = useRef<HTMLButtonElement>(null);
  const moreMenuRef = useRef<HTMLDivElement>(null);
  const [canvasOpened, setCanvasOpened] = useState(false);
  const [practicalOpened, setPracticalOpened] = useState(false);
  // The quiz open in the Quizzes tab (runner or result), and a module test row to bring into view.
  const [quizFormId, setQuizFormId] = useState<string | null>(null);
  const [quizModuleFocus, setQuizModuleFocus] = useState<string | null>(null);
  const [projectRequest, setProjectRequest] = useState<{ id: string; brief: string } | null>(null);
  const [writtenOpened, setWrittenOpened] = useState(false);
  const [recallView, setRecallView] = useState<'lesson' | 'evolution' | 'brain-dump'>('lesson');
  const [workbenchMode, setWorkbenchMode] = useState<'practice' | 'feynman'>('practice');
  const saveRecallDraft = useSaveLearningCardDraft();
  const handleImported = (importedProgramId: string) => {
    // A copy or another course: land on it, which resumes at its current lesson.
    if (importedProgramId !== program.summary.id) { onOpenProgram(importedProgramId); return; }
    // Import flushes pending saves first. Retained workbenches must now discard
    // local session selections and saved lab snapshots, even when IDs/revisions
    // match the replaced course. The import panel itself stays open.
    setWrittenOpened(false);
    setPracticalOpened(false);
    setCanvasOpened(false);
    setQuizFormId(null);
    setProjectRequest(null);
    setNotebookDrafts({});
  };
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
    if (destination.formId) setQuizFormId(destination.formId);
    if (next === 'quizzes' && !destination.formId) setQuizModuleFocus(destination.lessonId ? null : destination.moduleId ?? null);
    const resolvedTab = next === 'study' ? 'lessons' : next;
    setTab(resolvedTab);
    setMoreOpen(false);
    if (resolvedTab === 'canvas') setCanvasOpened(true);
    if (resolvedTab === 'practical') setPracticalOpened(true);
    if (resolvedTab === 'workbench') setWrittenOpened(true);
    if (resolvedTab === 'workbench' || resolvedTab === 'practical') setLastPracticeTab(resolvedTab);
  }, [destination, program.modules]);
  const [title, setTitle] = useState(program.summary.title);
  const [notebookDrafts, setNotebookDrafts] = useState<Record<string, NotebookDraft>>({});
  const [savingBeforeMove, setSavingBeforeMove] = useState(false);
  const repair = useRepairLearningOutline(program.summary.id);
  const outlineBlocked = repair.isPending || outlineGeneration?.isPending || Boolean(program.outlineReview && program.outlineReview.status !== 'passed');
  const activeProgram = program.summary.status === 'active';
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const outlineEvidence = useLearningOutlineEvidence(
    program.summary.id,
    program.summary.revision,
    showCitations && program.sources.length > 0,
  );
  const visibleOutlineEvidence = showCitations && !outlineEvidence.isError ? outlineEvidence.data : null;
  const preparationPlan = useLearningPlan(activeProgram ? program.summary.id : '', preparePending);
  const preparationJobs = preparationPlan.data?.jobs?.filter((job) => job.kind === 'lesson_preparation').sort((a, b) => b.createdAt - a.createdAt) ?? [];
  const preparationJob = preparationJobs.find((job) => job.status === 'running') ?? preparationJobs.find((job) => job.status === 'pending') ?? preparationJobs[0];
  const preparingLesson = preparePending || Boolean(preparationPlan.data?.jobs?.some((job) => ['pending', 'running'].includes(job.status)));
  // A course step that would prepare a lesson only opens it while the course's references index.
  // While the workspace is open, the course's index embeds first.
  useCourseIndexOpen(program.summary.id);
  const waitsForIndex = useEvidenceWaitsForIndex(program.summary.id);
  const memoryQuery = useLearningMemory(program.summary.id, activeProgram);
  const theme = useEffectiveTheme();
  const currentModule = program.modules.find((m) => m.id === moduleId) ?? program.modules[0];
  const currentLesson = lessonId ? (currentModule?.lessons.find((l) => l.id === lessonId) ?? currentModule?.lessons[0]) : undefined;
  const allLessons = program.modules.flatMap((m) => m.lessons.map((lesson) => ({ module: m, lesson })));
  const completedCount = allLessons.filter(({ lesson }) => lesson.completed).length;
  const readyCount = allLessons.filter(({ lesson }) => lesson.preparation === 'ready').length;
  const quizzes = useQuizOverview(program.summary.id, activeProgram);
  const testsPassed = quizzes.data?.modules.filter((module) => module.test.verdict === 'passed').length ?? 0;
  const queryClient = useQueryClient();
  // A prepared or replaced lesson changes what its quiz can ask.
  useEffect(() => { void queryClient.invalidateQueries({ queryKey: learningQuizOverviewKey(program.summary.id) }); }, [queryClient, program.summary.id, program.summary.revision]);
  const resume = allLessons.find(({ lesson }) => lesson.id === program.summary.currentLessonId) ?? allLessons.find(({ lesson }) => !lesson.completed);
  const primary = [{ id: 'lessons' as const, label: 'Lessons' }, { id: 'workbench' as const, label: 'Practice' }, { id: 'quizzes' as const, label: 'Quizzes' }, { id: 'recall' as const, label: 'Recall' }, { id: 'notebook' as const, label: 'Notebook' }, { id: 'sources' as const, label: 'Sources' }];
  const activateTab = (next: Tab) => {
    if (next === 'canvas') setCanvasOpened(true);
    if (next === 'practical') setPracticalOpened(true);
    if (next === 'workbench') setWrittenOpened(true);
    if (next === 'workbench' || next === 'practical') setLastPracticeTab(next);
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
  const handleSelectFramework = (action: FrameworkAction) => {
    switch (action) {
      case 'recall-dump':
        setRecallView('brain-dump');
        selectTab('recall');
        break;
      case 'spaced-repetition':
        setRecallView('evolution');
        selectTab('recall');
        break;
      case 'feynman':
        setWorkbenchMode('feynman');
        setWrittenOpened(true);
        selectTab('workbench');
        break;
      case 'interleaving':
        setRecallView('lesson');
        selectTab('recall');
        break;
      case 'dual-coding':
        selectMoreTab('canvas');
        break;
      case 'handwritten-notes':
        selectTab('notebook');
        break;
    }
  };
  const practiceTab = tab === 'workbench' || tab === 'practical';
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
      else if (step.kind === 'assessment') { setQuizFormId(step.formId ?? null); activateTab('quizzes'); }
      else if (step.kind === 'quiz' && target) { activateTab('lessons'); requestAnimationFrame(() => focusQuizCallout(target.lesson.id)); }
      else if (step.kind === 'assignment') activateTab('workbench');
      else if (step.kind === 'recall') activateTab('recall');
      else {
        activateTab('lessons');
        if (target?.lesson.preparation === 'outline' && !preparingLesson && !waitsForIndex) onPrepare(target.lesson);
        if (step.kind === 'guided' && target) requestAnimationFrame(() => document.getElementById(`guided-${target.lesson.id}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' }));
      }
    });
  };

  const focusQuizCallout = (lessonId: string) => {
    const callout = document.getElementById(`quiz-callout-${lessonId}`);
    callout?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    callout?.focus({ preventScroll: true });
  };
  const openQuiz = (formId: string) => void afterNotebookSave(() => { setQuizFormId(formId); activateTab('quizzes'); });
  const showModuleTest = (moduleId: string) => void afterNotebookSave(() => { setQuizFormId(null); setQuizModuleFocus(moduleId); activateTab('quizzes'); });
  const afterNotebookSave = async (move: () => void) => {
    if (savingBeforeMove) return;
    setSavingBeforeMove(true);
    const saved = await flushPendingSaves();
    setSavingBeforeMove(false);
    if (!saved) return;
    move();
  };

  const effectiveTangentId = initialTangentId ?? destination?.tangentId ?? null;
  // The goal once, behind a disclosure, and never when it is the title (§2.2).
  const goal = program.summary.goal.trim();
  const shownTitle = (program.summary.status === 'draft' ? title : program.summary.title).trim();
  const showGoal = Boolean(goal) && goal !== shownTitle && !shownTitle.startsWith(goal.slice(0, 80));
  const lessonTitles = Object.fromEntries(allLessons.map(({ lesson }) => [lesson.id, lesson.title]));
  const openReview = onReview ? (programId: string, reviewLessonId: string) => void afterNotebookSave(() => onReview(programId, reviewLessonId)) : undefined;
  // After a new source re-checked the waiting claims: the one held lesson's review, else the Lessons tab with its Review buttons.
  const openHeldReview = () => {
    const held = [...new Set(currentPreparationJobs(preparationPlan.data?.jobs ?? []).flatMap(heldLessonIds))];
    if (held.length === 1 && openReview) openReview(program.summary.id, held[0]); else selectTab('lessons');
  };

  return (
    <ProgramTangents
      program={program}
      currentModuleId={currentModule?.id ?? null}
      currentLessonId={currentLesson?.id ?? null}
      initialTangentId={effectiveTangentId}
      onSelectLesson={(mId, lId) => {
        void afterNotebookSave(() => {
          setModuleId(mId);
          setLessonId(lId);
          activateTab('lessons');
        });
      }}
    >
      <div className="studio-workspace pb-12">
        <div className="studio-course-toolbar">
          <Button variant="ghost" type="button" disabled={savingBeforeMove} onClick={() => void afterNotebookSave(onBack)} className="studio-course-back disabled:opacity-50"><ArrowLeft size={15} /> {savingBeforeMove ? 'Saving your work…' : 'All programs'}</Button>
          <div className="studio-course-tools"><CitationVisibilityToggle /><ProgramTangentsTrigger />{activeProgram && <CourseSyllabus compact program={program} moduleId={moduleId} evidence={visibleOutlineEvidence} onSelect={(module, selectedLessonId) => void afterNotebookSave(() => { setModuleId(module.id); setLessonId(selectedLessonId); setTab('lessons'); requestAnimationFrame(() => document.getElementById(`lesson-${selectedLessonId}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })); })} />}
            <Button variant="ghost" type="button" aria-pressed={contextVisible} disabled={savingBeforeMove} onClick={() => void afterNotebookSave(() => setContextVisible((visible) => !visible))} className="studio-context-toggle disabled:opacity-50" title={contextVisible ? 'Hide lesson outline and notes' : 'Show lesson outline and notes'}>{contextVisible ? <PanelRightClose size={15} /> : <PanelRightOpen size={15} />}<span>Outline & notes</span></Button>
          </div>
        </div>
        {showCitations && <OutlineEvidenceStatus report={outlineEvidence} />}
        <header className="studio-course-header">
          <div className="studio-course-header-main">
            <div className="studio-course-title-group">
              <div className="studio-eyebrow"><Layers3 size={13} /> {program.summary.status === 'draft' ? 'Draft outline' : 'Active program'}</div>
              {program.summary.status === 'draft' ? <><h1 className="sr-only">{title || 'Untitled program'}</h1><label className="sr-only" htmlFor="program-title">Program title</label><input id="program-title" aria-label="Program title" value={title} onChange={(e) => setTitle(e.target.value)} className="studio-course-title-input" /></> : <h1>{program.summary.title}</h1>}
              {showGoal && <details className="studio-course-goal"><summary>Goal</summary><p className="mt-2 whitespace-pre-line">{goal}</p></details>}
            </div>
            <div className="studio-course-progress">
              <div className="studio-course-progress-row"><span>Lessons complete</span><strong>{completedCount} <span className="font-normal text-text-muted">/ {allLessons.length}</span></strong></div>
              <div className="studio-progress-track" aria-hidden="true"><span style={{ width: `${allLessons.length ? completedCount / allLessons.length * 100 : 0}%` }} /></div>
              {quizzes.data && quizzes.data.quizzesTotal > 0 && <div className="studio-course-progress-row mt-2"><span>Quizzes passed</span><strong>{quizzes.data.quizzesPassed} <span className="font-normal text-text-muted">/ {quizzes.data.quizzesTotal}</span></strong></div>}
            </div>
          </div>
          {program.summary.status === 'draft' && <div className="studio-course-draft-actions"><p>{outlineBlocked ? 'Resolve the outline findings before accepting this course.' : 'Accept to prepare lessons individually and track progress.'}</p><div className="flex items-center gap-3">{acceptError && <span role="alert" className="max-w-xs text-sm text-danger-fg">{acceptError}</span>}<Button variant="outline" type="button" disabled={acceptPending || outlineBlocked || !title.trim()} onClick={() => onAccept(title.trim())} className="studio-button studio-button-primary disabled:opacity-50">{acceptPending ? 'Saving…' : 'Accept program'}</Button></div></div>}
        </header>
    <OutlineReviewPanel program={program} generation={outlineGeneration} repair={repair} />

    {!program.sources.length && <p className="mt-3 text-xs leading-5 text-text-muted">Topic-based course · AI-authored from general knowledge. Add references in Sources to ground future lessons.</p>}
    {!activeProgram && <CourseSyllabus program={program} moduleId={moduleId} evidence={visibleOutlineEvidence} onSelect={(module, selectedLessonId) => void afterNotebookSave(() => { setModuleId(module.id); setLessonId(selectedLessonId); setTab('lessons'); requestAnimationFrame(() => document.getElementById(`lesson-${selectedLessonId}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })); })} />}
    {activeProgram && <StudyHabitsBar className="studio-habits" onSelectFramework={handleSelectFramework} />}
    <div className="grid min-w-0 grid-cols-1 gap-2">
      <CourseNavigator modules={program.modules} moduleId={currentModule?.id ?? ''} lessonId={currentLesson?.id ?? ''} disabled={savingBeforeMove} onModuleChange={(next) => void afterNotebookSave(() => { setModuleId(next.id); setLessonId(next.lessons.find((lesson) => !lesson.completed)?.id ?? next.lessons[0]?.id ?? ''); setTab('lessons'); })} onLessonChange={(id) => void afterNotebookSave(() => setLessonId(id))}>
        {activeProgram ? <CourseCoach program={program} dueCount={memoryQuery.data?.dueCount ?? 0} disabled={savingBeforeMove || preparePending} onStep={goToStep} /> : resume && <Button type="button" aria-label={`Continue lesson: ${resume.lesson.title}`} title={resume.lesson.title} disabled={savingBeforeMove} onClick={() => void afterNotebookSave(() => { setModuleId(resume.module.id); setLessonId(resume.lesson.id); setTab('lessons'); })} className="studio-button studio-button-primary">Continue lesson <ArrowRight size={15} /></Button>}
      </CourseNavigator>
      <main className="min-w-0"><section id="workspace-panel" tabIndex={-1} aria-label="Program workspace content" className="studio-workspace-panel">
        <div className="studio-module-header">
          <div className="min-w-0 flex-1">
            <h2 className="truncate">{currentModule?.title}</h2>
            <p className="line-clamp-1">{currentModule?.summary}</p>
            {showCitations && currentModule && <OutlineCitation citation={findOutlineCitation(visibleOutlineEvidence, 'module_summary', currentModule.id, currentModule.summary)} evidence={visibleOutlineEvidence} />}
          </div>
          <details className="group relative shrink-0 text-sm">
            <summary className="min-h-10 cursor-pointer list-none rounded-full border border-border px-3 py-2 text-text-secondary hover:border-accent/40 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">Module details <span className="text-accent">⌄</span></summary>
            <div className="absolute right-0 z-20 mt-2 w-[min(24rem,calc(100vw-2rem))] rounded-xl border border-border bg-surface p-4 shadow-xl">
              <h3 className="font-medium text-text-primary">Outcomes</h3>
              <ul className="mt-2 space-y-3 text-sm leading-5 text-text-secondary">{currentModule?.outcomes.map((outcome, index) => <li key={outcome}><div className="flex gap-2"><span aria-hidden="true">•</span><span>{outcome}</span></div>{showCitations && <OutlineCitation citation={findOutlineCitation(visibleOutlineEvidence, 'outcome', currentModule.id, outcome, index)} evidence={visibleOutlineEvidence} className="ml-4" />}</li>)}</ul>
            </div>
          </details>
        </div>
        <nav aria-label="Module workspace" className="studio-workspace-tabs"><NavigationTabs label="Program workspace" value={practiceTab ? 'workbench' : tab} disabled={savingBeforeMove} options={primary.filter((item) => activeProgram || item.id === 'lessons' || item.id === 'sources').map((item) => ({ ...item, badge: item.id === 'recall' && memoryQuery.data?.dueCount ? memoryQuery.data.dueCount : undefined }))} onValueChange={(id) => id === 'workbench' ? selectPracticeGroup() : selectTab(id)} idPrefix="workspace-tab" panelId="workspace-panel" actions={<div className="relative ml-auto"><button ref={moreButtonRef} type="button" aria-haspopup="menu" aria-expanded={moreOpen} aria-current={['plan', 'canvas', 'portability'].includes(tab) ? 'page' : undefined} aria-controls="workspace-more-menu" onClick={() => setMoreOpen((value) => !value)} className="min-h-11 rounded-lg px-3 text-sm font-medium text-text-secondary hover:bg-background focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">More <span aria-hidden="true">⌄</span></button>{moreOpen && <div id="workspace-more-menu" ref={moreMenuRef} role="menu" aria-label="More workspace tools" onKeyDown={(event) => { if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return; event.preventDefault(); const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')); const index = items.indexOf(document.activeElement as HTMLButtonElement); const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length; items[next]?.focus(); }} className="absolute right-0 top-full z-30 mt-1 w-52 rounded-xl border border-border bg-surface p-1.5 shadow-xl">{(activeProgram ? [{ id: 'plan' as const, label: 'Plan' }, { id: 'canvas' as const, label: 'Canvas' }, { id: 'portability' as const, label: 'Import & export' }] : [{ id: 'portability' as const, label: 'Import & export' }]).map((item) => <button key={item.id} type="button" role="menuitem" aria-current={tab === item.id ? 'page' : undefined} onClick={() => selectMoreTab(item.id)} className="flex min-h-11 w-full items-center rounded-lg px-3 text-left text-sm text-text-primary hover:bg-background focus-visible:bg-background focus-visible:outline-hidden">{item.label}</button>)}</div>}</div>} /></nav>
        {practiceTab && <><div className="mt-3 flex flex-wrap items-center justify-between gap-2"><div className="text-sm text-text-secondary"><span className="font-medium text-text-primary">{currentModule?.title}</span>{currentLesson && <> <span aria-hidden="true">›</span> {currentLesson.title}</>}</div><NavigationTabs label="Practice activities" appearance="segmented" value={tab} disabled={savingBeforeMove} options={[{ id: 'workbench', label: 'Written practice' }, { id: 'practical', label: 'Code & simulations' }]} onValueChange={selectTab} idPrefix="practice-tab" panelId="practice-content" /></div></>}

        {activeProgram && tab === 'starting_point' && <div className="mt-4"><StartingPointPanel programId={program.summary.id} onStudy={(lessonId) => goToStep({ kind: 'lesson', title: '', reason: '', lessonId })} onChallenge={(lessonId) => goToStep({ kind: 'assessment', title: '', reason: '', lessonId })} /></div>}
        <CourseIndexNotice programId={program.summary.id} />
        {/* The full panel belongs to Lessons; elsewhere one line keeps the tab's own content in view. */}
        {activeProgram && (tab === 'lessons'
          ? <LessonPreparationStatus job={preparationJob} pending={preparePending} programId={program.summary.id} revision={program.summary.revision} error={prepareError} expanded={preparationExpanded} onExpandedChange={setPreparationExpanded} lessonTitles={lessonTitles} onReview={openReview} />
          : <LessonPreparationLine job={preparationJob} pending={preparePending} programId={program.summary.id} error={prepareError} onView={() => selectTab('lessons')} />)}
        {activeProgram && preparationPlan.isError && preparePending && <p role="alert" className="my-3 text-sm text-rose-700">Progress could not be loaded. <button type="button" className="underline" onClick={() => void preparationPlan.refetch()}>Retry progress</button></p>}
        {tab === 'lessons' && <div className="studio-lessons">{!currentModule?.lessons.length ? <p className="rounded-lg bg-surface p-6 text-sm text-text-muted">This module has no lessons in its outline.</p> : currentModule.lessons.map((lesson, index) => <LessonCard key={lesson.id} lesson={lesson} number={`${program.modules.indexOf(currentModule) + 1}.${index + 1}`} moduleId={currentModule.id} outlineEvidence={visibleOutlineEvidence} program={program} selected={lesson.id === lessonId} isCurrent={lesson.id === program.summary.currentLessonId || (lesson.id === resume?.lesson.id && !program.summary.currentLessonId)} pending={completePending || (lesson.preparation === 'outline' && preparingLesson)} onPrepare={() => onPrepare(lesson)} onComplete={() => onComplete(lesson)} onSelect={() => void afterNotebookSave(() => setLessonId(lesson.id))} onPractice={() => void afterNotebookSave(() => { setLessonId(lesson.id); setTab('workbench'); setWrittenOpened(true); setLastPracticeTab('workbench'); })} onReview={openReview} contextVisible={contextVisible} onSources={() => selectTab('sources')}
          notebook={activeProgram && lesson.id === lessonId ? <NotebookPanel compact enabled program={program} lessonId={lesson.id} memory={memoryQuery.data} memoryLoading={memoryQuery.isLoading} memoryError={memoryQuery.error?.message} onRetryMemory={() => void memoryQuery.refetch()} drafts={notebookDrafts} setDraft={(key, draft) => setNotebookDrafts((current) => ({ ...current, [key]: draft }))} onOpenNotebook={() => selectTab('notebook')} /> : undefined}
          quizCallout={activeProgram && lesson.preparation === 'ready' ? <LessonQuizCallout program={program} lesson={lesson} onOpenForm={openQuiz} onViewResult={openQuiz} /> : null} />)}</div>}
        {activeProgram && tab === 'lessons' && <nav aria-label="Module milestones" className="studio-milestones flex flex-wrap items-center gap-1 text-sm"><span className="px-3 text-text-secondary"><span className="mr-1.5 text-xs font-semibold tabular-nums text-accent">01</span>Learn · {currentModule?.lessons.filter((item) => item.completed).length ?? 0} of {currentModule?.lessons.length ?? 0} complete</span><button type="button" onClick={() => selectTab('workbench')} className="min-h-10 rounded-full px-3 text-left text-text-secondary hover:bg-background hover:text-text-primary disabled:opacity-50"><span className="mr-1.5 text-xs font-semibold tabular-nums text-accent">02</span>Apply →</button>{currentModule && <ModuleTestCallout program={program} module={currentModule} onOpenForm={openQuiz} onShowTest={() => showModuleTest(currentModule.id)} />}<button type="button" disabled={!currentModule?.lessons.some((item) => item.preparation === 'ready')} onClick={() => void afterNotebookSave(() => {
          if (!currentModule) return;
          const anchor = [...currentModule.lessons].reverse().find((item) => item.preparation === 'ready');
          if (!anchor) return;
          setLessonId(anchor.id);
          const finalModule = currentModule.id === program.modules.at(-1)?.id;
          const scope = finalModule ? program.modules : [currentModule];
          setProjectRequest({ id: crypto.randomUUID(), brief: `Create a ${finalModule ? 'final capstone integrating the complete course' : 'module project'} for ${program.summary.goal}. Project progression: ${scope.map((item) => item.project ? `${item.project.title}: ${item.project.brief} Deliverables: ${item.project.deliverables.join('; ')}. Success criteria: ${item.project.successCriteria.join('; ')}` : item.summary).join(' | ')}. Each milestone must extend the same project artifact and include a revision after feedback. Outcomes: ${scope.flatMap((item) => item.outcomes).join('; ')}. Require a concrete artifact, milestones, constraints, an explicit grading rubric, and a reflection on tradeoffs. Build on these lessons: ${scope.flatMap((item) => item.lessons.map((lesson) => lesson.title)).join('; ')}.`.slice(0, 4000) });
          activateTab('practical');
        })} title="A project brief shaped by these outcomes, with deliverables and a rubric." className="min-h-10 rounded-full px-3 text-left text-text-secondary hover:bg-background hover:text-text-primary disabled:opacity-50"><span className="mr-1.5 text-xs font-semibold tabular-nums text-accent">04</span>{currentModule?.id === program.modules.at(-1)?.id ? 'Build your capstone →' : 'Build a module project →'}</button></nav>}
        {activeProgram && tab === 'plan' && <div className="mt-5"><PlanPanel programId={program.summary.id} onStartingPoint={() => selectTab('starting_point')} /></div>}
        {tab === 'portability' && <div className="mt-5"><PortabilityPanel programId={program.summary.id} onImported={handleImported} /></div>}
        {activeProgram && (practiceTab || writtenOpened || practicalOpened) && <div id="practice-content" role="tabpanel" tabIndex={0} aria-labelledby={`practice-tab-${practiceTab ? tab : lastPracticeTab}`} hidden={!practiceTab} className="mt-3">
        {activeProgram && writtenOpened && (
          <div hidden={tab !== 'workbench'} className="mt-5 space-y-4">
            <div role="group" aria-label="Written practice mode" className="flex flex-wrap gap-1 rounded-lg border border-border bg-background p-1 text-xs">
              <button
                type="button"
                aria-pressed={workbenchMode === 'practice'}
                onClick={() => setWorkbenchMode('practice')}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  workbenchMode === 'practice' ? 'bg-surface text-text-primary shadow-xs font-medium' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                Grounded practice
              </button>
              <button
                type="button"
                aria-pressed={workbenchMode === 'feynman'}
                onClick={() => setWorkbenchMode('feynman')}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  workbenchMode === 'feynman' ? 'bg-surface text-text-primary shadow-xs font-medium' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                Feynman technique (Teach a beginner)
              </button>
            </div>

            <div hidden={workbenchMode !== 'practice'}>
              <PracticeWorkbenchPanel key={currentLesson?.id} program={program} lesson={currentLesson} />
            </div>

            {workbenchMode === 'feynman' && (
              <FeynmanTechniquePanel program={program} lesson={currentLesson} module={currentModule} />
            )}
          </div>
        )}
        {activeProgram && practicalOpened && <div hidden={tab !== 'practical'} className="mt-5"><PracticalWorkbenchPanel program={program} lesson={currentLesson ?? null} projectRequest={projectRequest} /></div>}
        </div>}
        {activeProgram && tab === 'quizzes' && <div className="mt-4" role="tabpanel" aria-labelledby="workspace-tab-quizzes"><QuizzesOverview program={program} formId={quizFormId} onFormIdChange={setQuizFormId} focusModuleId={quizModuleFocus}
          onOpenLesson={(lessonId) => { const target = allLessons.find(({ lesson }) => lesson.id === lessonId); if (target) void afterNotebookSave(() => { setModuleId(target.module.id); setLessonId(lessonId); activateTab('lessons'); }); }}
          completionFor={(lessonId) => { const target = allLessons.find(({ lesson }) => lesson.id === lessonId)?.lesson; return target ? { completed: target.completed, pending: completePending, onComplete: () => onComplete(target) } : undefined; }} /></div>}
        {tab === 'sources' && <div className="mt-5">{program.summary.status === 'active' ? <><SourcesPanel programId={program.summary.id} onOpenReview={openHeldReview} /><SourceMaintenancePanel programId={program.summary.id} /></> : <div className="rounded-xl border border-dashed border-border bg-background p-6"><h3 className="font-serif text-xl text-text-primary">Accept this outline to open its source library</h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">The outline’s original material is shown in each citation. After acceptance, you can add program-scoped snapshots, read saved versions, search their text, and review changes here.</p></div>}</div>}
        {activeProgram && tab === 'notebook' && <div className="mt-5"><NotebookPanel enabled program={program} lessonId={currentLesson?.id} memory={memoryQuery.data} memoryLoading={memoryQuery.isLoading} memoryError={memoryQuery.error?.message} onRetryMemory={() => void memoryQuery.refetch()} drafts={notebookDrafts} setDraft={(key, draft) => setNotebookDrafts((current) => ({ ...current, [key]: draft }))} /></div>}
        {activeProgram && canvasOpened && tab === 'canvas' && <div className="mt-5"><Suspense fallback={<div role="status" className="rounded-xl border border-border bg-surface p-8 text-sm text-text-muted">Opening Canvas…</div>}><LazyCanvasPanel programId={program.summary.id} lessonId={currentLesson?.id} lessonTitle={currentLesson?.title ?? 'Learning canvas'} theme={theme} enabled lesson={currentLesson} /></Suspense></div>}
        {activeProgram && tab === 'recall' && (
          <div className="mt-5 space-y-4">
            <div role="group" aria-label="Recall view" className="flex flex-wrap gap-1 rounded-lg border border-border bg-background p-1 text-xs">
              <button
                type="button"
                aria-pressed={recallView === 'lesson'}
                onClick={() => setRecallView('lesson')}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  recallView === 'lesson' ? 'bg-surface text-text-primary shadow-xs font-medium' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                In-Studio review
              </button>
              <button
                type="button"
                aria-pressed={recallView === 'brain-dump'}
                onClick={() => setRecallView('brain-dump')}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  recallView === 'brain-dump' ? 'bg-surface text-text-primary shadow-xs font-medium' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                Free recall (Brain dump)
              </button>
              <button
                type="button"
                aria-pressed={recallView === 'evolution'}
                onClick={() => setRecallView('evolution')}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  recallView === 'evolution' ? 'bg-surface text-text-primary shadow-xs font-medium' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                Versioned cards & schedules
              </button>
            </div>

            {recallView === 'lesson' && (
              <RecallPanel
                program={program}
                lessonId={currentLesson?.id}
                memory={memoryQuery.data}
                memoryLoading={memoryQuery.isLoading}
                memoryError={memoryQuery.error?.message}
                onRetryMemory={() => void memoryQuery.refetch()}
              />
            )}

            {recallView === 'brain-dump' && (
              <ActiveRecallBrainDump
                program={program}
                lesson={currentLesson}
                onDraftMissedConcept={(question, answer) => {
                  if (!currentLesson?.id) return;
                  saveRecallDraft.mutate({
                    draftId: null,
                    programId: program.summary.id,
                    lessonId: currentLesson.id,
                    question,
                    answer,
                    explanation: 'Drafted from free recall gap analysis',
                    sourceIds: [],
                  });
                }}
              />
            )}

            {recallView === 'evolution' && (
              <RecallEvolutionPanel programId={program.summary.id} active />
            )}
          </div>
        )}
        {(prepareError || completeError) && tab === 'lessons' && <p role="alert" className="mt-4 rounded-xl bg-rose-500/10 p-4 text-sm text-rose-700">{prepareError ? `Lesson preparation failed: ${prepareError}` : `Completion could not be saved: ${completeError}`}</p>}
      </section></main>

      <details className="studio-course-provenance border border-border px-4"><summary className="min-h-11 cursor-pointer py-3 text-sm font-semibold text-text-secondary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">Progress, source materials & authorship</summary><div className="grid gap-3 pb-4 md:grid-cols-2 xl:grid-cols-3"><section className="rounded-xl border border-border bg-background p-4"><div className="text-sm font-medium text-text-muted">Outcome evidence</div><h3 className="mt-1 font-serif text-lg text-text-primary">Where you are</h3><p className="mt-1 text-sm leading-5 text-text-muted">Different signals answer different questions.</p><div className="mt-3 space-y-3">{[{ label: 'Encountered', value: `${readyCount} of ${allLessons.length}`, detail: 'Lessons prepared' }, { label: 'Checked', value: quizzes.data ? `${quizzes.data.quizzesPassed} of ${quizzes.data.quizzesTotal}` : '—', detail: 'Lesson quizzes passed' }, { label: 'Demonstrated', value: quizzes.data ? `${testsPassed} of ${quizzes.data.modules.length}` : '—', detail: 'Module tests passed' }, { label: 'Due for recheck', value: !activeProgram ? 'After acceptance' : memoryQuery.isError ? 'Unavailable' : memoryQuery.data ? `${memoryQuery.data.dueCount} due` : 'Loading', detail: !activeProgram ? 'Review scheduling starts with accepted work' : memoryQuery.isError ? 'Could not load the review schedule' : 'FSRS 6 review schedule' }].map((row) => <div key={row.label} className="border-t border-border pt-2 first:border-0 first:pt-0"><div className="flex items-baseline justify-between gap-2"><span className="text-sm font-medium text-text-secondary">{row.label}</span><span className="text-sm font-semibold tabular-nums text-text-primary">{row.value}</span></div><div className="mt-0.5 text-sm text-text-muted">{row.detail}</div></div>)}</div><p className="mt-3 rounded-lg bg-surface px-3 py-2 text-sm leading-5 text-text-secondary">A quiz result is evidence from one attempt; it does not establish mastery or job readiness.</p></section>
      <section className="rounded-xl border border-border bg-background p-4"><h3 className="text-sm font-medium text-text-primary">Source materials</h3><p className="mt-1 text-sm leading-5 text-text-secondary">{program.sources.length ? `${program.sources.length} original ${program.sources.length === 1 ? 'source snapshot' : 'source snapshots'} grounding this program.` : 'No source snapshots are attached.'}</p>{program.summary.status === 'active' && <button type="button" disabled={savingBeforeMove} onClick={() => selectTab('sources')} className="mt-3 inline-flex min-h-11 items-center text-sm font-semibold text-accent hover:underline disabled:opacity-50">Open source library <ArrowRight size={15} className="ml-1" /></button>}{program.sources.length > 0 && <details className="mt-3"><summary className="min-h-11 cursor-pointer py-2 text-sm text-text-muted">Original outline excerpts</summary><SourceLine ids={program.sources.map((item) => item.id)} sources={program.sources} /></details>}</section>
      <section className="rounded-xl border border-border bg-background p-4"><h3 className="text-sm font-medium text-text-primary">Course authorship</h3><p className="mt-2 text-sm leading-5 text-text-secondary">Generated with {program.modelName || 'the selected model'}{program.sources.length ? ' using your chosen sources' : ' from general knowledge'}. The outline is saved before review. Corrections are rechecked and unresolved findings remain available for repair. Lesson material receives its own review and source checks. Lesson answer keys are model-authored and shown after submission.</p></section></div></details>
    </div>
  </div>
  </ProgramTangents>
  );
}
