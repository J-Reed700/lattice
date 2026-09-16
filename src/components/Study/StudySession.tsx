import { useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { PageHeader } from '@/components/ui/PageHeader';
import type { StudyCardDto, StudyRating } from '@/lib/bindings';
import { cn } from '@/lib/utils';

import { StudySource } from './StudySource';
import { useReviewStudyCard } from './useStudy';

export type StudyMode = 'flashcard' | 'quiz';
export function StudySession({ cards, mode, title, onClose }: { cards: StudyCardDto[]; mode: StudyMode; title: string; onClose: () => void }) {
  const [index, setIndex] = useState(0);
  const [revealed, setRevealed] = useState(false);
  const [selected, setSelected] = useState<number | null>(null);
  const [correctCount, setCorrectCount] = useState(0);
  const [missed, setMissed] = useState<string[]>([]);
  const reviewId = useRef(crypto.randomUUID());
  const review = useReviewStudyCard();
  const card = cards[index];
  const next = () => { setIndex(i => i + 1); setSelected(null); setRevealed(false); reviewId.current = crypto.randomUUID(); review.reset(); };
  const grade = (rating: StudyRating) => {
    if (review.isPending || (mode === 'quiz' && selected === null)) return;
    review.mutate({ reviewId: reviewId.current, cardId: card.id, expectedReviews: card.reviewCount, selectedOption: mode === 'quiz' ? selected : null, rating }, {
      onSuccess: saved => {
        const correct = mode === 'quiz' ? selected === saved.correctIndex : rating !== 'again';
        if (correct) setCorrectCount(c => c + 1);
        else setMissed(previous => [...previous, card.topic]);
        if (mode === 'quiz') setRevealed(true);
        else next();
      },
    });
  };
  if (!card) return <>
    <PageHeader title={mode === 'quiz' ? 'Practice complete' : 'Review complete'} meta={`${cards.length} ${mode === 'quiz' ? cards.length === 1 ? 'question' : 'questions' : cards.length === 1 ? 'card' : 'cards'} · ${correctCount} ${mode === 'quiz' ? 'correct' : 'recalled'}`} />
    <p className="text-sm text-text-secondary">Your results and next review dates are saved.</p>
    {missed.length > 0 && <div className="mt-8"><h2 className="mb-3 text-lg text-text-primary">Revisit</h2><ul className="space-y-2 text-sm text-text-secondary">{[...new Set(missed)].map(topic => <li key={topic}>{topic}</li>)}</ul></div>}
    <Button className="mt-8" onClick={onClose}>Back to deck</Button>
  </>;
  return <>
    <PageHeader title={mode === 'quiz' ? 'Practice quiz' : 'Flashcards'} meta={`${index + 1} of ${cards.length} · ${title}`} actions={<Button variant="ghost" disabled={review.isPending} onClick={onClose}>End session</Button>} />
    <div className="mb-8 h-1 overflow-hidden rounded-full bg-surface-raised" role="progressbar" aria-label="Session progress" aria-valuenow={index} aria-valuemin={0} aria-valuemax={cards.length}><div className="h-full bg-accent transition-all duration-fast" style={{ width: `${index / cards.length * 100}%` }} /></div>
    {revealed && <p className="mb-4 text-xs text-text-muted">{card.topic}</p>}
    <h2 className="font-serif text-2xl leading-relaxed text-text-primary">{card.question}</h2>
    {mode === 'quiz' && <fieldset className="mt-8 space-y-2" disabled={review.isPending || revealed}>
      <legend className="sr-only">Choose an answer</legend>
      {card.options.map((option, i) => <label key={i} className={cn('flex cursor-pointer items-start gap-3 rounded-md border p-4 text-sm leading-relaxed text-text-primary', selected === i ? 'border-accent bg-accent-muted' : 'border-border-subtle', revealed && i === card.correctIndex && 'border-accent')}>
        <input type="radio" name="study-answer" checked={selected === i} onChange={() => setSelected(i)} className="mt-1 accent-accent" />
        <span><span className="mr-2 text-text-muted">{String.fromCharCode(65 + i)}.</span>{' '}{option}{revealed && i === card.correctIndex && <span className="ml-2 font-medium text-accent">Correct answer</span>}</span>
      </label>)}
    </fieldset>}
    {revealed && <div className="mt-8 border-t border-border-subtle pt-6" aria-live="polite">
      <h3 className="mb-3 text-sm font-medium text-text-secondary">{mode === 'quiz' ? selected === card.correctIndex ? 'Correct' : 'Review this answer' : 'Answer'}</h3>
      {mode === 'flashcard' && <p className="mb-4 text-lg leading-relaxed text-text-primary">{card.answer}</p>}
      <p className="whitespace-pre-wrap text-sm leading-relaxed text-text-secondary">{card.explanation}</p>
      <StudySource sources={card.citations?.length ? card.citations : [card.source]} />
    </div>}
    {review.isError && <p role="alert" className="mt-4 text-sm text-danger-fg">Couldn’t save this review: {review.error.message}</p>}
    <div className="mt-8 flex flex-wrap justify-end gap-2">
      {mode === 'flashcard' && !revealed && <Button onClick={() => setRevealed(true)}>Reveal answer</Button>}
      {mode === 'flashcard' && revealed && (['again', 'hard', 'good', 'easy'] as const).map(rating => <Button key={rating} variant={rating === 'good' ? 'default' : 'secondary'} disabled={review.isPending} onClick={() => grade(rating)}>{rating[0].toUpperCase() + rating.slice(1)}</Button>)}
      {mode === 'quiz' && !revealed && <Button disabled={selected === null || review.isPending} onClick={() => grade('good')}>{review.isPending ? 'Saving…' : 'Check answer'}</Button>}
      {mode === 'quiz' && revealed && <Button onClick={next}>{index + 1 === cards.length ? 'Finish practice' : 'Next question'}</Button>}
    </div>
  </>;
}
