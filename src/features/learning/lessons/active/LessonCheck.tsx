import { useEffect, useId, useRef, useState, type ReactNode } from 'react';

import { CalendarClock, Eye, RotateCcw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { checkCardContent, checkCardId, type CheckParts } from '@/features/learning/lessons/active/checkReveal';
import { useLessonDraft } from '@/features/learning/lessons/active/useLessonDraft';
import { useLearningRecallV2, useReviewLearningRecallCard, useSaveLearningRecallCard } from '@/features/learning/portability/useLearningPortability';
import { MarkdownViewer } from '@/features/reading/components/viewers/MarkdownViewer';
import type { StudyRating } from '@/lib/bindings';

/**
 * How close the learner's own answer came decides the card's first review:
 * a miss comes back soonest. "Easy" is not offered on first sight.
 */
const CLOSENESS: { rating: StudyRating; label: string }[] = [
  { rating: 'again', label: 'Missed it' },
  { rating: 'hard', label: 'Partly' },
  { rating: 'good', label: 'Got it' },
];

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const DAY = 86_400_000;

function newId() { return globalThis.crypto.randomUUID(); }

/** When a scheduled card comes back, in words. */
export function comesBack(dueAt: number, now = Date.now()): string {
  const days = Math.round((dueAt - now) / DAY);
  if (days <= 0) return 'later today';
  if (days === 1) return 'tomorrow';
  if (days < 14) return `in ${days} days`;
  return `on ${new Date(dueAt).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}`;
}

/**
 * A check (learning science lessons §3.3): the learner commits to an answer
 * to a fresh case before the reasoning is shown. Committing first, even to
 * a guess, is what makes the explanation stick; the reveal names the
 * misconception behind each plausible wrong answer. Their rating of how
 * close they came schedules the case as a review card, so it comes back
 * later from memory (§3.5).
 */
export function LessonCheck({ programId, lessonId, lessonTitle, section, body, parts, sourceIds, active, questionView }: {
  programId: string;
  lessonId: string;
  lessonTitle: string;
  /** The check's section index in its lesson. */
  section: number;
  /** The check's whole body: a rewritten check names a new card. */
  body: string;
  parts: CheckParts;
  /** The section's cited source versions; the card cites them too. */
  sourceIds: string[];
  /** Whether the course is active: only an active course schedules reviews. */
  active: boolean;
  /** The case, as the lesson renders prose (with tangent selection). */
  questionView: ReactNode;
}) {
  const answerId = useId();
  const draftKey = `${lessonId}.check.${section}`;
  const [answer, setAnswer] = useLessonDraft(draftKey);
  const [revealedFlag, setRevealedFlag] = useLessonDraft(`${draftKey}.revealed`);
  const revealed = revealedFlag === '1';
  const [cardId, setCardId] = useState<string>();
  const recall = useLearningRecallV2(active ? programId : '');
  const saveCard = useSaveLearningRecallCard(programId);
  const reviewCard = useReviewLearningRecallCard(programId);
  const [error, setError] = useState<string>();
  const ids = useRef<{ operation: string; review: Partial<Record<StudyRating, string>> }>({ operation: newId(), review: {} });
  const revealRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let current = true;
    void checkCardId(lessonId, section, body).then((id) => { if (current) setCardId(id); });
    return () => { current = false; };
  }, [lessonId, section, body]);

  const card = recall.data?.cards.find((candidate) => candidate.id === cardId);
  const scheduled = card && card.scheduler.reviewCount > 0 ? card : undefined;
  const pending = saveCard.isPending || reviewCard.isPending;

  const reveal = () => {
    setRevealedFlag('1');
    requestAnimationFrame(() => revealRef.current?.focus());
  };

  const rate = async (rating: StudyRating) => {
    if (!cardId) return;
    setError(undefined);
    try {
      if (!card) {
        await saveCard.mutateAsync({
          operationId: ids.current.operation,
          programId,
          cardId,
          expectedContentRevision: null,
          format: 'question_answer',
          content: checkCardContent(parts),
          sourceVersionIds: [...new Set(sourceIds.filter((id) => UUID.test(id)))].slice(0, 20),
          changeReason: `From the check in “${lessonTitle}”`.slice(0, 500),
        });
      }
      const reviewId = ids.current.review[rating] ?? newId();
      ids.current.review[rating] = reviewId;
      await reviewCard.mutateAsync({ reviewId, programId, cardId, expectedReviewCount: 0, rating, selectedOption: null });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Your rating could not be saved.');
    }
  };

  return (
    <div className="mt-4">
      <div className="studio-lesson-prose">{questionView}</div>
      <div className="mt-5 rounded-lg border border-accent/30 bg-accent/5 p-4">
        <label htmlFor={answerId} className="block text-sm font-medium text-text-primary">Your answer</label>
        <p className="mt-1 text-xs leading-5 text-text-secondary">Commit before you look. A guess counts: answering first is what makes the explanation stick.</p>
        <textarea
          id={answerId}
          value={answer}
          onChange={(event) => setAnswer(event.target.value)}
          rows={4}
          maxLength={4000}
          placeholder="What happens, and why?"
          className="mt-3 block w-full min-w-0 rounded-md border border-border bg-surface px-3 py-2 text-sm leading-6 text-text-primary"
        />
        {!revealed && (
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <Button variant="outline" type="button" disabled={!answer.trim()} onClick={reveal} className="studio-button studio-button-primary disabled:opacity-50">
              <Eye size={14} /> Check my thinking
            </Button>
            <button type="button" onClick={reveal} className="text-xs text-text-secondary underline underline-offset-2 hover:text-text-primary">
              I don’t know yet: show me
            </button>
          </div>
        )}
      </div>
      {revealed && (
        <div ref={revealRef} tabIndex={-1} className="mt-5 outline-none" aria-live="polite">
          <div className="studio-section-label">Check your thinking</div>
          <div className="studio-lesson-prose"><MarkdownViewer content={parts.reveal} /></div>
          {active && (scheduled ? (
            <p role="status" className="mt-4 flex items-center gap-2 text-xs text-text-secondary">
              <CalendarClock size={14} className="text-accent" />
              In your reviews: this case comes back {comesBack(scheduled.scheduler.dueAt)}.
            </p>
          ) : (
            <div className="mt-5 rounded-lg border border-border bg-surface p-4">
              <p className="text-sm font-medium text-text-primary">How close was your answer?</p>
              <p className="mt-1 text-xs leading-5 text-text-secondary">Your answer decides when this case comes back for you to answer from memory: a miss returns soonest.</p>
              <div className="mt-3 flex flex-wrap gap-2" role="group" aria-label="How close was your answer?">
                {CLOSENESS.map(({ rating, label }) => (
                  <Button key={rating} variant="outline" type="button" disabled={pending || !cardId || recall.isLoading} onClick={() => void rate(rating)} className="studio-button disabled:opacity-50">
                    {label}
                  </Button>
                ))}
              </div>
              {pending && <p role="status" className="mt-2 text-xs text-text-muted">Scheduling…</p>}
              {error && (
                <p role="alert" className="mt-2 flex flex-wrap items-center gap-1 text-xs text-danger-fg">
                  <RotateCcw size={12} /> {error} Choose again to retry; nothing is saved twice.
                </p>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
