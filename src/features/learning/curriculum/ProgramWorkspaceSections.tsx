import type React from 'react';

import { ArrowLeft, ArrowRight, Layers3 } from 'lucide-react';

import { AssessmentPanel, AttemptReview, type FormSnapshot } from '@/features/learning/assessment/AssessmentPanel';
import { type moduleTestCoverage } from '@/features/learning/assessment/moduleTestCoverage';
import { CourseCoach } from '@/features/learning/curriculum/CourseCoach';
import { type CourseStep } from '@/features/learning/curriculum/courseProgress';
import { CourseSyllabus } from '@/features/learning/curriculum/CourseSyllabus';
import { OutlineCitation, OutlineEvidenceStatus } from '@/features/learning/curriculum/OutlineEvidence';
import { OutlineReviewPanel, type OutlineOperation } from '@/features/learning/curriculum/OutlineReviewPanel';
import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { RecallPanel } from '@/features/learning/memory/MemoryPanels';
import { useSaveLearningCardDraft, type useLearningMemory } from '@/features/learning/memory/useLearningMemory';
import { findOutlineCitation } from '@/features/learning/model/evidencePresentation';
import { ActiveRecallBrainDump } from '@/features/learning/recall/ActiveRecallBrainDump';
import { RecallEvolutionPanel } from '@/features/learning/recall/RecallEvolutionPanel';
import { SourceLine } from '@/features/learning/sources/SourceLine';
import { SourceMaintenancePanel } from '@/features/learning/sources/SourceMaintenancePanel';
import { SourcesPanel } from '@/features/learning/sources/SourcesPanel';
import { ProgramTangentsTrigger } from '@/features/learning/tangents/ProgramTangents';
import { type useRepairLearningOutline } from '@/features/learning/workspace/useLearningStudio';
import { CitationVisibilityToggle } from '@/features/reading/components/CitationVisibilityToggle';
import type {
  LearningAssessmentKind,
  LearningAttemptDto,
  LearningLessonDto,
  LearningModuleDto,
  LearningOutlineEvidenceDto,
  LearningProgramDto,
} from '@/lib/bindings';
import type { StudyTab } from '@/lib/studyActivity';

export function handleTabArrowKey(event: React.KeyboardEvent<HTMLButtonElement>) {
  if (!['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(event.key)) return;
  event.preventDefault();
  const buttons = Array.from(event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? []);
  const index = buttons.indexOf(event.currentTarget);
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : buttons.length - 1)) % buttons.length;
  buttons[next]?.focus();
}

export interface ProgramHeaderCardProps {
  program: LearningProgramDto;
  activeProgram: boolean;
  savingBeforeMove: boolean;
  onBack: () => void;
  completedCount: number;
  allLessonsCount: number;
  showCitations: boolean;
  moduleId: string;
  visibleOutlineEvidence?: LearningOutlineEvidenceDto | null;
  outlineEvidenceReport: Parameters<typeof OutlineEvidenceStatus>[0]['report'];
  onSelectSyllabusLesson: (module: LearningModuleDto, lessonId: string) => void;
  title: string;
  setTitle: (title: string) => void;
  acceptPending: boolean;
  acceptError?: string;
  outlineBlocked: boolean;
  onAccept: (title: string) => void;
  showGoal: boolean;
  goal: string;
  outlineGeneration?: OutlineOperation;
  repair: ReturnType<typeof useRepairLearningOutline>;
  currentModule?: LearningModuleDto;
  currentLesson?: LearningLessonDto;
  onSelectModule: (module: LearningModuleDto) => void;
  onSelectLessonId: (lessonId: string) => void;
  dueCount: number;
  preparePending: boolean;
  goToStep: (step: CourseStep) => void;
  resume?: { module: LearningModuleDto; lesson: LearningLessonDto };
  onContinueLesson: (moduleId: string, lessonId: string) => void;
}

