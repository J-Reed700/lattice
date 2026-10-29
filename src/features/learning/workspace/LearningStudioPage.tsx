import { useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeft } from 'lucide-react';
import { useSearchParams } from 'react-router';


import { ProgramBuilder } from '@/features/learning/curriculum/ProgramBuilder';
import { clearProgramBuilderDraft } from '@/features/learning/curriculum/programBuilderDraft';
import { ProgramWorkspace } from '@/features/learning/curriculum/ProgramWorkspace';
import type { FrameworkAction } from '@/features/learning/habits';
import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { ExportAllCourses } from '@/features/learning/portability/ExportAllCourses';
import { ImportCourseFlow } from '@/features/learning/portability/ImportCourseFlow';
import { useExportAllPending } from '@/features/learning/portability/useLearningPortability';
import { FlashcardDeckView } from '@/features/learning/recall/flashcards/FlashcardDeckView';
import { ProgramLibrary } from '@/features/learning/workspace/ProgramLibrary';
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
} from '@/features/learning/workspace/useLearningStudio';
import { useStudioNavigationGuard } from '@/features/learning/workspace/useStudioNavigationGuard';
import { useStopSpeechOnChange } from '@/features/speech/hooks/useSpeech';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { flushPendingSaves } from '@/lib/pendingSaves';
import type { StudyDestination, StudyTab } from '@/lib/studyActivity';

import './learningStudio.css';

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
  return <div className="learning-studio flex h-full min-h-0 flex-col">
    <StudyActivityPanel onOpen={next => void openTask(next)} />
    {navigationError && <p role="alert" className="px-6 py-3 text-sm text-danger-fg">Could not save before opening this task: {navigationError}</p>}
    {heldForExport && exportingAll && <p role="status" className="px-6 py-3 text-sm text-text-secondary">Your courses are still being exported. Open the task again when the export finishes.</p>}
    <div className="min-h-0 flex-1"><LearningStudioContent destination={destination} exportingAll={exportingAll} onOpenDestination={next => void openTask(next)} /></div>
  </div>;
}

const REVIEW_PARAMS = ['course', 'lesson', 'review'] as const;

