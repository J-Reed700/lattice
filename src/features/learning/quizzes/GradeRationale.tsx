import { useState, type ReactNode } from 'react';

import { Check, ChevronDown, CircleDot, CircleHelp, X } from 'lucide-react';

import { points, shortDate } from '@/features/learning/quizzes/quizCopy';
import { listNumbers, numberQuotes } from '@/features/learning/quizzes/rationaleHighlights';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningCriterionRationaleDto, LearningCriterionVerdict, LearningGradeRationaleDto, LearningSourceDto } from '@/lib/bindings';

const verdictWords: Record<LearningCriterionVerdict, { label: string; icon: ReactNode; className: string }> = {
  met: { label: 'Met', icon: <Check size={13} strokeWidth={2.5} />, className: 'bg-emerald-500/12 text-emerald-800 dark:text-emerald-200' },
  partly_met: { label: 'Partly met', icon: <CircleDot size={13} />, className: 'bg-amber-500/14 text-amber-900 dark:text-amber-200' },
  missed: { label: 'Missed', icon: <X size={13} strokeWidth={2.5} />, className: 'bg-rose-500/12 text-rose-800 dark:text-rose-200' },
  uncertain: { label: 'Uncertain', icon: <CircleHelp size={13} />, className: 'bg-background text-text-secondary' },
};

export function CriterionVerdict({ verdict }: { verdict: LearningCriterionVerdict }) {
  const words = verdictWords[verdict];
  return <span className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold ${words.className}`}><span aria-hidden="true">{words.icon}</span>{words.label}</span>;
}

function sourceTitle(sources: LearningSourceDto[], id: string | null) {
  return sources.find((source) => source.id === id)?.title ?? 'the lesson';
}

/** The learner's answer, each quoted span highlighted and numbered; the number is in the text, not only the colour. */
export function HighlightedAnswer({ answer, criteria }: { answer: string; criteria: LearningCriterionRationaleDto[] }) {
  const { segments, loose } = numberQuotes(answer, criteria);
  if (!answer.trim()) return <p className="text-sm italic text-text-muted">No answer given.</p>;
  return <div>
    <p className="whitespace-pre-wrap text-sm leading-7 text-text-secondary">{segments.map((segment, index) => (segment.quote
      ? <mark key={index} data-quote={segment.quote} className="rounded bg-accent/15 px-0.5 text-text-primary">{segment.text}<sup className="ml-0.5 font-sans text-[10px] font-bold text-accent">{segment.quote}</sup><span className="sr-only"> (quote {segment.quote})</span></mark>
      : <span key={index}>{segment.text}</span>))}</p>
    {loose.length > 0 && <ul className="mt-2 space-y-1 text-xs text-text-muted">{loose.map((item) => <li key={item.quote}><span className="font-bold text-accent">{item.quote}</span> “{item.text}”</li>)}</ul>}
  </div>;
}

function CriterionRow({ criterion, quotes, sources }: { criterion: LearningCriterionRationaleDto; quotes: number[]; sources: LearningSourceDto[] }) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const lost = criterion.verdict !== 'met';
  const share = criterion.points === null ? 'not graded' : `${points(criterion.points)} of ${points(criterion.maxPoints)}`;
  return <li className="rounded-xl border border-border bg-surface px-4 py-3">
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
      <CriterionVerdict verdict={criterion.verdict} />
      <span className="text-sm font-semibold text-text-primary">{criterion.title}</span>
      <span className="text-xs tabular-nums text-text-muted">· {share}</span>
    </div>
    {quotes.length > 0 && <p className="mt-1.5 text-xs text-text-secondary">Rests on {quotes.length === 1 ? 'quote' : 'quotes'} {listNumbers(quotes)}, in your words.</p>}
    {lost && criterion.whatWasMissing.trim() && <p className="mt-1.5 text-sm leading-6 text-text-secondary"><span className="font-semibold text-text-primary">What was missing: </span>{criterion.whatWasMissing}</p>}
    {lost && criterion.fullCreditIncludes.trim() && <p className="mt-1 text-sm leading-6 text-text-secondary"><span className="font-semibold text-text-primary">Full credit includes: </span>{criterion.fullCreditIncludes}</p>}
    {showCitations && criterion.sourceQuote && <blockquote className="mt-2 border-l-2 border-accent/40 pl-3 text-xs italic leading-5 text-text-muted">“{criterion.sourceQuote}” <span className="not-italic">— {sourceTitle(sources, criterion.sourceVersionId)}</span></blockquote>}
  </li>;
}

/** The criteria of one rationale, numbered against the same answer. Also used side by side in a contest. */
export function RationaleCriteria({ rationale, answer, sources }: { rationale: LearningGradeRationaleDto; answer: string; sources: LearningSourceDto[] }) {
  const { byCriterion } = numberQuotes(answer, rationale.criteria);
  return <ul className="space-y-2">{rationale.criteria.map((criterion, index) => <CriterionRow key={criterion.criterionId} criterion={criterion} quotes={byCriterion[index] ?? []} sources={sources} />)}</ul>;
}

export function graderLine(rationale: LearningGradeRationaleDto, statusWord: string) {
  return `Graded by ${rationale.graderModel || 'your model'} · ${statusWord} · ${shortDate(rationale.gradedAt)}`;
}

/**
 * "Why this lost points" (§4.3a): the summary, the answer with numbered
 * highlights, each criterion with what was missing and what full credit
 * includes, the source line and the grader. Open when points were lost.
 */
export function GradeRationale({ rationale, answer, lostPoints, statusWord, sources, children }: {
  rationale: LearningGradeRationaleDto; answer: string; lostPoints: boolean; statusWord: string; sources: LearningSourceDto[]; children?: ReactNode;
}) {
  const [open, setOpen] = useState(lostPoints);
  return <details open={open} onToggle={(event) => setOpen(event.currentTarget.open)} className="group mt-4 rounded-2xl border border-[#d7c9b8] bg-[#f6f1ea] dark:border-white/10 dark:bg-[#24211c]">
    <summary className="flex min-h-11 cursor-pointer list-none items-center justify-between gap-3 rounded-2xl px-4 text-sm font-semibold text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">
      <span>{lostPoints ? 'Why this lost points' : 'How this was graded'}</span>
      <ChevronDown size={16} aria-hidden="true" className="text-text-muted transition group-open:rotate-180" />
    </summary>
    <div className="space-y-4 px-4 pb-4">
      <p className="font-serif text-base leading-7 text-text-primary">{rationale.summary}</p>
      <section aria-label="Your answer">
        <h5 className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Your answer</h5>
        <div className="mt-1.5 rounded-xl bg-surface px-4 py-3"><HighlightedAnswer answer={answer} criteria={rationale.criteria} /></div>
      </section>
      <section aria-label="Criteria">
        <h5 className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Criteria</h5>
        <div className="mt-1.5"><RationaleCriteria rationale={rationale} answer={answer} sources={sources} /></div>
      </section>
      <p className="text-xs text-text-muted">{graderLine(rationale, statusWord)}</p>
      {children}
    </div>
  </details>;
}
