import { useEffect, useRef, useState } from 'react';

import { Brain, Check } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useLearningRecallV2, useReviewLearningRecallCard } from '@/features/learning/portability/useLearningPortability';
import { MarkdownViewer } from '@/features/reading/components/viewers/MarkdownViewer';
import type { LearningRecallCardDto, StudyRating } from '@/lib/bindings';

/** A warm-up is short: a few ideas, not a review session. */
export const WARM_UP_CARDS = 3;

const RATINGS: { rating: StudyRating; label: string }[] = [
  { rating: 'again', label: 'Forgot' },
  { rating: 'hard', label: 'Hard' },
  { rating: 'good', label: 'Good' },
  { rating: 'easy', label: 'Easy' },
];

function newId() { return globalThis.crypto.randomUUID(); }

/** The cards due now, longest overdue first: the warm-up's queue. */
export function warmUpQueue(cards: LearningRecallCardDto[], now = Date.now()): string[] {
  return cards
    .filter((card) => card.scheduler.dueAt <= now)
    .sort((a, b) => a.scheduler.dueAt - b.scheduler.dueAt)
    .slice(0, WARM_UP_CARDS)
    .map((card) => card.id);
}

function masked(card: LearningRecallCardDto) {
  return card.content.clozeDeletions.filter(Boolean).reduce((text, deletion) => text.split(deletion).join('＿＿＿'), card.content.prompt);
}

/**
 * The warm-up before a lesson (learning science lessons §3.1): a few review
 * cards that are due, answered from memory before new material. Spaced
 * retrieval is what keeps earlier ideas, and mixing them in before the new
 * lesson asks the learner to tell ideas apart. Each rating feeds the card's
 * FSRS schedule exactly as a review in Recall does.
 */
export function LessonWarmUp({ programId }: { programId: string }) {
  const recall = useLearningRecallV2(programId);
  const review = useReviewLearningRecallCard(programId);
  // Fixed when the lesson opens, so a reviewed card does not shuffle the rest.
  const [queue, setQueue] = useState<string[] | null>(null);
  const [position, setPosition] = useState(0);
  const [revealed, setRevealed] = useState(false);
  const [choice, setChoice] = useState<number | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [error, setError] = useState<string>();
  const reviewIds = useRef<Record<string, string>>({});

  useEffect(() => {
    if (queue === null && recall.data) setQueue(warmUpQueue(recall.data.cards));
  }, [queue, recall.data]);

  if (dismissed || !queue?.length) return null;
  const due = recall.data?.cards.filter((card) => card.scheduler.dueAt <= Date.now()).length ?? 0;
  const card = recall.data?.cards.find((candidate) => candidate.id === queue[position]);
  const finished = position >= queue.length || !card;

  const rate = async (rating: StudyRating) => {
    if (!card) return;
    const key = `${card.id}:${rating}:${choice ?? ''}`;
    reviewIds.current[key] ??= newId();
    setError(undefined);
    try {
      await review.mutateAsync({ reviewId: reviewIds.current[key], programId, cardId: card.id, expectedReviewCount: card.scheduler.reviewCount, rating, selectedOption: card.format === 'multiple_choice' ? choice : null });
      setPosition((current) => current + 1);
      setRevealed(false);
      setChoice(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'This review could not be saved.');
    }
  };

  return (
    <section aria-labelledby="warm-up-heading" className="mb-6 rounded-lg border border-border bg-surface p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Brain size={16} className="text-accent" aria-hidden="true" />
          <h4 id="warm-up-heading" className="text-sm font-medium text-text-primary">Warm up from memory</h4>
          {!finished && <span className="text-xs text-text-muted">{position + 1} of {queue.length}</span>}
        </div>
        <button type="button" onClick={() => setDismissed(true)} className="text-xs text-text-secondary underline underline-offset-2 hover:text-text-primary">
          {finished ? 'Close' : 'Skip for now'}
        </button>
      </div>
      {finished ? (
        <p role="status" className="mt-3 flex items-center gap-2 text-sm text-text-secondary">
          <Check size={14} className="text-success-fg" /> Warmed up. {due > 0 ? `${due} more ${due === 1 ? 'idea waits' : 'ideas wait'} in Recall.` : 'Nothing else is due.'}
        </p>
      ) : (
        <>
          <p className="mt-1 text-xs leading-5 text-text-secondary">A few earlier ideas that are due. Answer in your head before revealing; recalling them now keeps them.</p>
          <div className="studio-lesson-prose mt-4"><MarkdownViewer content={card.format === 'cloze' ? masked(card) : card.content.prompt} /></div>
          {card.format === 'multiple_choice' && !revealed && (
            <fieldset className="mt-3 space-y-2">
              <legend className="sr-only">Choose an answer</legend>
              {card.content.options.map((option, index) => (
                <label key={`${card.id}:${index}`} className={`flex cursor-pointer items-start gap-2 rounded-md border px-3 py-2 text-sm ${choice === index ? 'border-accent bg-accent/5' : 'border-border'}`}>
                  <input type="radio" name={`warm-up-${card.id}`} checked={choice === index} onChange={() => setChoice(index)} className="mt-1 accent-accent" />
                  <span>{option}</span>
                </label>
              ))}
            </fieldset>
          )}
          {!revealed ? (
            <Button variant="outline" type="button" disabled={card.format === 'multiple_choice' && choice === null} onClick={() => setRevealed(true)} className="studio-button studio-button-primary mt-4 disabled:opacity-50">
              Reveal the answer
            </Button>
          ) : (
            <div className="mt-4 border-t border-border pt-4" aria-live="polite">
              {card.format === 'multiple_choice' && card.content.correctOptionIndex != null && (
                <p className="mb-2 text-sm text-text-secondary">{choice === card.content.correctOptionIndex ? 'Your choice is right.' : `The answer is: ${card.content.options[card.content.correctOptionIndex]}`}</p>
              )}
              <div className="studio-lesson-prose mt-0"><MarkdownViewer content={card.content.answer} /></div>
              {card.content.explanation && <div className="studio-lesson-prose mt-2 text-sm"><MarkdownViewer content={card.content.explanation} /></div>}
              {card.format === 'multiple_choice' ? (
                <Button variant="outline" type="button" disabled={review.isPending} onClick={() => void rate(choice === card.content.correctOptionIndex ? 'good' : 'again')} className="studio-button studio-button-primary mt-4">
                  {review.isPending ? 'Saving…' : 'Next'}
                </Button>
              ) : (
                <div className="mt-4">
                  <p className="text-xs text-text-muted">How well did you recall it?</p>
                  <div className="mt-2 flex flex-wrap gap-2" role="group" aria-label="How well did you recall it?">
                    {RATINGS.map(({ rating, label }) => (
                      <Button key={rating} variant="outline" type="button" disabled={review.isPending} onClick={() => void rate(rating)} className="studio-button disabled:opacity-50">{label}</Button>
                    ))}
                  </div>
                </div>
              )}
              {error && <p role="alert" className="mt-2 text-xs text-danger-fg">{error} Choose again to retry; nothing is saved twice.</p>}
            </div>
          )}
        </>
      )}
    </section>
  );
}
