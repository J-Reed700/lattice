import { useEffect, useLayoutEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeft } from 'lucide-react';
import { useSearchParams } from 'react-router';


import { LearningModelNotice } from '@/features/learning/curriculum/LearningModelNotice';
import { ProgramBuilder } from '@/features/learning/curriculum/ProgramBuilder';
import { clearProgramBuilderDraft } from '@/features/learning/curriculum/programBuilderDraft';
import { ProgramWorkspace } from '@/features/learning/curriculum/ProgramWorkspace';
import type { FrameworkAction } from '@/features/learning/habits';
import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { ExportAllCourses } from '@/features/learning/portability/ExportAllCourses';
import { ImportCourseFlow } from '@/features/learning/portability/ImportCourseFlow';
import { useExportAllPending } from '@/features/learning/portability/useLearningPortability';
import { FlashcardDeckView } from '@/features/learning/recall/flashcards/FlashcardDeckView';
import { readLastCourse, writeLastCourse } from '@/features/learning/workspace/lastCourse';
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
  usePrepareLearningAhead,
  usePrepareLearningLesson,
  usePrepareLearningModule,
} from '@/features/learning/workspace/useLearningStudio';
import { useStudioNavigationGuard } from '@/features/learning/workspace/useStudioNavigationGuard';
import { ReaderPane } from '@/features/reading/components/ReaderPane';
import { useReaderRoom } from '@/features/reading/hooks/useReaderRoom';
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

const REVIEW_PARAMS = ['course', 'lesson', 'review', 'claim'] as const;
const DEEP_LINK_PARAMS = ['course', 'lesson', 'review', 'deck', 'newDeck', 'section', 'drills', 'new', 'builder'] as const;

type ProgramMutationState = { error: Error | null; variables: { programId: string } | undefined; submittedAt: number };

const mutationErrorFor = (programId: string, mutation: ProgramMutationState) => (mutation.variables?.programId === programId ? mutation.error : null);

function latestErrorFor(programId: string, mutations: ProgramMutationState[]): Error | undefined {
  const latest = mutations.reduce<ProgramMutationState | undefined>((best, mutation) => (mutation.variables && (!best || mutation.submittedAt > best.submittedAt) ? mutation : best), undefined);
  return (latest && mutationErrorFor(programId, latest)) || undefined;
}

