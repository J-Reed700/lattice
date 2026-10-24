import { useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, ArrowRight, Archive, BookOpenCheck, Plus, Sparkles, Upload } from 'lucide-react';
import { useNavigate, useSearchParams } from 'react-router';

import { type FormSnapshot } from '@/features/learning/assessment/AssessmentPanel';
import { ProgramBuilder } from '@/features/learning/curriculum/ProgramBuilder';
import { clearProgramBuilderDraft } from '@/features/learning/curriculum/programBuilderDraft';
import { ProgramWorkspace } from '@/features/learning/curriculum/ProgramWorkspace';
import { StudyHabitsBar, type FrameworkAction } from '@/features/learning/habits';
import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { ExportAllCourses } from '@/features/learning/portability/ExportAllCourses';
import { ImportCourseFlow } from '@/features/learning/portability/ImportCourseFlow';
import { useExportAllPending } from '@/features/learning/portability/useLearningPortability';
import { FlashcardDeckView } from '@/features/learning/recall/flashcards/FlashcardDeckView';
import { FlashcardsSection } from '@/features/learning/recall/flashcards/FlashcardsSection';
import { CourseTangentsSection } from '@/features/learning/tangents/CourseTangentsSection';
import { StudyActivityPanel } from '@/features/learning/workspace/StudyActivityPanel';
import {
  LEARNING_PROGRAMS_KEY,
  learningProgramKey,
  useAcceptLearningProgram,
  useCompleteLearningLesson,
  useGenerateLearningProgram,
  useLearningProgram,
  useLearningPrograms,
  usePrepareLearningLesson,
  useSubmitLearningAttempt,
} from '@/features/learning/workspace/useLearningStudio';
import { useStudioNavigationGuard } from '@/features/learning/workspace/useStudioNavigationGuard';
import type { LearningAssessmentKind, LearningLessonDto, LearningModuleDto, LearningQuestionDto, SubmitLearningAttemptRequestDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';
import type { StudyDestination, StudyTab } from '@/lib/studyActivity';

type Screen = 'programs' | 'builder' | 'import' | 'export-all';

function message(error: unknown) { return error instanceof Error ? error.message : String(error); }

export function LearningStudioPage() {
  const [destination, setDestination] = useState<StudyDestination>();
  const [navigationError, setNavigationError] = useState<string>();
  const [heldForExport, setHeldForExport] = useState(false);
  // Export-all results exist only on its screen until they arrive, so a task
  // opened from the activity panel meanwhile keeps that screen and says why.
  const exportingAll = useExportAllPending();
  const openTask = async (next: StudyDestination) => {
    setHeldForExport(exportingAll);
    if (exportingAll) { setNavigationError(undefined); return; }
    try {
      if (!await flushPendingSaves()) throw new Error('Your work has not been saved yet. Try again after resolving the save error.');
      setNavigationError(undefined);
      setDestination({ ...next });
    } catch (error) { setNavigationError(message(error)); }
  };
  // `?course=` (with `&lesson=`), as the activity view links, opens that
  // course or lesson like a task does.
  const [searchParams, setSearchParams] = useSearchParams();
  // A review link (`&review=claims`) stays in the address: the review screen
  // reads it, so the browser's Back leaves the screen.
  const reviewLink = searchParams.get('review') === 'claims';
  const requestedCourse = reviewLink ? null : searchParams.get('course');
  const requestedLesson = searchParams.get('lesson');
  const latestOpenTask = useRef(openTask);
  latestOpenTask.current = openTask;
  useEffect(() => {
    if (!requestedCourse) return;
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      next.delete('course');
      next.delete('lesson');
      return next;
    }, { replace: true });
    void latestOpenTask.current(requestedLesson
      ? { programId: requestedCourse, lessonId: requestedLesson, tab: 'lessons' }
      : { programId: requestedCourse });
  }, [requestedCourse, requestedLesson, setSearchParams]);
  return <div className="flex h-full min-h-0 flex-col bg-bg">
    <StudyActivityPanel onOpen={next => void openTask(next)} />
    {navigationError && <p role="alert" className="px-6 py-3 text-sm text-danger-fg">Could not save before opening this task: {navigationError}</p>}
    {heldForExport && exportingAll && <p role="status" className="px-6 py-3 text-sm text-text-secondary">Your courses are still being exported. Open the task again when the export finishes.</p>}
    <div className="min-h-0 flex-1"><LearningStudioContent destination={destination} exportingAll={exportingAll} onOpenDestination={next => void openTask(next)} /></div>
  </div>;
}