export function ProgramHeaderCard({
  program, activeProgram, savingBeforeMove, onBack, completedCount, allLessonsCount,
  showCitations, moduleId, visibleOutlineEvidence, outlineEvidenceReport, onSelectSyllabusLesson,
  title, setTitle, acceptPending, acceptError, outlineBlocked, onAccept, showGoal, goal,
  outlineGeneration, repair, currentModule, currentLesson, onSelectModule, onSelectLessonId,
  dueCount, preparePending, goToStep, resume, onContinueLesson,
}: ProgramHeaderCardProps) {
  return (
    <header className="rounded-xl border border-border-default bg-surface shadow-xs overflow-hidden">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border-subtle bg-surface-sunken/50 px-4 py-2.5">
        <div className="flex items-center gap-3">
          <button
            type="button"
            disabled={savingBeforeMove}
            onClick={onBack}
            className="inline-flex min-h-8 items-center gap-1.5 text-xs font-medium text-text-secondary hover:text-text-primary disabled:opacity-50 transition-colors"
          >
            <ArrowLeft size={14} /> {savingBeforeMove ? 'Saving your work…' : 'All programs'}
          </button>
          <span className="inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-surface px-2.5 py-0.5 text-xxs font-medium text-accent">
            <Layers3 size={12} /> {program.summary.status === 'draft' ? 'Draft outline' : 'Active'}
          </span>
          <div className="hidden sm:flex items-center gap-2 text-xs text-text-secondary">
            <span className="tabular-nums font-medium text-text-primary">{completedCount}/{allLessonsCount}</span>
            <span>lessons</span>
            <div className="h-1.5 w-20 overflow-hidden rounded-full bg-surface-raised">
              <div
                className="h-full rounded-full bg-accent transition-all"
                style={{ width: `${allLessonsCount ? (completedCount / allLessonsCount) * 100 : 0}%` }}
              />
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <CitationVisibilityToggle />
          <ProgramTangentsTrigger />
          {activeProgram && (
            <CourseSyllabus
              compact
              program={program}
              moduleId={moduleId}
              evidence={visibleOutlineEvidence}
              onSelect={onSelectSyllabusLesson}
            />
          )}
        </div>
      </div>

      <div className="px-5 py-4">
        {program.summary.status === 'draft' ? (
          <>
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div className="min-w-0 flex-1">
                <h1 className="sr-only">{title || 'Untitled program'}</h1>
                <label className="sr-only" htmlFor="program-title">Program title</label>
                <input
                  id="program-title"
                  aria-label="Program title"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  className="w-full max-w-2xl border-b border-border-default bg-transparent font-serif text-2xl leading-tight text-text-primary outline-hidden focus:border-accent"
                />
              </div>
              <div className="flex items-center gap-3">
                {acceptError && <span role="alert" className="max-w-xs text-xs text-danger-fg">{acceptError}</span>}
                <button
                  type="button"
                  disabled={acceptPending || outlineBlocked || !title.trim()}
                  onClick={() => onAccept(title.trim())}
                  className="min-h-9 rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors disabled:opacity-50 shadow-2xs"
                >
                  {acceptPending ? 'Saving…' : 'Accept program'}
                </button>
              </div>
            </div>
            <p className="mt-2 text-xs text-text-secondary">
              {outlineBlocked ? 'Resolve the outline findings before accepting this course.' : 'Accept to prepare lessons individually and track progress.'}
            </p>
          </>
        ) : (
          <h1 className="line-clamp-2 wrap-break-word font-serif text-2xl leading-snug text-text-primary sm:text-[1.75rem]">
            {program.summary.title}
          </h1>
        )}

        {showGoal && (
          <details className="mt-2 max-w-3xl text-sm">
            <summary className="cursor-pointer text-xs font-medium text-text-secondary hover:text-text-primary">Goal</summary>
            <p className="mt-1 whitespace-pre-line text-xs leading-5 text-text-secondary">{goal}</p>
          </details>
        )}

        {showCitations && <div className="mt-3"><OutlineEvidenceStatus report={outlineEvidenceReport} /></div>}
        <OutlineReviewPanel program={program} generation={outlineGeneration} repair={repair} />
        {!program.sources.length && (
          <p className="mt-2.5 text-xs leading-5 text-text-muted">
            Topic-based course · AI-authored from general knowledge. Add references in Sources to ground future lessons.
          </p>
        )}
      </div>

      <div className="border-t border-border-subtle bg-surface-sunken/30 px-5 py-3">
        <div className="grid gap-3 lg:grid-cols-[1fr_1fr_auto] lg:items-center">
          <div className="flex items-center gap-2 min-w-0">
            <label htmlFor="module-select" className="inline-flex shrink-0 items-center gap-1.5 text-xs font-medium text-text-secondary">
              <Layers3 size={14} className="text-accent" /> Module
            </label>
            <select
              id="module-select"
              aria-label="Module"
              value={currentModule?.id ?? ''}
              disabled={savingBeforeMove}
              onChange={(event) => {
                const next = program.modules.find((item) => item.id === event.target.value);
                if (next) onSelectModule(next);
              }}
              className="min-h-9 min-w-0 flex-1 rounded-lg border border-border-default bg-surface px-3 py-1.5 text-xs text-text-primary shadow-2xs focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
            >
              {program.modules.map((module, index) => (
                <option key={module.id} value={module.id}>
                  Module {index + 1}: {module.title} ({module.lessons.filter((item) => item.completed).length}/{module.lessons.length} complete)
                </option>
              ))}
            </select>
          </div>

          <div className="flex items-center gap-2 min-w-0">
            <label htmlFor="lesson-select" className="shrink-0 text-xs font-medium text-text-secondary">Lesson</label>
            <select
              id="lesson-select"
              aria-label="Lesson context"
              value={currentLesson?.id ?? ''}
              disabled={savingBeforeMove || !currentModule?.lessons.length}
              onChange={(event) => onSelectLessonId(event.target.value)}
              className="min-h-9 min-w-0 flex-1 rounded-lg border border-border-default bg-surface px-3 py-1.5 text-xs text-text-primary shadow-2xs focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
            >
              <option value="">No lesson selected</option>
              {currentModule?.lessons.map((lesson, index) => (
                <option key={lesson.id} value={lesson.id}>{index + 1}. {lesson.title}</option>
              ))}
            </select>
          </div>

          <div className="flex items-center shrink-0">
            {activeProgram ? (
              <CourseCoach program={program} dueCount={dueCount} disabled={savingBeforeMove || preparePending} onStep={goToStep} />
            ) : resume ? (
              <button
                type="button"
                aria-label={`Continue lesson: ${resume.lesson.title}`}
                title={resume.lesson.title}
                disabled={savingBeforeMove}
                onClick={() => onContinueLesson(resume.module.id, resume.lesson.id)}
                className="inline-flex min-h-9 shrink-0 items-center gap-2 whitespace-nowrap rounded-lg bg-accent px-3.5 py-1.5 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors disabled:opacity-50 shadow-2xs"
              >
                Continue lesson <ArrowRight size={14} />
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </header>
  );
}

export function ModuleDetailsDropdown({
  currentModule, showCitations, visibleOutlineEvidence,
}: {
  currentModule?: LearningModuleDto;
  showCitations: boolean;
  visibleOutlineEvidence?: LearningOutlineEvidenceDto | null;
}) {
  return (
    <details className="group relative shrink-0 text-sm">
      <summary className="inline-flex min-h-8 cursor-pointer list-none items-center gap-1.5 rounded-lg border border-border-default bg-surface px-3 py-1.5 text-xs font-medium text-text-secondary hover:text-text-primary hover:border-border-strong focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent shadow-2xs transition-colors">
        <span>Module details</span>
        <span className="text-accent">⌄</span>
      </summary>
      <div className="absolute right-0 z-20 mt-2 w-[min(24rem,calc(100vw-2rem))] rounded-xl border border-border-default bg-surface p-4 shadow-xl">
        <h3 className="font-medium text-text-primary">Outcomes</h3>
        <ul className="mt-2 space-y-3 text-xs leading-5 text-text-secondary">
          {currentModule?.outcomes.map((outcome, index) => (
            <li key={outcome}>
              <div className="flex gap-2"><span aria-hidden="true">•</span><span>{outcome}</span></div>
              {showCitations && (
                <OutlineCitation
                  citation={findOutlineCitation(visibleOutlineEvidence, 'outcome', currentModule.id, outcome, index)}
                  evidence={visibleOutlineEvidence}
                  className="ml-4"
                />
              )}
            </li>
          ))}
        </ul>
      </div>
    </details>
  );
}

export function PracticeSubNav({
  currentModule, currentLesson, tab, savingBeforeMove, onSelectTab,
}: {
  currentModule?: LearningModuleDto;
  currentLesson?: LearningLessonDto;
  tab: StudyTab;
  savingBeforeMove: boolean;
  onSelectTab: (tab: StudyTab) => void;
}) {
  return (
    <div className="border-b border-border-subtle bg-surface-sunken/40 px-5 py-2 flex flex-wrap items-center justify-between gap-2">
      <div className="text-xs text-text-secondary">
        <span className="font-medium text-text-primary">{currentModule?.title}</span>
        {currentLesson && (<><span aria-hidden="true" className="text-text-muted"> › </span><span>{currentLesson.title}</span></>)}
      </div>
      <div role="tablist" aria-label="Practice and assessment" className="flex flex-wrap gap-1 rounded-lg bg-surface-sunken p-1 border border-border-subtle text-xs">
        {[
          { id: 'workbench' as const, label: 'Written practice' },
          { id: 'practical' as const, label: 'Code & simulations' },
          { id: 'quick-checks' as const, label: 'Quick checks' },
          { id: 'assess-evidence' as const, label: 'Assessments' },
        ].map((item) => {
          const isSelected = tab === item.id;
          return (
            <button
              key={item.id}
              id={`practice-tab-${item.id}`}
              aria-controls="practice-content"
              role="tab"
              aria-selected={isSelected}
              tabIndex={isSelected ? 0 : -1}
              type="button"
              disabled={savingBeforeMove}
              onClick={() => onSelectTab(item.id)}
              onKeyDown={handleTabArrowKey}
              className={`min-h-7 rounded-md px-2.5 py-1 text-xs font-medium focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent transition-colors ${
                isSelected ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
              }`}
            >
              {item.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function LessonsListSection({
  currentModule, visibleOutlineEvidence, program, lessonId, currentLessonId, resumeLessonId,
  completePending, preparingLesson, onPrepare, onComplete, onSelectLesson, onPracticeLesson, onReview,
}: {
  currentModule?: LearningModuleDto;
  visibleOutlineEvidence?: LearningOutlineEvidenceDto | null;
  program: LearningProgramDto;
  lessonId: string;
  currentLessonId?: string | null;
  resumeLessonId?: string;
  completePending: boolean;
  preparingLesson: boolean;
  onPrepare: (lesson: LearningLessonDto) => void;
  onComplete: (lesson: LearningLessonDto) => void;
  onSelectLesson: (lessonId: string) => void;
  onPracticeLesson: (lessonId: string) => void;
  onReview?: (programId: string, reviewLessonId: string) => void;
}) {
  return (
    <div className="space-y-2.5">
      {!currentModule?.lessons.length ? (
        <p className="rounded-xl border border-border-subtle bg-surface-sunken p-6 text-sm text-text-muted">
          This module has no lessons in its outline.
        </p>
      ) : (
        currentModule.lessons.map((lesson) => (
          <LessonCard
            key={lesson.id}
            lesson={lesson}
            moduleId={currentModule.id}
            outlineEvidence={visibleOutlineEvidence}
            program={program}
            selected={lesson.id === lessonId}
            isCurrent={lesson.id === currentLessonId || (lesson.id === resumeLessonId && !currentLessonId)}
            pending={completePending || (lesson.preparation === 'outline' && preparingLesson)}
            onPrepare={() => onPrepare(lesson)}
            onComplete={() => onComplete(lesson)}
            onSelect={() => onSelectLesson(lesson.id)}
            onPractice={() => onPracticeLesson(lesson.id)}
            onReview={onReview}
          />
        ))
      )}
    </div>
  );
}

export function ModuleMilestonesNav({
  currentModule, program, modulePrepared, onSelectTab, onStartProject,
}: {
  currentModule: LearningModuleDto | undefined;
  program: LearningProgramDto;
  modulePrepared: boolean;
  onSelectTab: (tab: StudyTab) => void;
  onStartProject: (anchorLessonId: string, finalModule: boolean) => void;
}) {
  return (
    <nav aria-label="Module milestones" className="mt-6 flex flex-wrap items-center gap-2 rounded-lg border border-border-subtle bg-surface-sunken/40 px-4 py-2.5 text-xs">
      <span className="inline-flex items-center gap-1.5 font-medium text-text-secondary">
        <span className="grid h-5 w-5 place-items-center rounded bg-accent-muted text-xxs font-semibold tabular-nums text-accent">01</span>
        <span>Learn · {currentModule?.lessons.filter((item) => item.completed).length ?? 0} of {currentModule?.lessons.length ?? 0} complete</span>
      </span>
      <span className="text-border-subtle" aria-hidden="true">|</span>
      <button
        type="button"
        onClick={() => onSelectTab('workbench')}
        className="inline-flex min-h-7 items-center gap-1.5 rounded-md px-2.5 py-1 text-text-secondary hover:bg-surface hover:text-text-primary transition-colors disabled:opacity-50"
      >
        <span className="text-xxs font-semibold tabular-nums text-text-muted">02</span>
        <span>Apply →</span>
      </button>
      <span className="text-border-subtle" aria-hidden="true">|</span>
      <button
        type="button"
        title={modulePrepared ? 'Explain and apply the module outcomes in a written checkpoint.' : 'Study first, or challenge the module if you already know the material.'}
        onClick={() => onSelectTab('assess-evidence')}
        className="inline-flex min-h-7 items-center gap-1.5 rounded-md px-2.5 py-1 text-text-secondary hover:bg-surface hover:text-text-primary transition-colors disabled:opacity-50"
      >
        <span className="text-xxs font-semibold tabular-nums text-text-muted">03</span>
        <span>Check →</span>
      </button>
      <span className="text-border-subtle" aria-hidden="true">|</span>
      <button
        type="button"
        disabled={!currentModule?.lessons.some((item) => item.preparation === 'ready')}
        onClick={() => {
          if (!currentModule) return;
          const anchor = [...currentModule.lessons].reverse().find((item) => item.preparation === 'ready');
          if (!anchor) return;
          const finalModule = currentModule.id === program.modules.at(-1)?.id;
          onStartProject(anchor.id, finalModule);
        }}
        title="A project brief shaped by these outcomes, with deliverables and a rubric."
        className="inline-flex min-h-7 items-center gap-1.5 rounded-md px-2.5 py-1 text-text-secondary hover:bg-surface hover:text-text-primary transition-colors disabled:opacity-50"
      >
        <span className="text-xxs font-semibold tabular-nums text-text-muted">04</span>
        <span>{currentModule?.id === program.modules.at(-1)?.id ? 'Build your capstone →' : 'Build a module project →'}</span>
      </button>
    </nav>
  );
}

export function QuickChecksSection({
  program, quickKind, setQuickKind, showQuickResults, setShowQuickResults,
  moduleAttempts, testCoverage, assessmentKey, chosenQuestions, form,
  submitPending, submitError, setAnswer, onSubmitAttempt, onReviewLesson,
}: {
  program: LearningProgramDto;
  quickKind: LearningAssessmentKind;
  setQuickKind: (kind: LearningAssessmentKind) => void;
  showQuickResults: boolean;
  setShowQuickResults: (show: boolean) => void;
  moduleAttempts: LearningAttemptDto[];
  testCoverage: ReturnType<typeof moduleTestCoverage> | null;
  assessmentKey: string;
  chosenQuestions: LearningLessonDto['questions'];
  form: FormSnapshot;
  submitPending: boolean;
  submitError?: string;
  setAnswer: (key: string, questionId: string, value: number) => void;
  onSubmitAttempt: (attemptId: string) => void;
  onReviewLesson: () => void;
}) {
  return (
    <div className="mt-4">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap gap-2" role="group" aria-label="Quick check type">
          {(['practice', 'quiz', 'test'] as LearningAssessmentKind[]).map((kind) => (
            <button
              key={kind}
              type="button"
              aria-pressed={!showQuickResults && quickKind === kind}
              onClick={() => { setShowQuickResults(false); setQuickKind(kind); }}
              className={`rounded-lg border px-3 py-1.5 text-xs font-medium capitalize transition-colors ${
                !showQuickResults && quickKind === kind ? 'border-accent bg-accent text-accent-fg' : 'border-border-default bg-surface text-text-secondary hover:text-text-primary'
              }`}
            >
              {kind === 'test' ? 'Module test' : kind}
            </button>
          ))}
          <button
            type="button"
            aria-pressed={showQuickResults}
            onClick={() => setShowQuickResults(true)}
            className={`rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors ${
              showQuickResults ? 'border-accent bg-accent text-accent-fg' : 'border-border-default bg-surface text-text-secondary hover:text-text-primary'
            }`}
          >
            History ({moduleAttempts.length})
          </button>
        </div>
        <p className="text-xs text-text-muted">Quick checks draw on lesson knowledge and show answer keys after submission.</p>
      </div>
      {showQuickResults ? (
        <AttemptReview attempts={moduleAttempts} sources={program.sources} />
      ) : (
        <>
          {quickKind === 'test' && !testCoverage?.coveredLessons ? (
            <div className="mb-4 rounded-xl border border-warning-muted bg-warning-muted/20 p-4 text-xs leading-5 text-warning-fg">
              Prepare at least one lesson in this module to take its test. Browsing the syllabus never changes progress.
            </div>
          ) : null}
          <AssessmentPanel
            key={`${assessmentKey}:${chosenQuestions.map((q) => q.id).join(',')}`}
            kind={quickKind}
            questions={chosenQuestions}
            sources={program.sources}
            coverage={quickKind === 'test' ? testCoverage : null}
            attempt={form}
            pending={submitPending}
            error={submitError}
            onAnswer={(id, value) => setAnswer(assessmentKey, id, value)}
            onSubmit={onSubmitAttempt}
            onReview={onReviewLesson}
          />
          {submitError && <span className="sr-only" role="status">Answers remain saved in this session.</span>}
        </>
      )}
    </div>
  );
}

export function SourcesSection({
  program, sourcesView, setSourcesView, onOpenReview,
}: {
  program: LearningProgramDto;
  sourcesView: 'library' | 'continuity';
  setSourcesView: (view: 'library' | 'continuity') => void;
  onOpenReview: () => void;
}) {
  return (
    <div className="mt-4">
      {program.summary.status === 'active' ? (
        <>
          <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
            <div role="group" aria-label="Sources view" className="flex flex-wrap gap-1 rounded-lg border border-border-subtle bg-surface-sunken p-1 text-xs">
              <button
                type="button"
                aria-pressed={sourcesView === 'library'}
                onClick={() => setSourcesView('library')}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  sourcesView === 'library' ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                Source library
              </button>
              <button
                type="button"
                aria-pressed={sourcesView === 'continuity'}
                onClick={() => setSourcesView('continuity')}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  sourcesView === 'continuity' ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                Source continuity & quotes
              </button>
            </div>
            <p className="text-xs text-text-muted">
              {sourcesView === 'library' ? 'Grounding reference collection and snapshot status' : 'Audit coverage and quote stability across reference updates'}
            </p>
          </div>
          {sourcesView === 'library' ? <SourcesPanel programId={program.summary.id} onOpenReview={onOpenReview} /> : <SourceMaintenancePanel programId={program.summary.id} />}
        </>
      ) : (
        <div className="rounded-xl border border-dashed border-border-default bg-surface-sunken p-6">
          <h3 className="font-serif text-xl text-text-primary">Accept this outline to open its source library</h3>
          <p className="mt-2 max-w-2xl text-xs leading-5 text-text-secondary">
            The outline’s original material is shown in each citation. After acceptance, you can add program-scoped snapshots, read saved versions, search their text, and review changes here.
          </p>
        </div>
      )}
    </div>
  );
}

export function RecallSection({
  program, currentLesson, recallView, setRecallView, memoryQuery,
}: {
  program: LearningProgramDto;
  currentLesson?: LearningLessonDto;
  recallView: 'lesson' | 'evolution' | 'brain-dump';
  setRecallView: (view: 'lesson' | 'evolution' | 'brain-dump') => void;
  memoryQuery: ReturnType<typeof useLearningMemory>;
}) {
  const saveDraft = useSaveLearningCardDraft();

  return (
    <div className="mt-4">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div role="group" aria-label="Recall view" className="flex flex-wrap gap-1 rounded-lg border border-border-subtle bg-surface-sunken p-1 text-xs">
          <button
            type="button"
            aria-pressed={recallView === 'lesson'}
            onClick={() => setRecallView('lesson')}
            className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
              recallView === 'lesson' ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            Lesson recall
          </button>
          <button
            type="button"
            aria-pressed={recallView === 'evolution'}
            onClick={() => setRecallView('evolution')}
            className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
              recallView === 'evolution' ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            Versioned deck & FSRS
          </button>
          <button
            type="button"
            aria-pressed={recallView === 'brain-dump'}
            onClick={() => setRecallView('brain-dump')}
            className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
              recallView === 'brain-dump' ? 'bg-surface text-text-primary shadow-2xs font-medium' : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            Free recall (Brain dump)
          </button>
        </div>
        <p className="text-xs text-text-muted">
          {recallView === 'lesson'
            ? 'Spaced retrieval practice for currently due lesson items'
            : recallView === 'evolution'
              ? 'Deck history, stability factors, and FSRS scheduling'
              : 'Pre-study retrieval and unprompted memory generation'}
        </p>
      </div>

      {recallView === 'lesson' ? (
        <RecallPanel
          program={program}
          lessonId={currentLesson?.id}
          memory={memoryQuery.data}
          memoryLoading={memoryQuery.isLoading}
          memoryError={memoryQuery.error?.message}
          onRetryMemory={() => void memoryQuery.refetch()}
        />
      ) : recallView === 'evolution' ? (
        <RecallEvolutionPanel programId={program.summary.id} active />
      ) : (
        <ActiveRecallBrainDump
          program={program}
          lesson={currentLesson}
          onDraftMissedConcept={(question, answer) => {
            if (!currentLesson) return;
            saveDraft.mutate(
              {
                draftId: null,
                programId: program.summary.id,
                lessonId: currentLesson.id,
                question,
                answer,
                explanation: 'Created from Free Recall (Brain Dump) gap analysis.',
                sourceIds: [],
              },
              { onSuccess: () => setRecallView('lesson') },
            );
          }}
        />
      )}
    </div>
  );
}

export function ProgramOutcomeDisclosure({
  program, activeProgram, readyCount, allLessonsLength, practiceCount, demonstratedCount, memoryQuery, savingBeforeMove, onOpenSources,
}: {
  program: LearningProgramDto;
  activeProgram: boolean;
  readyCount: number;
  allLessonsLength: number;
  practiceCount: number;
  demonstratedCount: number;
  memoryQuery: { isError: boolean; data?: { dueCount: number } | null };
  savingBeforeMove: boolean;
  onOpenSources: () => void;
}) {
  return (
    <details className="mt-4 rounded-xl border border-border-default bg-surface px-5 py-3 shadow-2xs">
      <summary className="cursor-pointer py-1 text-xs font-semibold text-text-secondary hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent transition-colors">
        Progress, source materials & authorship
      </summary>
      <div className="grid gap-3 pt-3 md:grid-cols-2 xl:grid-cols-3">
        <section className="rounded-xl border border-border-default bg-surface-sunken p-4">
          <div className="text-xs font-medium text-text-muted">Outcome evidence</div>
          <h3 className="mt-1 font-serif text-base font-medium text-text-primary">Where you are</h3>
          <p className="mt-1 text-xs leading-5 text-text-muted">Different signals answer different questions.</p>
          <div className="mt-3 space-y-2.5">
            {[
              { label: 'Encountered', value: `${readyCount} of ${allLessonsLength}`, detail: 'Lessons prepared' },
              { label: 'Practiced', value: `${practiceCount}`, detail: 'Practice attempts' },
              { label: 'Demonstrated', value: `${demonstratedCount}`, detail: 'Module test attempts' },
              {
                label: 'Due for recheck',
                value: !activeProgram ? 'After acceptance' : memoryQuery.isError ? 'Unavailable' : memoryQuery.data ? `${memoryQuery.data.dueCount} due` : 'Loading',
                detail: !activeProgram ? 'Review scheduling starts with accepted work' : memoryQuery.isError ? 'Could not load the review schedule' : 'FSRS 6 review schedule',
              },
            ].map((row) => (
              <div key={row.label} className="border-t border-border-subtle pt-2 first:border-0 first:pt-0">
                <div className="flex items-baseline justify-between gap-2">
                  <span className="text-xs font-medium text-text-secondary">{row.label}</span>
                  <span className="text-xs font-semibold tabular-nums text-text-primary">{row.value}</span>
                </div>
                <div className="mt-0.5 text-xxs text-text-muted">{row.detail}</div>
              </div>
            ))}
          </div>
          <p className="mt-3 rounded-lg border border-border-subtle bg-surface px-3 py-2 text-xxs leading-relaxed text-text-secondary">
            A matched answer key records a quiz result; it does not establish mastery or job readiness.
          </p>
        </section>

        <section className="rounded-xl border border-border-default bg-surface-sunken p-4">
          <h3 className="text-xs font-medium text-text-primary">Source materials</h3>
          <p className="mt-1 text-xs leading-5 text-text-secondary">
            {program.sources.length
              ? `${program.sources.length} original ${program.sources.length === 1 ? 'source snapshot' : 'source snapshots'} grounding this program.`
              : 'No source snapshots are attached.'}
          </p>
          {program.summary.status === 'active' && (
            <button
              type="button"
              disabled={savingBeforeMove}
              onClick={onOpenSources}
              className="mt-3 inline-flex min-h-8 items-center text-xs font-semibold text-accent hover:underline disabled:opacity-50"
            >
              Open source library <ArrowRight size={13} className="ml-1" />
            </button>
          )}
          {program.sources.length > 0 && (
            <details className="mt-3">
              <summary className="min-h-8 cursor-pointer py-1.5 text-xs text-text-muted hover:text-text-primary">Original outline excerpts</summary>
              <SourceLine ids={program.sources.map((item) => item.id)} sources={program.sources} />
            </details>
          )}
        </section>

        <section className="rounded-xl border border-border-default bg-surface-sunken p-4">
          <h3 className="text-xs font-medium text-text-primary">Course authorship</h3>
          <p className="mt-1 text-xs leading-5 text-text-secondary">
            Generated with {program.modelName || 'the selected model'}
            {program.sources.length ? ' using your chosen sources' : ' from general knowledge'}. The outline is saved
            before review. Corrections are rechecked and unresolved findings remain available for repair. Lesson
            material receives its own review and source checks. Lesson answer keys are model-authored and shown after
            submission.
          </p>
        </section>
      </div>
    </details>
  );
}
