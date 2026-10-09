import { useState } from 'react';

import { ArrowLeft, ArrowRight, Check, CircleHelp, RotateCcw } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import type { LearningAssessmentKind, LearningAttemptDto, LearningQuestionDto, LearningSourceDto } from '@/lib/bindings';
import { useCitationDisplayStore } from '@/stores/citationDisplayStore';

export type FormSnapshot = { answers: Record<string, number>; attemptId: string | null; fingerprint: string | null };

function newId() {
  return globalThis.crypto?.randomUUID?.() ?? `attempt-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function formatSubmitted(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'date unavailable' : date.toLocaleDateString();
}

export function AssessmentPanel({ kind, questions, sources, attempt, pending, error, onAnswer, onSubmit, onReview }: {
  kind: LearningAssessmentKind; questions: LearningQuestionDto[]; sources: LearningSourceDto[];
  attempt: FormSnapshot; pending: boolean; error?: string; onAnswer: (id: string, value: number) => void;
  onSubmit: (id: string) => void; onReview: () => void;
}) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const [index, setIndex] = useState(0);
  const answered = questions.filter((q) => Number.isInteger(attempt.answers[q.id])).length;
  const current = questions[index];
  if (!questions.length) return <div className="rounded-2xl border border-dashed border-border bg-surface/70 p-9 text-center"><CircleHelp className="mx-auto h-7 w-7 text-text-muted" /><h3 className="mt-3 font-serif text-xl text-text-primary">Nothing to assess yet</h3><p className="mx-auto mt-2 max-w-md text-sm leading-6 text-text-secondary">Prepare the relevant lessons first. Studio will show questions here when they are ready.</p><button type="button" onClick={onReview} className="mt-5 text-sm font-medium text-accent hover:underline">Go to lessons <ArrowRight className="ml-1 inline h-4 w-4" /></button></div>;
  return (
    <section className="rounded-2xl border border-border bg-surface shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-6 py-5"><div><div className="text-xs font-semibold uppercase tracking-[.16em] text-accent">{kind === 'practice' ? 'Practice' : kind === 'quiz' ? 'Checkpoint quiz' : 'Module test'}</div><h3 className="mt-1 font-serif text-2xl text-text-primary">{kind === 'test' ? 'Show what you can recall' : kind === 'quiz' ? 'A quick retrieval check' : 'Try the ideas in a new way'}</h3></div><div className="rounded-full bg-background px-3 py-1.5 text-xs tabular-nums text-text-secondary">{answered} of {questions.length} answered</div></div>
      <div className="p-6 sm:p-8">
        {kind === 'test' && <p className="mb-5 rounded-lg bg-[#f2ebe2] px-4 py-3 text-xs leading-5 text-text-secondary dark:bg-white/5">This module test uses exposed items from prepared lessons. Answer keys appear after submission; this result is a quiz score, not a mastery claim.</p>}
        <div className="mb-6 flex flex-wrap gap-2" aria-label="Question navigation">{questions.map((question, i) => <button type="button" key={question.id} aria-label={`Question ${i + 1}${Number.isInteger(attempt.answers[question.id]) ? ', answered' : ', unanswered'}`} aria-current={index === i ? 'step' : undefined} onClick={() => setIndex(i)} className={`grid h-8 w-8 place-items-center rounded-full text-xs font-semibold transition ${index === i ? 'bg-accent text-accent-fg' : Number.isInteger(attempt.answers[question.id]) ? 'bg-accent/15 text-accent' : 'bg-background text-text-muted hover:bg-border/60'}`}>{Number.isInteger(attempt.answers[question.id]) ? <Check size={14} /> : i + 1}</button>)}</div>
        {current && <div key={current.id}>
          <div className="text-xs font-medium uppercase tracking-[.13em] text-text-muted">Question {index + 1}</div>
          <div className="mt-3 max-w-3xl font-serif text-[1.45rem] leading-snug text-text-primary"><TiptapViewer content={current.prompt} /></div>
          <fieldset className="mt-6 space-y-2.5" disabled={pending}><legend className="sr-only">Choose one answer</legend>{current.options.map((option, i) => <label key={`${current.id}-${i}`} className={`flex cursor-pointer items-start gap-3 rounded-xl border px-4 py-3.5 transition ${attempt.answers[current.id] === i ? 'border-accent bg-accent/5 ring-1 ring-accent/30' : 'border-border hover:bg-background'} ${pending ? 'cursor-wait opacity-70' : ''}`}><input type="radio" name={`answer-${current.id}`} checked={attempt.answers[current.id] === i} onChange={() => onAnswer(current.id, i)} className="mt-1 accent-accent" /><span className="text-sm leading-6 text-text-secondary">{option}</span></label>)}</fieldset>
          {showCitations && !!current.sourceIds.length && <p className="mt-4 text-xs text-text-muted">Based on: {current.sourceIds.map((id) => sources.find((source) => source.id === id)?.title).filter(Boolean).join(', ') || 'program sources'}</p>}
        </div>}
        <div className="mt-8 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-5"><button type="button" disabled={index === 0} onClick={() => setIndex((i) => Math.max(0, i - 1))} className="inline-flex items-center gap-2 rounded-full px-4 py-2.5 text-sm text-text-secondary hover:bg-background disabled:opacity-35"><ArrowLeft size={15} /> Previous</button><div className="flex gap-2">{index < questions.length - 1 && <button type="button" onClick={() => setIndex((i) => Math.min(questions.length - 1, i + 1))} className="inline-flex items-center gap-2 rounded-full bg-background px-4 py-2.5 text-sm text-text-primary hover:bg-border/60">Next <ArrowRight size={15} /></button>}<button type="button" disabled={pending || answered !== questions.length} onClick={() => onSubmit(attempt.attemptId ?? newId())} className="inline-flex items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg disabled:cursor-not-allowed disabled:opacity-45">{pending ? 'Submitting…' : 'Submit answers'} <Check size={15} /></button></div></div>
        {answered !== questions.length && <p className="mt-3 text-right text-xs text-text-muted">Answer all {questions.length} questions before submitting.</p>}
        {error && <div role="alert" className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl bg-rose-500/10 px-4 py-3 text-sm text-rose-700"><span>Your answers are still here. {error}</span><span className="inline-flex items-center gap-1 text-xs"><RotateCcw size={13} /> Retry safely</span></div>}
      </div>
    </section>
  );
}

export function AttemptReview({ attempts, sources }: { attempts: LearningAttemptDto[]; sources: LearningSourceDto[] }) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  if (!attempts.length) return <div className="rounded-2xl border border-dashed border-border bg-surface p-10 text-center"><h3 className="font-serif text-xl text-text-primary">Your results will gather here</h3><p className="mt-2 text-sm text-text-secondary">Submitted practice, quizzes, and tests keep their own history.</p></div>;
  return <div className="space-y-5">{[...attempts].sort((a, b) => b.submittedAt - a.submittedAt).map((attempt) => <article key={attempt.id} className="overflow-hidden rounded-2xl border border-border bg-surface shadow-sm"><header className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-6 py-5"><div><div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">{attempt.kind} · submitted {formatSubmitted(attempt.submittedAt)}</div><h3 className="mt-1 font-serif text-xl text-text-primary">{attempt.correct} correct of {attempt.total}</h3></div><span className="rounded-full bg-background px-3 py-1.5 text-xs text-text-muted">Model-authored answer key</span></header><div className="divide-y divide-border">{attempt.results.map((result, i) => { const right = result.selectedIndex === result.correctIndex; return <div key={`${attempt.id}-${result.questionId}`} className="p-6"><div className="flex gap-3"><span className={`mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded-full ${right ? 'bg-emerald-500/15 text-emerald-700' : 'bg-rose-500/15 text-rose-700'}`}><Check size={14} /></span><div className="min-w-0"><div className="text-xs text-text-muted">Question {i + 1} · {right ? 'Matched the answer key' : 'Review this idea'}</div><div className="mt-1 font-medium leading-6 text-text-primary"><TiptapViewer content={result.prompt} /></div><p className="mt-2 text-sm leading-6 text-text-secondary"><span className="font-medium">Answer key: </span>{result.options[result.correctIndex]}</p><div className="mt-2 text-sm leading-6 text-text-secondary"><TiptapViewer content={result.explanation} /></div>{showCitations && result.sourceIds.length > 0 && <p className="mt-3 text-xs text-text-muted">Sources: {result.sourceIds.map((id) => sources.find((source) => source.id === id)?.title).filter(Boolean).join(', ') || 'saved source excerpt'}</p>}</div></div></div>; })}</div></article>)}</div>;
}