const REVIEW_PARAMS = ['course', 'lesson', 'review'] as const;

function LearningStudioContent({ destination, exportingAll, onOpenDestination }: { destination?: StudyDestination; exportingAll: boolean; onOpenDestination?: (destination: StudyDestination) => void }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const client = useQueryClient();
  const [screen, setScreen] = useState<Screen>('programs');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const navigation = useStudioNavigationGuard(selectedId !== null);
  const [forms, setForms] = useState<Record<string, FormSnapshot>>({});
  const programs = useLearningPrograms();
  const detail = useLearningProgram(selectedId);
  const generate = useGenerateLearningProgram();
  const accept = useAcceptLearningProgram();
  const prepare = usePrepareLearningLesson();
  const complete = useCompleteLearningLesson();
  const submit = useSubmitLearningAttempt();
  const handledNewRoute = useRef(false);
  const flashcardsRef = useRef<HTMLElement>(null);
  const routedDestination = useRef<StudyDestination | undefined>(undefined);
  const flashcardRoute = searchParams.has('deck') || searchParams.get('newDeck') === '1';
  // The review screen is the address `?course=…&lesson=…&review=claims` .
  const reviewCourse = searchParams.get('review') === 'claims' ? searchParams.get('course') : null;
  const reviewLesson = reviewCourse ? searchParams.get('lesson') : null;
  const reviewing = Boolean(reviewCourse && reviewLesson);
  const pushedReview = useRef(false);
  useEffect(() => {
    if (!reviewCourse) return;
    setScreen('programs');
    setSelectedId(reviewCourse);
  }, [reviewCourse]);
  // Left by the browser's Back: the next leave has no entry of ours to pop.
  useEffect(() => { if (!reviewing) pushedReview.current = false; }, [reviewing]);
  const openReview = (programId: string, lessonId: string) => {
    pushedReview.current = true;
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      next.set('course', programId);
      next.set('lesson', lessonId);
      next.set('review', 'claims');
      return next;
    });
  };
  const leaveReview = () => {
    const lessonId = reviewLesson;
    if (pushedReview.current) { pushedReview.current = false; void navigate(-1); }
    else setSearchParams((current) => {
      const next = new URLSearchParams(current);
      REVIEW_PARAMS.forEach((key) => next.delete(key));
      return next;
    }, { replace: true });
    // Back on the Lessons tab, on the lesson's row.
    if (lessonId) setTimeout(() => requestAnimationFrame(() => {
      const row = document.getElementById(`lesson-${lessonId}`);
      row?.scrollIntoView({ block: 'center' });
      (row?.querySelector<HTMLElement>('button') ?? row)?.focus();
    }), 0);
  };

  useEffect(() => {
    if (!destination) return;
    setScreen('programs');
    setSelectedId(destination.programId ?? null);
  }, [destination]);
  const latestOpenReview = useRef(openReview);
  latestOpenReview.current = openReview;
  useEffect(() => {
    if (destination?.review === 'claims' && destination.programId && destination.lessonId) latestOpenReview.current(destination.programId, destination.lessonId);
  }, [destination]);
  useEffect(() => {
    // Let the current course's save guard update before moving to a deck.
    // Avoid navigating to the current URL, which would unnecessarily block.
    if (!destination || destination.review || routedDestination.current === destination || screen !== 'programs' || selectedId !== (destination.programId ?? null)) return;
    routedDestination.current = destination;
    const next = new URLSearchParams(destination.flashcards ? destination.deckId ? { deck: destination.deckId } : { section: 'flashcards' } : {});
    if (next.toString() !== searchParams.toString()) setSearchParams(next, { replace: true });
  }, [destination, screen, selectedId, searchParams, setSearchParams]);

  useEffect(() => {
    if (searchParams.get('new') !== '1') {
      handledNewRoute.current = false;
      return;
    }
    if (handledNewRoute.current) return;
    handledNewRoute.current = true;
    setScreen('builder');
    generate.reset();
    const next = new URLSearchParams(searchParams);
    next.delete('new');
    setSearchParams(next, { replace: true });
  }, [generate, searchParams, setSearchParams]);

  // `?section=flashcards` (the command palette) lands on the Flashcards section.
  useEffect(() => {
    if (searchParams.get('section') !== 'flashcards' || selectedId || screen !== 'programs') return;
    flashcardsRef.current?.scrollIntoView({ block: 'start' });
    const next = new URLSearchParams(searchParams);
    next.delete('section');
    setSearchParams(next, { replace: true });
  }, [screen, searchParams, selectedId, setSearchParams]);

  const refresh = () => { void programs.refetch(); if (selectedId) void detail.refetch(); };
  // Landing on an imported course is resuming it: the workspace opens at its current lesson.
  const openImported = (programId: string) => {
    setScreen('programs');
    setSelectedId(programId);
    void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
    void client.invalidateQueries({ queryKey: learningProgramKey(programId) });
  };
  const create = (request: Parameters<typeof generate.mutate>[0]) => generate.mutate(request, {
    onSuccess: (program) => { clearProgramBuilderDraft(); setSelectedId(program.summary.id); setScreen('programs'); },
  });

  const handleSelectDestination = (next: StudyDestination) => {
    if (onOpenDestination) {
      onOpenDestination(next);
    } else {
      setScreen('programs');
      setSelectedId(next.programId ?? null);
    }
  };

  const handleSelectFramework = (action: FrameworkAction) => {
    if (action === 'interleaving') {
      flashcardsRef.current?.scrollIntoView({ block: 'start' });
      return;
    }
    if (!programs.data?.length) return;
    const targetProgram = programs.data.find((p) => p.status === 'active') ?? programs.data[0];
    const tabMap: Record<Exclude<FrameworkAction, 'interleaving'>, StudyTab> = {
      'recall-dump': 'recall',
      'spaced-repetition': 'recall',
      feynman: 'workbench',
      'dual-coding': 'canvas',
      'handwritten-notes': 'notebook',
    };
    handleSelectDestination({ programId: targetProgram.id, tab: tabMap[action] });
  };

  const formFor = (key: string, questions: { id: string }[]): FormSnapshot => {
    const existing = forms[key];
    if (!existing) return { answers: {}, attemptId: null, fingerprint: null };
    const known = new Set(questions.map((question) => question.id));
    const answers = Object.fromEntries(Object.entries(existing.answers).filter(([id]) => known.has(id)));
    return { ...existing, answers };
  };
  const setAnswer = (key: string, questionId: string, value: number) => setForms((current) => {
    const previous = current[key] ?? { answers: {}, attemptId: null, fingerprint: null };
    if (previous.answers[questionId] === value) return current;
    return { ...current, [key]: { answers: { ...previous.answers, [questionId]: value }, attemptId: null, fingerprint: null } };
  });

  const submitAttempt = (key: string, kind: LearningAssessmentKind, module: LearningModuleDto, lesson: LearningLessonDto | null, questions: LearningQuestionDto[], form: FormSnapshot, proposedId: string) => {
    if (!selectedId || questions.length === 0) return;
    const answers = questions.map((question) => ({ questionId: question.id, selectedIndex: form.answers[question.id] }));
    if (answers.some((answer) => !Number.isInteger(answer.selectedIndex))) return;
    const fingerprint = JSON.stringify(answers);
    const attemptId = form.attemptId && form.fingerprint === fingerprint ? form.attemptId : proposedId;
    // Save the identifier before sending. If the response is lost, retrying the
    // same unchanged form reuses this UUID and the backend's idempotency key.
    setForms((current) => ({ ...current, [key]: { answers: form.answers, attemptId, fingerprint } }));
    const request: SubmitLearningAttemptRequestDto = {
      attemptId,
      programId: selectedId,
      expectedRevision: detail.data?.summary.revision ?? 0,
      moduleId: module.id,
      lessonId: kind === 'test' ? null : lesson?.id ?? null,
      kind,
      answers,
    };
    submit.mutate(request, { onSuccess: () => { setForms((current) => ({ ...current, [key]: { answers: {}, attemptId: null, fingerprint: null } })); } });
  };

  if (flashcardRoute && !selectedId && screen === 'programs') return <FlashcardDeckView />;
  if (screen === 'import' || screen === 'export-all') return <div className="h-full min-h-0 overflow-y-auto bg-bg px-4 py-4 sm:px-6 sm:py-5"><div className="mx-auto max-w-3xl"><button type="button" disabled={exportingAll} onClick={() => setScreen('programs')} className="mb-4 inline-flex min-h-10 items-center gap-2 text-sm text-text-muted hover:text-text-primary disabled:opacity-50"><ArrowLeft size={16} /> All programs</button>{screen === 'import' ? <ImportCourseFlow headingLevel={2} onImported={openImported} /> : <ExportAllCourses />}</div></div>;
  if (screen === 'builder') return <div className="h-full min-h-0 overflow-y-auto bg-bg px-5 py-7 sm:px-8"><ProgramBuilder onOpenDraft={() => { if (generate.progress?.programId) { setSelectedId(generate.progress.programId); setScreen('programs'); } }} progress={generate.progress} cancelling={generate.cancelling} onStop={() => { void generate.cancel(); }} cancelError={generate.cancelError} pending={generate.isPending} error={generate.error ? message(generate.error) : undefined} onGenerate={create} onCancel={() => setScreen('programs')} /></div>;

  const reviewTitle = reviewing ? detail.data?.modules.flatMap((module) => module.lessons).find((lesson) => lesson.id === reviewLesson)?.title ?? 'Lesson' : '';
  return <>
    {reviewing && <div className="h-full min-h-0">
      {detail.data?.summary.id === reviewCourse
        ? <ClaimReviewScreen key={`${reviewCourse}:${reviewLesson}`} programId={reviewCourse!} lessonId={reviewLesson!} lessonTitle={reviewTitle} courseRevision={detail.data.summary.revision} onBack={leaveReview} />
        : detail.isError ? <div role="alert" className="m-6 rounded-xl border border-border-default bg-danger-muted/30 p-4 text-sm text-text-primary"><p className="text-danger-fg">{message(detail.error)}</p><button type="button" onClick={leaveReview} className="mt-3 underline">Back to the course</button></div>
          : <p role="status" className="p-6 text-sm text-text-muted">Loading the review…</p>}
    </div>}
    {/* The course page stays mounted behind the review, so leaving it returns to the same tab and scroll. */}
    <div hidden={reviewing} className="h-full min-h-0 overflow-y-auto bg-bg px-4 py-4 sm:px-6 sm:py-6">
    {navigation.error && <p role="alert" className="mx-auto mb-4 max-w-[1450px] rounded-xl border border-border-default bg-danger-muted/30 p-3 text-sm text-danger-fg">{navigation.error}</p>}
    {selectedId ? (
      detail.isLoading ? <div className="mx-auto flex max-w-5xl items-center justify-center rounded-xl border border-border-default bg-surface p-16 text-sm text-text-muted">Loading your program…</div>
        : detail.isError || !detail.data ? <div className="mx-auto max-w-3xl rounded-xl border border-border-default bg-surface p-8 shadow-xs"><h2 className="font-serif text-2xl text-text-primary">This program could not be opened</h2><p className="mt-2 text-sm text-text-secondary">{message(detail.error)}</p><div className="mt-5 flex gap-3"><button type="button" onClick={refresh} className="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-accent-fg shadow-xs transition hover:brightness-95">Retry</button><button type="button" onClick={() => setSelectedId(null)} className="rounded-xl border border-border-default bg-surface px-4 py-2 text-sm font-medium text-text-primary shadow-xs transition hover:bg-surface-raised">All programs</button></div></div>
          : <ProgramWorkspace key={selectedId} destination={destination?.programId === selectedId ? destination : undefined} initialTangentId={destination?.tangentId} outlineGeneration={generate.progress?.programId === selectedId ? generate : undefined} program={detail.data} onBack={() => { setSelectedId(null); void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY }); }} onOpenProgram={openImported} onReview={openReview} onAccept={(title) => accept.mutate({ programId: selectedId, expectedRevision: detail.data!.summary.revision, title })} acceptPending={accept.isPending} acceptError={accept.error ? message(accept.error) : undefined} onPrepare={(lesson) => prepare.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} preparePending={prepare.isPending} prepareError={prepare.error ? message(prepare.error) : undefined} onComplete={(lesson) => complete.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} completePending={complete.isPending} completeError={complete.error ? message(complete.error) : undefined} formFor={formFor} setAnswer={setAnswer} onSubmitAttempt={submitAttempt} submitPending={submit.isPending} submitError={submit.error ? message(submit.error) : undefined} />
    ) : <div className="mx-auto max-w-[1450px]">
      <header className="rounded-xl border border-border-default bg-surface p-6 shadow-xs sm:p-8">
        <div className="flex flex-wrap items-start justify-between gap-6">
          <div className="max-w-2xl">
            <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[.18em] text-accent">
              <BookOpenCheck size={15} /> Learning Studio
            </div>
            <h1 className="mt-2.5 font-serif text-[28px] leading-tight text-text-primary sm:text-3xl">
              Turn your materials into a clear path you can practice
            </h1>
            <p className="mt-2.5 text-sm leading-relaxed text-text-secondary">
              Follow a complete course through in-depth lessons, practical assignments, knowledge checks, and a final project. Your own materials are optional.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2.5">
            <button
              type="button"
              onClick={() => setScreen('import')}
              className="inline-flex items-center gap-2 rounded-xl border border-border-default bg-surface px-3.5 py-2 text-sm font-medium text-text-primary shadow-xs transition hover:bg-surface-raised"
            >
              <Upload size={15} /> Import a course…
            </button>
            <button
              type="button"
              disabled={exportingAll}
              onClick={() => setScreen('export-all')}
              className="inline-flex items-center gap-2 rounded-xl border border-border-default bg-surface px-3.5 py-2 text-sm font-medium text-text-primary shadow-xs transition hover:bg-surface-raised disabled:opacity-50"
            >
              <Archive size={15} /> Export all courses…
            </button>
            <button
              type="button"
              onClick={() => { generate.reset(); setScreen('builder'); }}
              className="inline-flex items-center gap-2 rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-accent-fg shadow-xs transition hover:brightness-95"
            >
              <Plus size={16} /> New program
            </button>
          </div>
        </div>
      </header>

      <StudyHabitsBar className="mt-6" onSelectFramework={handleSelectFramework} />

      {/* 1. Programs come first */}
      <section aria-labelledby="studio-programs-heading" className="mt-8">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <div className="text-[10px] font-semibold uppercase tracking-[.16em] text-text-muted">Your learning</div>
            <h2 id="studio-programs-heading" className="mt-1 font-serif text-2xl text-text-primary">Programs</h2>
          </div>
          {programs.data?.length ? (
            <div className="text-xs text-text-muted">
              {programs.data.length} {programs.data.length === 1 ? 'program' : 'programs'}
            </div>
          ) : null}
        </div>

        {programs.isLoading ? (
          <div className="mt-5 grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {[0, 1, 2].map((n) => (
              <div key={n} className="h-52 animate-pulse rounded-xl border border-border-subtle bg-surface" />
            ))}
          </div>
        ) : programs.isError ? (
          <div role="alert" className="mt-5 flex flex-wrap items-center justify-between gap-4 rounded-xl border border-border-default bg-surface p-6 shadow-xs">
            <div>
              <h3 className="font-serif text-xl text-text-primary">Programs are unavailable</h3>
              <p className="mt-1 text-sm text-text-secondary">{message(programs.error)}</p>
            </div>
            <button
              type="button"
              onClick={refresh}
              className="rounded-xl border border-border-default bg-surface px-4 py-2 text-sm font-medium text-text-primary shadow-xs transition hover:bg-surface-raised"
            >
              Retry
            </button>
          </div>
        ) : programs.data?.length ? (
          <div className="mt-5 grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {programs.data.map((program) => {
              const percent = program.lessonCount ? Math.round((program.completedLessons / program.lessonCount) * 100) : 0;
              const active = program.status === 'active';
              return (
                <button
                  type="button"
                  key={program.id}
                  onClick={() => {
                    setSelectedId(program.id);
                    void client.invalidateQueries({ queryKey: learningProgramKey(program.id) });
                  }}
                  className="group flex flex-col justify-between rounded-xl border border-border-default bg-surface p-5 text-left shadow-xs transition-colors hover:border-border-strong hover:bg-surface-raised/40 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
                >
                  <div>
                    <div className="flex items-center justify-between gap-2">
                      <span
                        className={`rounded-full px-2.5 py-0.5 text-[10px] font-semibold uppercase tracking-[.13em] ${
                          active ? 'bg-accent-muted text-accent' : 'bg-warning-muted text-warning-fg'
                        }`}
                      >
                        {active ? 'Active' : 'Draft outline'}
                      </span>
                      <span className="text-xs text-text-muted">
                        {program.moduleCount} {program.moduleCount === 1 ? 'module' : 'modules'} · {program.lessonCount} {program.lessonCount === 1 ? 'lesson' : 'lessons'}
                      </span>
                    </div>
                    <h3 className="mt-4 font-serif text-xl font-medium leading-snug text-text-primary">
                      {program.title}
                    </h3>
                    <p className="mt-2 line-clamp-2 text-sm leading-relaxed text-text-secondary">
                      {program.goal}
                    </p>
                  </div>
                  <div className="mt-6 border-t border-border-subtle pt-4">
                    <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface-sunken">
                      <div
                        className="h-full rounded-full bg-accent transition-all"
                        style={{ width: `${active ? percent : 0}%` }}
                      />
                    </div>
                    <div className="mt-3 flex items-center justify-between text-xs">
                      <span className="text-text-muted">
                        {active ? `${program.completedLessons}/${program.lessonCount} completed` : 'Review outline'}
                      </span>
                      <span className="inline-flex items-center gap-1 font-medium text-accent group-hover:underline">
                        Continue <ArrowRight size={14} className="transition group-hover:translate-x-0.5" />
                      </span>
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        ) : (
          <div className="mt-5 grid gap-5 rounded-xl border border-border-default bg-surface p-8 shadow-xs sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center sm:p-10">
            <div>
              <div className="grid h-10 w-10 place-items-center rounded-lg border border-border-subtle bg-surface-sunken text-accent">
                <Sparkles size={18} />
              </div>
              <h3 className="mt-4 font-serif text-2xl text-text-primary">Start with a question you care about.</h3>
              <p className="mt-2 max-w-xl text-sm leading-relaxed text-text-secondary">
                Your first program starts with your goal and experience. Add your own materials if you have them. You will review its outline before Studio prepares a lesson.
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2.5 sm:flex-col sm:items-stretch">
              <button
                type="button"
                onClick={() => setScreen('builder')}
                className="inline-flex items-center justify-center gap-2 rounded-xl bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg shadow-xs transition hover:brightness-95"
              >
                Build your first program <ArrowRight size={15} />
              </button>
              <button
                type="button"
                onClick={() => setScreen('import')}
                className="inline-flex items-center justify-center gap-2 rounded-xl border border-border-default bg-surface px-5 py-2.5 text-sm font-medium text-text-primary shadow-xs transition hover:bg-surface-raised"
              >
                <Upload size={15} /> Import a course
              </button>
            </div>
          </div>
        )}
      </section>

      {/* 2. Flashcards */}
      <FlashcardsSection ref={flashcardsRef} />

      {/* 3. Course Tangents below Courses & Flashcards */}
      <CourseTangentsSection onOpenDestination={handleSelectDestination} />

      {/* Hairline minimalist footer */}
      <footer className="mt-12 flex flex-wrap items-center justify-between gap-4 border-t border-border-subtle pt-4 text-xs text-text-muted">
        <span>Programs and study data stay on this device</span>
        <span>Local learning workspace · Private to your vault</span>
      </footer>
    </div>}
    </div>
  </>;
}