function LearningStudioContent({ destination, exportingAll, onOpenDestination }: { destination?: StudyDestination; exportingAll: boolean; onOpenDestination?: (destination: StudyDestination) => void }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useAppNavigate();
  const client = useQueryClient();
  const [screen, setScreen] = useState<Screen>('programs');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  useStopSpeechOnChange(`${screen}:${selectedId}`);
  const navigation = useStudioNavigationGuard(selectedId !== null);
  const programs = useLearningPrograms();
  const detail = useLearningProgram(selectedId);
  const generate = useGenerateLearningProgram();
  const accept = useAcceptLearningProgram();
  const prepare = usePrepareLearningLesson();
  const complete = useCompleteLearningLesson();
  const handledNewRoute = useRef(false);
  const flashcardsRef = useRef<HTMLElement>(null);
  const routedDestination = useRef<StudyDestination | undefined>(undefined);
  const flashcardRoute = searchParams.has('deck') || searchParams.get('newDeck') === '1';
  // A deck opened from inside a program, such as an interview's drill deck,
  // leaves the program for the deck.
  const requestedDeck = searchParams.get('deck');
  useEffect(() => {
    if (!requestedDeck) return;
    setScreen('programs');
    setSelectedId(null);
  }, [requestedDeck]);
  // The review screen is the address `?course=…&lesson=…&review=claims` (§2.3).
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

  if (flashcardRoute && !selectedId && screen === 'programs') return <FlashcardDeckView />;
  if (screen === 'import' || screen === 'export-all') return <div className="studio-page-scroll h-full min-h-0 overflow-y-auto bg-background"><div className="mx-auto max-w-3xl"><button type="button" disabled={exportingAll} onClick={() => setScreen('programs')} className="mb-2 inline-flex min-h-10 items-center gap-2 text-sm text-text-muted hover:text-text-primary disabled:opacity-50"><ArrowLeft size={16} /> All programs</button>{screen === 'import' ? <ImportCourseFlow headingLevel={2} onImported={openImported} /> : <ExportAllCourses />}</div></div>;
  if (screen === 'builder') return <div className="h-full min-h-0 overflow-y-auto bg-background px-5 py-7 sm:px-8"><ProgramBuilder onOpenDraft={() => { if (generate.progress?.programId) { setSelectedId(generate.progress.programId); setScreen('programs'); } }} progress={generate.progress} cancelling={generate.cancelling} onStop={() => { void generate.cancel(); }} cancelError={generate.cancelError} pending={generate.isPending} error={generate.error ? message(generate.error) : undefined} onGenerate={create} onCancel={() => setScreen('programs')} /></div>;

  const reviewTitle = reviewing ? detail.data?.modules.flatMap((module) => module.lessons).find((lesson) => lesson.id === reviewLesson)?.title ?? 'Lesson' : '';
  return <>
    {reviewing && <div className="h-full min-h-0">
      {detail.data?.summary.id === reviewCourse
        ? <ClaimReviewScreen key={`${reviewCourse}:${reviewLesson}`} programId={reviewCourse!} lessonId={reviewLesson!} lessonTitle={reviewTitle} courseRevision={detail.data.summary.revision} onBack={leaveReview} />
        : detail.isError ? <div role="alert" className="m-6 rounded-xl bg-danger-muted p-4 text-sm text-text-primary"><p>{message(detail.error)}</p><button type="button" onClick={leaveReview} className="mt-3 underline">Back to the course</button></div>
          : <p role="status" className="p-6 text-sm text-text-muted">Loading the review…</p>}
    </div>}
    {/* The course page stays mounted behind the review, so leaving it returns to the same tab and scroll. */}
    <div hidden={reviewing} className="studio-page-scroll h-full min-h-0 overflow-y-auto bg-background">
    {navigation.error && <p role="alert" className="mx-auto mb-4 max-w-[1500px] rounded-xl border border-rose-500/25 bg-rose-500/5 p-3 text-sm text-text-primary">{navigation.error}</p>}
    {selectedId ? (
      detail.isLoading ? <div className="mx-auto flex max-w-5xl items-center justify-center rounded-2xl border border-border bg-surface p-16 text-sm text-text-muted">Loading your program…</div>
        : detail.isError || !detail.data ? <div className="mx-auto max-w-3xl rounded-2xl border border-rose-500/25 bg-rose-500/5 p-8"><h2 className="font-serif text-2xl text-text-primary">This program could not be opened</h2><p className="mt-2 text-sm text-text-secondary">{message(detail.error)}</p><div className="mt-5 flex gap-3"><button type="button" onClick={refresh} className="rounded-full bg-accent px-4 py-2 text-sm font-medium text-accent-fg">Retry</button><button type="button" onClick={() => setSelectedId(null)} className="rounded-full border border-border px-4 py-2 text-sm">All programs</button></div></div>
          : <ProgramWorkspace key={selectedId} destination={destination?.programId === selectedId ? destination : undefined} initialTangentId={destination?.tangentId} outlineGeneration={generate.progress?.programId === selectedId ? generate : undefined} program={detail.data} onBack={() => { setSelectedId(null); void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY }); }} onOpenProgram={openImported} onReview={openReview} onAccept={(title) => accept.mutate({ programId: selectedId, expectedRevision: detail.data!.summary.revision, title })} acceptPending={accept.isPending} acceptError={accept.error ? message(accept.error) : undefined} onPrepare={(lesson) => prepare.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} preparePending={prepare.isPending} prepareError={prepare.error ? message(prepare.error) : undefined} onComplete={(lesson) => complete.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} completePending={complete.isPending} completeError={complete.error ? message(complete.error) : undefined} />
    ) : <ProgramLibrary
      programs={programs.data}
      loading={programs.isLoading}
      error={programs.isError ? message(programs.error) : undefined}
      onRetry={refresh}
      onOpen={(id) => { setSelectedId(id); void client.invalidateQueries({ queryKey: learningProgramKey(id) }); }}
      onBuild={() => { generate.reset(); setScreen('builder'); }}
      onImport={() => setScreen('import')}
      onExport={() => setScreen('export-all')}
      onSelectFramework={handleSelectFramework}
      onOpenDestination={handleSelectDestination}
      flashcardsRef={flashcardsRef}
    />}
    </div>
  </>;
}
