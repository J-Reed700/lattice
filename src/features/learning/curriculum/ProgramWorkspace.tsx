import { Suspense, lazy, useEffect, useRef, useState } from 'react';

import { useLearningOutlineEvidence } from '@/features/learning/api/evidenceQueries';
import { AssessmentEvidencePanel } from '@/features/learning/assessment/AssessmentEvidencePanel';
import { type FormSnapshot } from '@/features/learning/assessment/AssessmentPanel';
import { moduleTestCoverage } from '@/features/learning/assessment/moduleTestCoverage';
import { type CourseStep } from '@/features/learning/curriculum/courseProgress';
import { CourseSyllabus } from '@/features/learning/curriculum/CourseSyllabus';
import { OutlineCitation } from '@/features/learning/curriculum/OutlineEvidence';
import { type OutlineOperation } from '@/features/learning/curriculum/OutlineReviewPanel';
import { PlanPanel } from '@/features/learning/curriculum/PlanPanel';
import {
  handleTabArrowKey,
  LessonsListSection,
  ModuleDetailsDropdown,
  ModuleMilestonesNav,
  PracticeSubNav,
  ProgramHeaderCard,
  ProgramOutcomeDisclosure,
  QuickChecksSection,
  RecallSection,
  SourcesSection,
} from '@/features/learning/curriculum/ProgramWorkspaceSections';
import { StartingPointPanel } from '@/features/learning/curriculum/StartingPointPanel';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { StudyHabitsBar, type FrameworkAction } from '@/features/learning/habits';
import { LessonPreparationLine } from '@/features/learning/lessons/LessonPreparationLine';
import { LessonPreparationStatus } from '@/features/learning/lessons/LessonPreparationProgress';
import { currentPreparationJobs, heldLessonIds } from '@/features/learning/lessons/review/claimReviewPresentation';
import { NotebookPanel } from '@/features/learning/memory/MemoryPanels';
import { useLearningMemory } from '@/features/learning/memory/useLearningMemory';
import { findOutlineCitation } from '@/features/learning/model/evidencePresentation';
import { PortabilityPanel } from '@/features/learning/portability/PortabilityPanel';
import { FeynmanTechniquePanel } from '@/features/learning/practice/FeynmanTechniquePanel';
import { PracticalWorkbenchPanel } from '@/features/learning/practice/practical/PracticalWorkbenchPanel';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { CourseIndexNotice } from '@/features/learning/sources/CourseIndexNotice';
import { useCourseIndexOpen, useEvidenceWaitsForIndex } from '@/features/learning/sources/useCourseIndexStatus';
import { ProgramTangents } from '@/features/learning/tangents/ProgramTangents';
import { useRepairLearningOutline } from '@/features/learning/workspace/useLearningStudio';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useEffectiveTheme } from '@/hooks/useApplyTheme';
import type { LearningAssessmentKind, LearningLessonDto, LearningModuleDto, LearningProgramDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';
import type { StudyDestination, StudyTab } from '@/lib/studyActivity';

const LazyCanvasPanel = lazy(() => import('@/features/learning/canvas/CanvasPanel'));

type Tab = StudyTab;
type NotebookDraft = { title: string; content: string };

export function ProgramWorkspace({
  program, destination, outlineGeneration, initialTangentId, onBack, onOpenProgram,
  onAccept, acceptPending, acceptError, onPrepare, preparePending, prepareError,
  onComplete, completePending, completeError, formFor, setAnswer, onSubmitAttempt,
  submitPending, submitError, onReview,
}: {
  destination?: StudyDestination;
  outlineGeneration?: OutlineOperation;
  initialTangentId?: string | null;
  program: LearningProgramDto; onBack: () => void; onAccept: (title: string) => void; acceptPending: boolean; acceptError?: string;
  onOpenProgram: (programId: string) => void;
  onPrepare: (lesson: LearningLessonDto) => void; preparePending: boolean; prepareError?: string;
  onComplete: (lesson: LearningLessonDto) => void; completePending: boolean; completeError?: string;
  formFor: (key: string, questions: { id: string }[]) => FormSnapshot; setAnswer: (key: string, questionId: string, value: number) => void;
  onSubmitAttempt: (key: string, kind: LearningAssessmentKind, module: LearningModuleDto, lesson: LearningLessonDto | null, questions: LearningLessonDto['questions'], form: FormSnapshot, attemptId: string) => void;
  submitPending: boolean; submitError?: string;
  onReview?: (programId: string, lessonId: string) => void;
}) {
  const [moduleId, setModuleId] = useState(() => program.modules.find((m) => m.lessons.some((l) => l.id === program.summary.currentLessonId))?.id ?? program.modules[0]?.id ?? '');
  const [lessonId, setLessonId] = useState(program.summary.currentLessonId ?? program.modules[0]?.lessons[0]?.id ?? '');
  const [tab, setTab] = useState<Tab>('lessons');
  const [lastPracticeTab, setLastPracticeTab] = useState<Tab>('workbench');
  const [preparationExpanded, setPreparationExpanded] = useState(false);
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
  const [workbenchMode, setWorkbenchMode] = useState<'practice' | 'feynman'>('practice');
  const [sourcesView, setSourcesView] = useState<'library' | 'continuity'>('library');
  const [recallView, setRecallView] = useState<'lesson' | 'evolution' | 'brain-dump'>('lesson');
  const [importEpoch, setImportEpoch] = useState(0);

  const handleImported = (importedProgramId: string) => {
    if (importedProgramId !== program.summary.id) { onOpenProgram(importedProgramId); return; }
    setWrittenOpened(false);
    setPracticalOpened(false);
    setCanvasOpened(false);
    setWorkbenchMode('practice');
    setRequestedFormId(null);
    setProjectRequest(null);
    setNotebookDrafts({});
    setImportEpoch((epoch) => epoch + 1);
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
    if (destination.formId) setRequestedFormId(destination.formId);
    const resolvedTab = next === 'study' ? 'lessons' : next;
    setTab(resolvedTab);
    setMoreOpen(false);
    if (resolvedTab === 'canvas') setCanvasOpened(true);
    if (resolvedTab === 'practical') setPracticalOpened(true);
    if (resolvedTab === 'workbench') setWrittenOpened(true);
    if (['workbench', 'practical', 'quick-checks', 'assess-evidence'].includes(resolvedTab)) setLastPracticeTab(resolvedTab);
  }, [destination, program.modules]);

  const [title, setTitle] = useState(program.summary.title);
  const [notebookDrafts, setNotebookDrafts] = useState<Record<string, NotebookDraft>>({});
  const [savingBeforeMove, setSavingBeforeMove] = useState(false);
  const repair = useRepairLearningOutline(program.summary.id);
  const outlineBlocked = repair.isPending || outlineGeneration?.isPending || Boolean(program.outlineReview && program.outlineReview.status !== 'passed');
  const activeProgram = program.summary.status === 'active';
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const outlineEvidence = useLearningOutlineEvidence(program.summary.id, program.summary.revision, showCitations && program.sources.length > 0);
  const visibleOutlineEvidence = showCitations && !outlineEvidence.isError ? outlineEvidence.data : null;
  const preparationPlan = useLearningPlan(activeProgram ? program.summary.id : '', preparePending);
  const preparationJobs = preparationPlan.data?.jobs?.filter((job) => job.kind === 'lesson_preparation').sort((a, b) => b.createdAt - a.createdAt) ?? [];
  const preparationJob = preparationJobs.find((job) => job.status === 'running') ?? preparationJobs.find((job) => job.status === 'pending') ?? preparationJobs[0];
  const preparingLesson = preparePending || Boolean(preparationPlan.data?.jobs?.some((job) => ['pending', 'running'].includes(job.status)));
  useCourseIndexOpen(program.summary.id);
  const waitsForIndex = useEvidenceWaitsForIndex(program.summary.id);
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
  const modulePrepared = Boolean(currentModule?.lessons.length && currentModule.lessons.every((lesson) => lesson.preparation === 'ready'));
  const testCoverage = currentModule ? moduleTestCoverage(currentModule) : null;
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
  const selectTab = (next: Tab) => { void afterNotebookSave(() => activateTab(next)); };
  const selectPracticeGroup = () => selectTab(lastPracticeTab);
  const selectMoreTab = (next: Tab) => {
    void afterNotebookSave(() => {
      activateTab(next);
      requestAnimationFrame(() => document.getElementById('workspace-panel')?.focus());
    });
  };
  const practiceTab = tab === 'workbench' || tab === 'practical' || tab === 'quick-checks' || tab === 'assess-evidence';

  const handleSelectFramework = (action: FrameworkAction) => {
    if (action === 'recall-dump') {
      setRecallView('brain-dump');
      selectTab('recall');
    } else if (action === 'spaced-repetition') {
      setRecallView('evolution');
      selectTab('recall');
    } else if (action === 'feynman') {
      setWorkbenchMode('feynman');
      setWrittenOpened(true);
      selectTab('workbench');
    } else if (action === 'interleaving') {
      setRecallView('lesson');
      selectTab('recall');
    } else if (action === 'dual-coding') {
      selectMoreTab('canvas');
    } else if (action === 'handwritten-notes') {
      selectTab('notebook');
    }
  };

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
        if (target?.lesson.preparation === 'outline' && !preparingLesson && !waitsForIndex) onPrepare(target.lesson);
        if (step.kind === 'guided' && target) requestAnimationFrame(() => document.getElementById(`guided-${target.lesson.id}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' }));
      }
    });
  };

  const questionBundle = (kind: LearningAssessmentKind) => {
    if (kind === 'test') return moduleTestQuestions;
    return currentLesson?.questions.filter((question) => question.kind === kind) ?? [];
  };
  const formKey = (kind: LearningAssessmentKind) => `${program.summary.id}:${currentModule?.id}:${kind === 'test' ? 'module' : currentLesson?.id ?? 'none'}:${kind}:${importEpoch}`;
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

  const effectiveTangentId = initialTangentId ?? destination?.tangentId ?? null;
  const goal = program.summary.goal.trim();
  const shownTitle = (program.summary.status === 'draft' ? title : program.summary.title).trim();
  const showGoal = Boolean(goal) && goal !== shownTitle && !shownTitle.startsWith(goal.slice(0, 80));
  const lessonTitles = Object.fromEntries(allLessons.map(({ lesson }) => [lesson.id, lesson.title]));
  const openReview = onReview ? (programId: string, reviewLessonId: string) => void afterNotebookSave(() => onReview(programId, reviewLessonId)) : undefined;
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
      onSelectLesson={(mId, lId) => { void afterNotebookSave(() => { setModuleId(mId); setLessonId(lId); activateTab('lessons'); }); }}
    >
      <div className="mx-auto max-w-[1500px] pb-12">
        <ProgramHeaderCard
          program={program} activeProgram={activeProgram} savingBeforeMove={savingBeforeMove}
          onBack={() => void afterNotebookSave(onBack)} completedCount={completedCount} allLessonsCount={allLessons.length}
          showCitations={showCitations} moduleId={moduleId} visibleOutlineEvidence={visibleOutlineEvidence}
          outlineEvidenceReport={outlineEvidence}
          onSelectSyllabusLesson={(m, sId) => void afterNotebookSave(() => { setModuleId(m.id); setLessonId(sId); setTab('lessons'); requestAnimationFrame(() => document.getElementById(`lesson-${sId}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })); })}
          title={title} setTitle={setTitle} acceptPending={acceptPending} acceptError={acceptError}
          outlineBlocked={outlineBlocked} onAccept={onAccept} showGoal={showGoal} goal={goal}
          outlineGeneration={outlineGeneration} repair={repair} currentModule={currentModule} currentLesson={currentLesson}
          onSelectModule={(next) => void afterNotebookSave(() => { setModuleId(next.id); setLessonId(next.lessons.find((l) => !l.completed)?.id ?? next.lessons[0]?.id ?? ''); setTab('lessons'); })}
          onSelectLessonId={(nextId) => void afterNotebookSave(() => setLessonId(nextId))}
          dueCount={memoryQuery.data?.dueCount ?? 0} preparePending={preparePending} goToStep={goToStep} resume={resume}
          onContinueLesson={(mId, lId) => void afterNotebookSave(() => { setModuleId(mId); setLessonId(lId); setTab('lessons'); })}
        />

        {!activeProgram && (
          <CourseSyllabus
            program={program} moduleId={moduleId} evidence={visibleOutlineEvidence}
            onSelect={(m, sId) => void afterNotebookSave(() => { setModuleId(m.id); setLessonId(sId); setTab('lessons'); requestAnimationFrame(() => document.getElementById(`lesson-${sId}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })); })}
          />
        )}

        <main className="mt-4 min-w-0">
          {activeProgram && (<StudyHabitsBar className="mb-3" onSelectFramework={handleSelectFramework} />)}

          <section id="workspace-panel" tabIndex={-1} aria-label="Program workspace content" className="rounded-xl border border-border-default bg-surface shadow-xs">
            <div className="border-b border-border-subtle px-5 pt-4 pb-3">
              <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
                <div className="min-w-0 flex-1">
                  <h2 className="truncate font-serif text-lg font-semibold text-text-primary">{currentModule?.title}</h2>
                  <p className="mt-0.5 line-clamp-1 text-xs text-text-secondary">{currentModule?.summary}</p>
                  {showCitations && currentModule && (
                    <OutlineCitation
                      citation={findOutlineCitation(
                        visibleOutlineEvidence,
                        'module_summary',
                        currentModule.id,
                        currentModule.summary,
                      )}
                      evidence={visibleOutlineEvidence}
                    />
                  )}
                </div>
                <ModuleDetailsDropdown currentModule={currentModule} showCitations={showCitations} visibleOutlineEvidence={visibleOutlineEvidence} />
              </div>

              <nav aria-label="Module workspace" className="mt-3">
                <div role="tablist" aria-label="Program workspace" className="flex flex-wrap items-center gap-1 rounded-lg border border-border-subtle bg-surface-sunken p-1 text-xs">
                  {primary
                    .filter((item) => activeProgram || item.id === 'lessons' || item.id === 'sources')
                    .map((item) => {
                      const isSelected = tab === item.id || (item.id === 'workbench' && practiceTab);
                      return (
                        <button
                          key={item.id}
                          id={`workspace-tab-${item.id}`}
                          aria-controls="workspace-panel"
                          role="tab"
                          aria-selected={isSelected}
                          tabIndex={isSelected ? 0 : -1}
                          type="button"
                          disabled={savingBeforeMove}
                          onClick={() => (item.id === 'workbench' ? selectPracticeGroup() : selectTab(item.id))}
                          onKeyDown={handleTabArrowKey}
                          className={`relative min-h-8 rounded-md px-3 py-1.5 font-medium transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 ${
                            isSelected ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
                          }`}
                        >
                          {item.label}
                          {item.id === 'recall' && memoryQuery.data?.dueCount ? (
                            <span className="ml-1.5 rounded-full bg-accent-muted px-1.5 py-0.5 text-xxs font-semibold text-accent">
                              {memoryQuery.data.dueCount}
                            </span>
                          ) : null}
                        </button>
                      );
                    })}

                  <div className="relative ml-auto">
                    <button
                      ref={moreButtonRef}
                      type="button"
                      aria-haspopup="menu"
                      aria-expanded={moreOpen}
                      aria-current={['plan', 'canvas', 'portability'].includes(tab) ? 'page' : undefined}
                      aria-controls="workspace-more-menu"
                      onClick={() => setMoreOpen((value) => !value)}
                      className="min-h-8 rounded-md px-3 py-1.5 font-medium text-text-secondary hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
                    >
                      More <span aria-hidden="true">⌄</span>
                    </button>
                    {moreOpen && (
                      <div
                        id="workspace-more-menu"
                        ref={moreMenuRef}
                        role="menu"
                        aria-label="More workspace tools"
                        onKeyDown={(event) => {
                          if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
                          event.preventDefault();
                          const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
                          const index = items.indexOf(document.activeElement as HTMLButtonElement);
                          const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length;
                          items[next]?.focus();
                        }}
                        className="absolute right-0 top-full z-30 mt-1 w-52 rounded-xl border border-border-default bg-surface p-1.5 shadow-xl"
                      >
                        {(activeProgram
                          ? [{ id: 'plan' as const, label: 'Plan' }, { id: 'canvas' as const, label: 'Canvas' }, { id: 'portability' as const, label: 'Import & export' }]
                          : [{ id: 'portability' as const, label: 'Import & export' }]
                        ).map((item) => (
                          <button
                            key={item.id}
                            type="button"
                            role="menuitem"
                            aria-current={tab === item.id ? 'page' : undefined}
                            onClick={() => selectMoreTab(item.id)}
                            className="flex min-h-9 w-full items-center rounded-lg px-3 text-left text-xs text-text-primary hover:bg-surface-raised focus-visible:bg-surface-raised focus-visible:outline-hidden"
                          >
                            {item.label}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              </nav>
            </div>

            {practiceTab && (
              <PracticeSubNav
                currentModule={currentModule} currentLesson={currentLesson} tab={tab}
                savingBeforeMove={savingBeforeMove} onSelectTab={selectTab}
              />
            )}

            <div className="p-4 sm:p-5">
              {activeProgram && tab === 'starting_point' && (
                <div>
                  <StartingPointPanel
                    programId={program.summary.id}
                    onStudy={(lId) => goToStep({ kind: 'lesson', title: '', reason: '', lessonId: lId })}
                    onChallenge={(lId) => goToStep({ kind: 'assessment', title: '', reason: '', lessonId: lId })}
                  />
                </div>
              )}

              <CourseIndexNotice programId={program.summary.id} />

              {activeProgram &&
                (tab === 'lessons' ? (
                  <LessonPreparationStatus
                    job={preparationJob} pending={preparePending} programId={program.summary.id} revision={program.summary.revision}
                    error={prepareError} expanded={preparationExpanded} onExpandedChange={setPreparationExpanded}
                    lessonTitles={lessonTitles} onReview={openReview}
                  />
                ) : (
                  <LessonPreparationLine
                    job={preparationJob} pending={preparePending} programId={program.summary.id}
                    error={prepareError} onView={() => selectTab('lessons')}
                  />
                ))}

              {activeProgram && preparationPlan.isError && preparePending && (
                <p role="alert" className="my-3 text-xs text-danger-fg">
                  Progress could not be loaded.{' '}
                  <button type="button" className="underline" onClick={() => void preparationPlan.refetch()}>
                    Retry progress
                  </button>
                </p>
              )}

              {tab === 'lessons' && (
                <LessonsListSection
                  currentModule={currentModule} visibleOutlineEvidence={visibleOutlineEvidence} program={program}
                  lessonId={lessonId} currentLessonId={program.summary.currentLessonId} resumeLessonId={resume?.lesson.id}
                  completePending={completePending} preparingLesson={preparingLesson} onPrepare={onPrepare} onComplete={onComplete}
                  onSelectLesson={(lId) => void afterNotebookSave(() => setLessonId(lId))}
                  onPracticeLesson={(lId) => void afterNotebookSave(() => { setLessonId(lId); setTab('workbench'); setWrittenOpened(true); setLastPracticeTab('workbench'); })}
                  onReview={openReview}
                />
              )}

              {activeProgram && tab === 'lessons' && (
                <ModuleMilestonesNav
                  currentModule={currentModule} program={program} modulePrepared={modulePrepared} onSelectTab={selectTab}
                  onStartProject={(anchorId, finalModule) => void afterNotebookSave(() => {
                    if (!currentModule) return;
                    setLessonId(anchorId);
                    const scope = finalModule ? program.modules : [currentModule];
                    setProjectRequest({
                      id: crypto.randomUUID(),
                      brief: `Create a ${finalModule ? 'final capstone integrating the complete course' : 'module project'} for ${program.summary.goal}. Project progression: ${scope.map((item) => item.project ? `${item.project.title}: ${item.project.brief} Deliverables: ${item.project.deliverables.join('; ')}. Success criteria: ${item.project.successCriteria.join('; ')}` : item.summary).join(' | ')}. Each milestone must extend the same project artifact and include a revision after feedback. Outcomes: ${scope.flatMap((item) => item.outcomes).join('; ')}. Require a concrete artifact, milestones, constraints, an explicit grading rubric, and a reflection on tradeoffs. Build on these lessons: ${scope.flatMap((item) => item.lessons.map((lesson) => lesson.title)).join('; ')}.`.slice(0, 4000),
                    });
                    activateTab('practical');
                  })}
                />
              )}

              {activeProgram && tab === 'plan' && (
                <div className="mt-4">
                  <PlanPanel programId={program.summary.id} onStartingPoint={() => selectTab('starting_point')} />
                </div>
              )}

              {tab === 'portability' && (
                <div className="mt-4">
                  <PortabilityPanel programId={program.summary.id} onImported={handleImported} />
                </div>
              )}

              {activeProgram && (practiceTab || writtenOpened || practicalOpened) && (
                <div id="practice-content" role="tabpanel" tabIndex={0} aria-labelledby={`practice-tab-${practiceTab ? tab : lastPracticeTab}`} hidden={!practiceTab} className="mt-3">
                  {activeProgram && tab === 'quick-checks' && (
                    <QuickChecksSection
                      program={program} quickKind={quickKind} setQuickKind={setQuickKind} showQuickResults={showQuickResults} setShowQuickResults={setShowQuickResults}
                      moduleAttempts={moduleAttempts} testCoverage={testCoverage} assessmentKey={assessmentKey} chosenQuestions={chosenQuestions}
                      form={form} submitPending={submitPending} submitError={submitError} setAnswer={setAnswer}
                      onSubmitAttempt={(attemptId) => currentModule && onSubmitAttempt(assessmentKey, quickKind, currentModule, quickKind === 'test' ? null : currentLesson ?? null, chosenQuestions, form, attemptId)}
                      onReviewLesson={() => setTab('lessons')}
                    />
                  )}

                  {activeProgram && tab === 'assess-evidence' && (
                    <div className="mt-4">
                      <AssessmentEvidencePanel requestedFormId={requestedFormId} programId={program.summary.id} program={program} module={currentModule} />
                    </div>
                  )}

                  {activeProgram && writtenOpened && (
                    <div hidden={tab !== 'workbench'} className="mt-4 space-y-4">
                      <div role="group" aria-label="Written practice mode" className="inline-flex flex-wrap gap-1 rounded-lg border border-border-subtle bg-surface-sunken p-1 text-xs">
                        <button
                          type="button"
                          aria-pressed={workbenchMode === 'practice'}
                          onClick={() => setWorkbenchMode('practice')}
                          className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                            workbenchMode === 'practice' ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
                          }`}
                        >
                          Grounded practice
                        </button>
                        <button
                          type="button"
                          aria-pressed={workbenchMode === 'feynman'}
                          onClick={() => setWorkbenchMode('feynman')}
                          className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                            workbenchMode === 'feynman' ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
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

                  {activeProgram && practicalOpened && (
                    <div hidden={tab !== 'practical'} className="mt-4">
                      <PracticalWorkbenchPanel program={program} lesson={currentLesson ?? null} projectRequest={projectRequest} />
                    </div>
                  )}
                </div>
              )}

              {tab === 'sources' && (
                <SourcesSection program={program} sourcesView={sourcesView} setSourcesView={setSourcesView} onOpenReview={openHeldReview} />
              )}

              {activeProgram && tab === 'notebook' && (
                <div className="mt-4">
                  <NotebookPanel
                    enabled program={program} lessonId={currentLesson?.id} memory={memoryQuery.data} memoryLoading={memoryQuery.isLoading}
                    memoryError={memoryQuery.error?.message} onRetryMemory={() => void memoryQuery.refetch()} drafts={notebookDrafts}
                    setDraft={(key, draft) => setNotebookDrafts((current) => ({ ...current, [key]: draft }))}
                  />
                </div>
              )}

              {activeProgram && canvasOpened && tab === 'canvas' && (
                <div className="mt-4">
                  <Suspense fallback={<div role="status" className="rounded-xl border border-border-default bg-surface p-8 text-xs text-text-muted">Opening Canvas…</div>}>
                    <LazyCanvasPanel
                      programId={program.summary.id} lessonId={currentLesson?.id} lessonTitle={currentLesson?.title ?? 'Learning canvas'}
                      theme={theme} enabled lesson={currentLesson}
                    />
                  </Suspense>
                </div>
              )}

              {activeProgram && tab === 'recall' && (
                <RecallSection
                  program={program} currentLesson={currentLesson} recallView={recallView}
                  setRecallView={setRecallView} memoryQuery={memoryQuery}
                />
              )}

              {(prepareError || completeError) && tab === 'lessons' && (
                <p role="alert" className="mt-4 rounded-xl border border-danger/20 bg-danger-muted/30 p-4 text-xs text-danger-fg">
                  {prepareError ? `Lesson preparation failed: ${prepareError}` : `Completion could not be saved: ${completeError}`}
                </p>
              )}
            </div>
          </section>
        </main>

        <ProgramOutcomeDisclosure
          program={program} activeProgram={activeProgram} readyCount={readyCount} allLessonsLength={allLessons.length}
          practiceCount={practiceCount} demonstratedCount={demonstratedCount} memoryQuery={memoryQuery}
          savingBeforeMove={savingBeforeMove} onOpenSources={() => selectTab('sources')}
        />
      </div>
    </ProgramTangents>
  );
}
