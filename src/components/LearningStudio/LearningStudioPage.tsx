import { useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ArrowRight, BookOpenCheck, Clock3, Plus, Sparkles } from 'lucide-react';
import { useSearchParams } from 'react-router';

import type { LearningAssessmentKind, LearningLessonDto, LearningModuleDto, LearningQuestionDto, SubmitLearningAttemptRequestDto } from '@/lib/bindings';

import { type FormSnapshot } from './AssessmentPanel';
import { FlashcardDeckView } from './flashcards/FlashcardDeckView';
import { FlashcardsSection } from './flashcards/FlashcardsSection';
import { ProgramBuilder } from './ProgramBuilder';
import { ProgramWorkspace } from './ProgramWorkspace';
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
} from './useLearningStudio';
import { useStudioNavigationGuard } from './useStudioNavigationGuard';

type Screen = 'programs' | 'builder';

function message(error: unknown) { return error instanceof Error ? error.message : String(error); }

export function LearningStudioPage() {
  const [searchParams, setSearchParams] = useSearchParams();
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
  const flashcardRoute = searchParams.has('deck') || searchParams.get('newDeck') === '1';

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
  const create = (request: Parameters<typeof generate.mutate>[0]) => generate.mutate(request, {
    onSuccess: (program) => { setSelectedId(program.summary.id); setScreen('programs'); },
  });

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
  if (screen === 'builder') return <div className="h-full min-h-0 overflow-y-auto bg-background px-5 py-7 sm:px-8"><ProgramBuilder pending={generate.isPending} error={generate.error ? message(generate.error) : undefined} onGenerate={create} onCancel={() => setScreen('programs')} /></div>;

  return <div className="h-full min-h-0 overflow-y-auto bg-background px-4 py-4 sm:px-6 sm:py-5">
    {navigation.error && <p role="alert" className="mx-auto mb-4 max-w-[1500px] rounded-xl border border-rose-500/25 bg-rose-500/5 p-3 text-sm text-text-primary">{navigation.error}</p>}
    {selectedId ? (
      detail.isLoading ? <div className="mx-auto flex max-w-5xl items-center justify-center rounded-2xl border border-border bg-surface p-16 text-sm text-text-muted">Loading your program…</div>
        : detail.isError || !detail.data ? <div className="mx-auto max-w-3xl rounded-2xl border border-rose-500/25 bg-rose-500/5 p-8"><h2 className="font-serif text-2xl text-text-primary">This program could not be opened</h2><p className="mt-2 text-sm text-text-secondary">{message(detail.error)}</p><div className="mt-5 flex gap-3"><button type="button" onClick={refresh} className="rounded-full bg-accent px-4 py-2 text-sm font-medium text-accent-fg">Retry</button><button type="button" onClick={() => setSelectedId(null)} className="rounded-full border border-border px-4 py-2 text-sm">All programs</button></div></div>
          : <ProgramWorkspace program={detail.data} onBack={() => { setSelectedId(null); void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY }); }} onAccept={(title) => accept.mutate({ programId: selectedId, expectedRevision: detail.data!.summary.revision, title })} acceptPending={accept.isPending} acceptError={accept.error ? message(accept.error) : undefined} onPrepare={(lesson) => prepare.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} preparePending={prepare.isPending} prepareError={prepare.error ? message(prepare.error) : undefined} onComplete={(lesson) => complete.mutate({ programId: selectedId, lessonId: lesson.id, expectedRevision: detail.data!.summary.revision })} completePending={complete.isPending} completeError={complete.error ? message(complete.error) : undefined} formFor={formFor} setAnswer={setAnswer} onSubmitAttempt={submitAttempt} submitPending={submit.isPending} submitError={submit.error ? message(submit.error) : undefined} />
    ) : <div className="mx-auto max-w-[1450px]">
      <header className="relative overflow-hidden rounded-[1.6rem] border border-[#d7c9b8] bg-[#eee8df] px-7 py-8 sm:px-10 sm:py-10 dark:border-white/10 dark:bg-[#28251f]"><div className="pointer-events-none absolute -right-12 -top-20 h-64 w-64 rounded-full border border-accent/20" /><div className="relative flex flex-wrap items-end justify-between gap-6"><div className="max-w-2xl"><div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[.18em] text-accent"><BookOpenCheck size={15} /> Learning Studio</div><h1 className="mt-3 font-serif text-4xl leading-tight text-text-primary sm:text-[3.2rem]">A course shaped around your curiosity.</h1><p className="mt-3 max-w-xl text-sm leading-6 text-text-secondary">Bring a goal and materials you trust. Build an outline, prepare one lesson at a time, and keep your work and results together.</p></div><button type="button" onClick={() => { generate.reset(); setScreen('builder'); }} className="inline-flex items-center gap-2 rounded-full bg-accent px-5 py-3 text-sm font-semibold text-accent-fg shadow-sm transition hover:brightness-95"><Plus size={16} /> Build a program</button></div></header>
      <div className="mt-8 flex flex-wrap items-end justify-between gap-4"><div><div className="text-[10px] font-semibold uppercase tracking-[.16em] text-text-muted">Your learning</div><h2 className="mt-1 font-serif text-2xl text-text-primary">Programs</h2></div>{programs.data?.length ? <div className="text-xs text-text-muted">{programs.data.length} {programs.data.length === 1 ? 'program' : 'programs'}</div> : null}</div>
      {programs.isLoading ? <div className="mt-5 grid gap-4 md:grid-cols-2 xl:grid-cols-3">{[0, 1, 2].map((n) => <div key={n} className="h-52 animate-pulse rounded-2xl bg-surface" />)}</div>
        : programs.isError ? <div role="alert" className="mt-5 flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-rose-500/25 bg-surface p-6"><div><h3 className="font-serif text-xl text-text-primary">Programs are unavailable</h3><p className="mt-1 text-sm text-text-secondary">{message(programs.error)}</p></div><button type="button" onClick={refresh} className="rounded-full border border-border px-4 py-2 text-sm">Retry</button></div>
          : programs.data?.length ? <div className="mt-5 grid gap-4 md:grid-cols-2 xl:grid-cols-3">{programs.data.map((program) => {
            const percent = program.lessonCount ? Math.round(program.completedLessons / program.lessonCount * 100) : 0;
            const active = program.status === 'active';
            const created = new Date(program.createdAt);
            const createdLabel = Number.isNaN(created.getTime()) ? 'date unavailable' : created.toLocaleDateString();
            return <button type="button" key={program.id} onClick={() => { setSelectedId(program.id); void client.invalidateQueries({ queryKey: learningProgramKey(program.id) }); }} className="group relative flex min-h-56 flex-col overflow-hidden rounded-2xl border border-border/70 bg-surface p-6 text-left shadow-sm transition hover:-translate-y-0.5 hover:border-accent/40 hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"><div className="absolute right-0 top-0 h-28 w-28 rounded-bl-[5rem] bg-[#eee8df]/80 transition group-hover:bg-accent/10 dark:bg-white/5" /><div className="relative flex items-center justify-between"><span className={`rounded-full px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[.13em] ${active ? 'bg-emerald-500/10 text-emerald-700' : 'bg-amber-500/10 text-amber-700'}`}>{active ? 'Active' : 'Draft outline'}</span><ArrowRight size={16} className="text-text-muted transition group-hover:translate-x-1 group-hover:text-accent" /></div><h3 className="relative mt-5 line-clamp-2 font-serif text-2xl leading-tight text-text-primary">{program.title}</h3><p className="relative mt-2 line-clamp-2 text-sm leading-5 text-text-secondary">{program.goal}</p><div className="relative mt-auto pt-6"><div className="flex items-center justify-between text-[11px] text-text-muted"><span className="flex items-center gap-1.5"><Clock3 size={13} /> {program.moduleCount} {program.moduleCount === 1 ? 'module' : 'modules'} · {program.lessonCount} {program.lessonCount === 1 ? 'lesson' : 'lessons'}</span><span>{active ? `${program.completedLessons} complete` : 'Review outline'}</span></div><div className="mt-2 h-1 overflow-hidden rounded-full bg-background"><div className="h-full rounded-full bg-accent" style={{ width: `${active ? percent : 0}%` }} /></div><div className="mt-2 flex items-center gap-1 text-[10px] text-text-muted">{active ? 'Self-reported lesson completion' : 'No progress recorded'} · created {createdLabel}</div></div></button>;
          })}</div> : <div className="mt-5 grid gap-5 rounded-2xl border border-dashed border-border bg-surface p-8 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center sm:p-10"><div><div className="grid h-11 w-11 place-items-center rounded-full bg-accent/10 text-accent"><Sparkles size={19} /></div><h3 className="mt-5 font-serif text-2xl text-text-primary">Start with a question you care about.</h3><p className="mt-2 max-w-xl text-sm leading-6 text-text-secondary">Your first program brings together a goal, your starting point, and source material. You will review its outline before Studio prepares a lesson.</p></div><button type="button" onClick={() => setScreen('builder')} className="inline-flex items-center justify-center gap-2 rounded-full bg-accent px-5 py-3 text-sm font-semibold text-accent-fg">Build your first program <ArrowRight size={15} /></button></div>}
      <FlashcardsSection ref={flashcardsRef} />
      <div className="mt-8 flex flex-wrap items-center gap-2 text-[10px] text-text-muted"><span className="rounded-full border border-border px-2.5 py-1">Source-backed</span><span className="rounded-full border border-border px-2.5 py-1">Review outline before lessons</span><span className="rounded-full border border-border px-2.5 py-1">Progress from persisted work</span><span className="ml-auto">Programs stay on this device</span></div>
    </div>}
  </div>;
}
