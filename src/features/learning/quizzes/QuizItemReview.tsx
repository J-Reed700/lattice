import { useState } from 'react';

import { BookmarkPlus, Check, Flag, MessageSquareQuote } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { ContestForm, ContestOutcome } from '@/features/learning/quizzes/GradeContest';
import { GradeRationale } from '@/features/learning/quizzes/GradeRationale';
import { itemRegistry } from '@/features/learning/quizzes/items/itemRegistry';
import { draftOf } from '@/features/learning/quizzes/items/itemResponse';
import { itemVerdict, type GradingState } from '@/features/learning/quizzes/quizCopy';
import { QuizSourceLine } from '@/features/learning/quizzes/QuizSourceLine';
import { VerdictChip } from '@/features/learning/quizzes/QuizVerdict';
import type { LearningAssessmentFormItemDto, LearningAssessmentItemResultDto, LearningSourceDto } from '@/lib/bindings';

const CONTESTABLE = new Set(['provisional', 'uncertain', 'needs_review']);
const link = 'inline-flex min-h-11 items-center gap-1.5 rounded-full px-3 text-sm font-medium text-text-secondary hover:bg-background hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50';

export type ItemActions = {
  addToRecall: (itemId: string) => void;
  recallAdded: (itemId: string) => boolean;
  recallPending: boolean;
  contest: (itemId: string, note: string) => Promise<boolean>;
  contestPending: boolean;
  contestError?: string;
  override: (itemId: string) => void;
  overridePending: boolean;
  overrideError?: string;
  report: (itemId: string, note: string) => Promise<boolean>;
  reportPending: boolean;
  reported: (itemId: string) => boolean;
};

function ReportLink({ itemId, actions }: { itemId: string; actions: ItemActions }) {
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState('');
  if (actions.reported(itemId)) return <p className="text-xs text-text-muted">Reported · this question is left out of future quizzes.</p>;
  if (!open) return <button type="button" onClick={() => setOpen(true)} className={link}><Flag size={14} aria-hidden="true" /> Report this question</button>;
  return <form className="w-full rounded-xl border border-border bg-surface p-3" onSubmit={(event) => { event.preventDefault(); void actions.report(itemId, note.trim()).then((ok) => ok && setOpen(false)); }}>
    <label htmlFor={`report-${itemId}`} className="text-sm font-semibold text-text-primary">What is wrong with this question?</label>
    <textarea id={`report-${itemId}`} rows={2} value={note} onChange={(event) => setNote(event.target.value)} className="mt-2 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-text-primary outline-hidden focus:border-accent" />
    <div className="mt-2 flex justify-end gap-2">
      <button type="button" onClick={() => setOpen(false)} className={link}>Cancel</button>
      <button type="submit" disabled={actions.reportPending} className="min-h-11 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg disabled:opacity-50">Report</button>
    </div>
  </form>;
}

/** One reviewed question: verdict in words, the key and why, the grade's reasons, and what you can do next. */
export function QuizItemReview({ item, result, index, grading, practice, sources, actions }: {
  item: LearningAssessmentFormItemDto; result: LearningAssessmentItemResultDto | null; index: number; grading: GradingState;
  practice: boolean; sources: LearningSourceDto[]; actions: ItemActions;
}) {
  const [contesting, setContesting] = useState(false);
  const renderer = itemRegistry[item.format];
  const draft = draftOf(item);
  const verdict = result ? itemVerdict(result, grading) : null;
  const canContest = Boolean(result && !practice && CONTESTABLE.has(result.gradeStatus) && !result.learnerOverride && (!result.contest || result.contest.status === 'failed'));
  const canRecall = Boolean(verdict && verdict.lostPoints && !verdict.pending);
  const answer = draft.textResponse;
  // A written answer's reasons come before what a strong answer covers.
  const rationaleFirst = item.format === 'explanation' && Boolean(result?.rationale);
  const review = <div className="mt-4">{item.keyReveal
    ? <renderer.Review item={item} draft={draft} reveal={item.keyReveal} result={result} />
    : <p className="text-sm text-text-muted">The key appears here once this answer is graded.</p>}</div>;
  const recall = canRecall && (actions.recallAdded(item.id)
    ? <span className="inline-flex min-h-11 items-center gap-1.5 px-3 text-sm text-emerald-800 dark:text-emerald-200"><Check size={14} aria-hidden="true" /> Added to Recall</span>
    : <button type="button" disabled={actions.recallPending} onClick={() => actions.addToRecall(item.id)} className={link}><BookmarkPlus size={14} aria-hidden="true" /> Add to Recall</button>);
  const contestControls = <>
    {canContest && !contesting && <button type="button" onClick={() => setContesting(true)} className={link}><MessageSquareQuote size={14} aria-hidden="true" /> I think I was right</button>}
    {contesting && <div className="w-full"><ContestForm pending={actions.contestPending} error={actions.contestError} onCancel={() => setContesting(false)}
      onSubmit={(note) => void actions.contest(item.id, note).then((ok) => ok && setContesting(false))} /></div>}
    {result?.contest && <div className="w-full"><ContestOutcome contest={result.contest} answer={answer} sources={sources}
      overridePending={actions.overridePending} overrideError={actions.overrideError} onOverride={() => actions.override(item.id)} /></div>}
  </>;
  return <li className="px-5 py-6 sm:px-8" aria-labelledby={`review-${item.id}`}>
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
      <h4 id={`review-${item.id}`} className="text-[11px] font-semibold uppercase tracking-[.15em] text-text-muted">Question {index + 1} · {renderer.label}</h4>
      {verdict && <VerdictChip words={verdict} />}
      {verdict?.detail && <span className="text-xs text-text-secondary">{verdict.detail}</span>}
      {item.previouslyExposed && <span className="text-xs text-text-muted">· Seen before</span>}
    </div>
    {!renderer.ownsPrompt && <div className="mt-2 max-w-3xl font-serif text-lg leading-snug text-text-primary"><TiptapViewer content={item.prompt} /></div>}
    {!rationaleFirst && review}
    {result?.rationale && <GradeRationale rationale={result.rationale} answer={answer} lostPoints={Boolean(verdict?.lostPoints)} statusWord={verdict?.label.toLowerCase() ?? 'graded'} sources={sources}>
      <div className="flex flex-wrap items-center gap-1">{recall}{contestControls}</div>
    </GradeRationale>}
    {rationaleFirst && review}
    {!result?.rationale && result?.feedback && result.gradeStatus !== 'pending' && <p className="mt-3 text-sm leading-6 text-text-secondary">{result.feedback}</p>}
    {!result?.rationale && <div className="mt-3 flex flex-wrap items-center gap-1">{recall}{contestControls}{result?.gradeStatus === 'deterministic' && !practice && <ReportLink itemId={item.id} actions={actions} />}</div>}
    <QuizSourceLine ids={item.sourceVersionIds} sources={sources} label="Source" />
  </li>;
}