function LearningStudioContent({ destination, exportingAll, onOpenDestination }: { destination?: StudyDestination; exportingAll: boolean; onOpenDestination?: (destination: StudyDestination) => void }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useAppNavigate();
  const client = useQueryClient();
  const generate = useGenerateLearningProgram();
  const {
    isPending: generatePending,
    error: generateError,
    builderFocused: generateBuilderFocused,
    hasLiveChannel: generateHasLiveChannel,
    finishedProgramId,
    reset: resetGenerate,
    setBuilderFocused,
    clearFinished,
    dismissError,
  } = generate;
  const hasDeepLink = Boolean(destination) || DEEP_LINK_PARAMS.some((param) => searchParams.has(param));
  const [screen, setScreen] = useState<Screen>(() => (
    !hasDeepLink && (generatePending || Boolean(generateError)) && generateBuilderFocused ? 'builder' : 'programs'
  ));
  const [selectedId, setSelectedId] = useState<string | null>(null);
  useStopSpeechOnChange(`${screen}:${selectedId}`);
  const navigation = useStudioNavigationGuard(selectedId !== null);
  const programs = useLearningPrograms();
  const detail = useLearningProgram(selectedId);
  const accept = useAcceptLearningProgram();
  const prepare = usePrepareLearningLesson();
  const prepareModule = usePrepareLearningModule();
  const prepareAhead = usePrepareLearningAhead();
  const complete = useCompleteLearningLesson();
  const handledNewRoute = useRef(false);
  const handledBuilderRoute = useRef(false);
  const userNavigatedRef = useRef(false);
  const flashcardsRef = useRef<HTMLElement>(null);
  const rowRef = useRef<HTMLDivElement>(null);
  const routedDestination = useRef<StudyDestination | undefined>(undefined);
  const flashcardRoute = searchParams.has('deck') || searchParams.get('newDeck') === '1';
  const room = useReaderRoom(rowRef, undefined, [screen, flashcardRoute]);
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
  const reviewClaim = reviewLesson ? searchParams.get('claim') : null;
  const reviewing = Boolean(reviewCourse && reviewLesson);
  const pushedReview = useRef(false);
  useEffect(() => {
    if (!reviewCourse) return;
    setScreen('programs');
    setSelectedId(reviewCourse);
  }, [reviewCourse]);
  // Left by the browser's Back: the next leave has no entry of ours to pop.
  useEffect(() => { if (!reviewing) pushedReview.current = false; }, [reviewing]);
  const openReview = (programId: string, lessonId: string, claimId?: string) => {
    pushedReview.current = true;
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      next.set('course', programId);
      next.set('lesson', lessonId);
      next.set('review', 'claims');
      if (claimId) next.set('claim', claimId); else next.delete('claim');
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

  const shouldAutoOpenBuilder =
    !userNavigatedRef.current
    && !hasDeepLink
    && screen === 'programs'
    && selectedId === null
    && !flashcardRoute
    && !reviewing
    && (generatePending || Boolean(generateError))
    && generateBuilderFocused
    && !generateHasLiveChannel;

  const activeScreen: Screen = shouldAutoOpenBuilder ? 'builder' : screen;

  useEffect(() => {
    if (shouldAutoOpenBuilder) {
      setScreen('builder');
    }
  }, [shouldAutoOpenBuilder]);

  useEffect(() => {
    if (searchParams.get('builder') !== '1') {
      handledBuilderRoute.current = false;
      return;
    }
    if (handledBuilderRoute.current) return;
    handledBuilderRoute.current = true;
    setSelectedId(null);
    setScreen('builder');
    setBuilderFocused(true);
    const next = new URLSearchParams(searchParams);
    next.delete('builder');
    setSearchParams(next, { replace: true });
  }, [searchParams, setBuilderFocused, setSearchParams]);

  useEffect(() => {
    if (searchParams.get('new') !== '1') {
      handledNewRoute.current = false;
      return;
    }
    if (handledNewRoute.current) return;
    handledNewRoute.current = true;
    setSelectedId(null);
    setScreen('builder');
    if (!generatePending) resetGenerate();
    setBuilderFocused(true);
    const next = new URLSearchParams(searchParams);
    next.delete('new');
    setSearchParams(next, { replace: true });
  }, [generatePending, resetGenerate, searchParams, setBuilderFocused, setSearchParams]);

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
  const create = (request: Parameters<typeof generate.mutate>[0]) => generate.mutate(request);
  useLayoutEffect(() => {
    if (!finishedProgramId) return;
    clearProgramBuilderDraft();
    clearFinished();
    if (activeScreen === 'builder') { setSelectedId(finishedProgramId); setScreen('programs'); }
  }, [activeScreen, clearFinished, finishedProgramId]);

  const handleSelectDestination = (next: StudyDestination) => {
    userNavigatedRef.current = true;
    if (onOpenDestination) {
      onOpenDestination(next);
    } else {
      setScreen('programs');
      setSelectedId(next.programId ?? null);
    }
  };

  const latestSelectDestination = useRef(handleSelectDestination);
  latestSelectDestination.current = handleSelectDestination;
  useEffect(() => {
    if (selectedId) writeLastCourse(selectedId);
  }, [selectedId]);
  // `?drills=open` (the command palette) opens the review drills of the course opened last, else of the first active one.
  const drillsLink = searchParams.get('drills') === 'open';
  const programList = programs.data;
  useEffect(() => {
    if (!drillsLink || !programList) return;
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      next.delete('drills');
      return next;
    }, { replace: true });
    const active = programList.filter((item) => item.status === 'active');
    const last = readLastCourse();
    const target = active.find((item) => item.id === last) ?? active[0];
    if (target) latestSelectDestination.current({ programId: target.id, tab: 'drills' });
  }, [drillsLink, programList, setSearchParams]);

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

  if (flashcardRoute && !selectedId && activeScreen === 'programs') return <FlashcardDeckView />;
  if (activeScreen === 'import' || activeScreen === 'export-all') return <div className="studio-page-scroll h-full min-h-0 overflow-y-auto bg-background"><div className="mx-auto max-w-3xl"><button type="button" disabled={exportingAll} onClick={() => setScreen('programs')} className="mb-2 inline-flex min-h-10 items-center gap-2 text-sm text-text-muted hover:text-text-primary disabled:opacity-50"><ArrowLeft size={16} /> All programs</button>{activeScreen === 'import' ? <ImportCourseFlow headingLevel={2} onImported={openImported} /> : <ExportAllCourses />}</div></div>;
  if (activeScreen === 'builder') return <div className="h-full min-h-0 overflow-y-auto bg-background px-5 py-7 sm:px-8"><LearningModelNotice /><ProgramBuilder activeDepth={generate.courseDepth} onOpenDraft={() => { if (generate.progress?.programId) { userNavigatedRef.current = true; setBuilderFocused(false); setSelectedId(generate.progress.programId); setScreen('programs'); } }} progress={generate.progress} cancelling={generate.cancelling} onStop={() => { void generate.cancel(); }} cancelError={generate.cancelError} pending={generate.isPending} error={generate.error ?? undefined} onDismissError={dismissError} onGenerate={create} onCancel={() => { userNavigatedRef.current = true; setBuilderFocused(false); setScreen('programs'); }} /></div>;

  const reviewTitle = reviewing ? detail.data?.modules.flatMap((module) => module.lessons).find((lesson) => lesson.id === reviewLesson)?.title ?? 'Lesson' : '';
  return <>
    {reviewing && <div className="h-full min-h-0">
      {detail.data?.summary.id === reviewCourse
        ? <ClaimReviewScreen key={`${reviewCourse}:${reviewLesson}:${reviewClaim}`} programId={reviewCourse!} lessonId={reviewLesson!} focusClaimId={reviewClaim} lessonTitle={reviewTitle} courseRevision={detail.data.summary.revision} onBack={leaveReview} />
        : detail.isError ? <div role="alert" className="m-6 rounded-xl bg-danger-muted p-4 text-sm text-text-primary"><p>{message(detail.error)}</p><button type="button" onClick={leaveReview} className="mt-3 underline">Back to the course</button></div>
          : <p role="status" className="p-6 text-sm text-text-muted">Loading the review…</p>}
    </div>}
    {/* The course page stays mounted behind the review, so leaving it returns to the same tab and scroll. */}
    <div ref={rowRef} hidden={reviewing} className="flex h-full min-h-0 w-full min-w-0 overflow-hidden">
    <div className="studio-page-scroll h-full min-h-0 min-w-0 flex-1 overflow-y-auto bg-background">
    {!selectedId && <LearningModelNotice />}
    {navigation.error && <p role="alert" className="mx-auto mb-4 max-w-[1500px] rounded-xl border border-rose-500/25 bg-rose-500/5 p-3 text-sm text-text-primary">{navigation.error}</p>}
    {selectedId ? (
      detail.isLoading ? <div className="mx-auto flex max-w-5xl items-center justify-center rounded-2xl border border-border bg-surface p-16 text-sm text-text-muted">Loading your program…</div>
        : detail.isError || !detail.data ? <div className="mx-auto max-w-3xl rounded-2xl border border-rose-500/25 bg-rose-500/5 p-8"><h2 className="font-serif text-2xl text-text-primary">This program could not be opened</h2><p className="mt-2 text-sm text-text-secondary">{message(detail.error)}</p><div className="mt-5 flex gap-3"><button type="button" onClick={refresh} className="rounded-full bg-accent px-4 py-2 text-sm font-medium text-accent-fg">Retry</button><button type="button" onClick={() => setSelectedId(null)} className="rounded-full border border-border px-4 py-2 text-sm">All programs</button></div></div>
          : <ProgramWorkspace key={selectedId} destination={destination?.programId === selectedId ? destination : undefined} initialTangentId={destination?.tangentId} outlineGeneration={generate.progress?.programId === selectedId ? generate : undefined} program={detail.data} onBack={() => { userNavigatedRef.current = true; setSelectedId(null); void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY }); }} onOpenProgram={openImported} onReview={openReview} onAccept={(title) => accept.mutate({ programId: selectedId, expectedRevision: detail.data!.summary.revision, title }, { onSuccess: (accepted) => { const first = accepted.modules[0]; if (first) prepareModule.mutate({ programId: accepted.summary.id, moduleId: first.id, expectedRevision: accepted.summary.revision }); } })} acceptPending={accept.isPending} acceptError={mutationErrorFor(selectedId, accept) ? message(accept.error) : undefined} onPrepare={(lesson) => prepare.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} onPrepareModule={(module) => prepareModule.mutate({ programId: selectedId, moduleId: module.id, expectedRevision: detail.data!.summary.revision })} onLessonOpened={(lessonId) => prepareAhead.mutate({ programId: selectedId, lessonId })} preparePending={prepare.isPending || prepareModule.isPending} prepareError={latestErrorFor(selectedId, [prepare, prepareModule, prepareAhead])} onComplete={(lesson) => complete.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} completePending={complete.isPending} completeError={complete.error ?? undefined} />
    ) : <ProgramLibrary
      programs={programs.data}
      loading={programs.isLoading}
      error={programs.isError ? message(programs.error) : undefined}
      onRetry={refresh}
      onOpen={(id) => { userNavigatedRef.current = true; setSelectedId(id); void client.invalidateQueries({ queryKey: learningProgramKey(id) }); }}
      onBuild={() => { userNavigatedRef.current = true; setBuilderFocused(true); setScreen('builder'); }}
      onImport={() => { userNavigatedRef.current = true; setScreen('import'); }}
      onExport={() => { userNavigatedRef.current = true; setScreen('export-all'); }}
      onSelectFramework={handleSelectFramework}
      onOpenDestination={handleSelectDestination}
      flashcardsRef={flashcardsRef}
      outlineGeneration={{
        isPending: generate.isPending,
        progress: generate.progress,
        goal: generate.goal,
        cancelling: generate.cancelling,
        cancelError: generate.cancelError,
        error: generate.error,
        onOpenBuilder: () => { userNavigatedRef.current = true; setBuilderFocused(true); setScreen('builder'); },
        onOpenDraft: () => {
          if (generate.progress?.programId) {
            userNavigatedRef.current = true;
            setBuilderFocused(false);
            setSelectedId(generate.progress.programId);
            setScreen('programs');
          }
        },
        onCancel: () => { void generate.cancel(); },
        onDismissError: dismissError,
      }}
    />}
    </div>
    <ReaderPane rowWidth={room.width} availableWidth={room.available} resetKey={`${selectedId ?? ''}:${reviewing}`} />
    </div>
  </>;
}
