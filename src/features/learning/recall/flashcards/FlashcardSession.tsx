import { useEffect, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { PageHeader } from '@/components/ui/PageHeader';
import { CardText } from '@/features/learning/recall/CardText';
import { FlashcardSource } from '@/features/learning/recall/flashcards/FlashcardSource';
import {
  answer,
  answerSeconds,
  attemptsFor,
  CONFIDENCE_LABEL,
  type Confidence,
  fromNow,
  keyPoints,
  type SessionQueue,
  startQueue,
  suggestedRating,
  summarize,
} from '@/features/learning/recall/flashcards/retrieval';
import { useReviewFlashcard } from '@/features/learning/recall/flashcards/useFlashcards';
import type { StudyCardDto, StudyRating } from '@/lib/bindings';
import { cn } from '@/lib/utils';


export type FlashcardMode = 'flashcard' | 'quiz';

const RATINGS: { rating: StudyRating; hint: string }[] = [
  { rating: 'again', hint: 'missed it' },
  { rating: 'hard', hint: 'with effort' },
  { rating: 'good', hint: 'got it' },
  { rating: 'easy', hint: 'instantly' },
];
const CONFIDENCES: Confidence[] = ['guess', 'unsure', 'sure'];

function clock(seconds: number) {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

function capitalized(value: string) {
  return value[0].toUpperCase() + value.slice(1);
}

/** Counts up while a card is being answered, against the time to aim for. */
function AnswerClock({ seconds }: { seconds: number }) {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const started = Date.now();
    const timer = window.setInterval(() => setElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const over = elapsed > seconds;
  return <div className="mt-6 flex items-center gap-3 text-xs text-text-muted">
    <span>Answer aloud and in full, as you would if asked</span>
    <span className={cn('tabular-nums', over && 'font-medium text-text-primary')} aria-live="off">{clock(elapsed)} / {clock(seconds)}</span>
    <span className="h-1 flex-1 overflow-hidden rounded-full bg-surface-raised" aria-hidden="true"><span className={cn('block h-full transition-all', over ? 'bg-text-muted' : 'bg-accent')} style={{ width: `${Math.min(100, elapsed / seconds * 100)}%` }} /></span>
  </div>;
}

function Summary({ queue, mode, total, title, savedDue, targetAt, onClose }: {
  queue: SessionQueue; mode: FlashcardMode; total: number; title: string;
  savedDue: Map<string, number>; targetAt?: number | null; onClose: () => void;
}) {
  const summary = summarize(queue.attempts);
  const now = Date.now();
  const missedTopics = [...new Set(queue.attempts.filter(attempt => attempt.rating === 'again').map(attempt => attempt.topic))];
  const nextDue = savedDue.size ? Math.min(...savedDue.values()) : null;
  const unit = mode === 'quiz' ? (total === 1 ? 'question' : 'questions') : (total === 1 ? 'card' : 'cards');
  const sure = summary.calibration.find(row => row.confidence === 'sure');
  return <>
    <PageHeader title={mode === 'quiz' ? 'Practice complete' : 'Review complete'} meta={`${total} ${unit} · ${summary.recalledFirstTry} ${mode === 'quiz' ? 'correct' : 'recalled'} first try`} />
    <dl className="grid grid-cols-3 gap-3 text-center">
      <div className="rounded-xl border border-border-subtle p-4"><dt className="text-xs text-text-muted">First try</dt><dd className="mt-1 font-serif text-2xl text-text-primary">{summary.recalledFirstTry}</dd></div>
      <div className="rounded-xl border border-border-subtle p-4"><dt className="text-xs text-text-muted">Relearned</dt><dd className="mt-1 font-serif text-2xl text-text-primary">{summary.relearned}</dd></div>
      <div className="rounded-xl border border-border-subtle p-4"><dt className="text-xs text-text-muted">Still shaky</dt><dd className="mt-1 font-serif text-2xl text-text-primary">{summary.stillMissed.length}</dd></div>
    </dl>
    {summary.relearned > 0 && <p className="mt-4 text-sm leading-relaxed text-text-secondary">Each missed card came back until you recalled it. Recalling it again in a later session is what makes it last.</p>}
    {summary.confidentErrors.length > 0 && <section className="mt-8 rounded-xl border border-accent/40 bg-accent/5 p-5">
      <h2 className="text-base font-medium text-text-primary">You were sure, and it was wrong</h2>
      <p className="mt-1 text-sm leading-relaxed text-text-secondary">These are the mistakes people fix best once they read the correction closely. Go over these answers once more now.</p>
      <ul className="mt-3 space-y-2 text-sm text-text-primary">{summary.confidentErrors.map(question => <li key={question}><CardText text={question} /></li>)}</ul>
    </section>}
    {sure && sure.total >= 2 && <p className="mt-6 text-sm text-text-secondary">When you said “I know this”, you were right {sure.right} of {sure.total} times.{sure.right < sure.total ? ' Treat that feeling as a hint, not proof: answer in full before you check.' : ''}</p>}
    {missedTopics.length > 0 && <div className="mt-8"><h2 className="mb-3 text-lg text-text-primary">Revisit</h2><ul className="space-y-2 text-sm text-text-secondary">{missedTopics.map(topic => <li key={topic}>{topic}</li>)}</ul></div>}
    <div className="mt-8 rounded-xl bg-surface-raised p-5 text-sm leading-relaxed text-text-secondary">
      <p className="font-medium text-text-primary">Your results and next review times are saved.</p>
      {targetAt && targetAt > now
        ? <p className="mt-1">Target {fromNow(targetAt, now)}{nextDue ? `; the next cards come due ${fromNow(nextDue, now)}` : ''}. Come back then for a short session rather than drilling on now: one correct recall in each of a few spaced sessions lasts far longer than many in one sitting. Fit one session in before you sleep and a short warm-up the morning of.</p>
        : nextDue ? <p className="mt-1">The next cards in {title} come due {fromNow(nextDue, now)}.</p> : null}
    </div>
    <Button className="mt-8" onClick={onClose}>Back to deck</Button>
  </>;
}

/**
 * A study session built on retrieval practice: answer from memory, say how
 * sure you are, then see the answer and rate the recall. A miss comes back
 * later in the session until it is recalled (see retrieval.ts for the
 * research behind each step).
 */
export function FlashcardSession({ cards, mode, title, onClose, targetAt }: { cards: StudyCardDto[]; mode: FlashcardMode; title: string; onClose: () => void; targetAt?: number | null }) {
  const sessionCards = mode === 'quiz' ? cards.filter(card => (card.format ?? 'multiple_choice') === 'multiple_choice' && card.options.length > 0) : cards;
  const [queue, setQueue] = useState(() => startQueue(sessionCards));
  const [ended, setEnded] = useState(false);
  const [confidence, setConfidence] = useState<Confidence | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [selected, setSelected] = useState<number | null>(null);
  const [attemptText, setAttemptText] = useState('');
  const [covered, setCovered] = useState<Set<number>>(new Set());
  const [result, setResult] = useState<{ saved: StudyCardDto; rating: StudyRating } | null>(null);
  const [savedDue] = useState(() => new Map<string, number>());
  const reviewAttempt = useRef<{ signature: string; id: string } | null>(null);
  const review = useReviewFlashcard();
  const card = ended ? undefined : queue.pending[0];
  const total = new Set(queue.attempts.map(attempt => attempt.cardId).concat(queue.pending.map(item => item.id))).size;
  const done = total - new Set(queue.pending.map(item => item.id)).size;
  const attemptNumber = card ? attemptsFor(queue, card.id) : 0;
  // The card's place among distinct cards: a relearning card keeps its first place.
  const position = new Set(queue.attempts.map(attempt => attempt.cardId)).size + (attemptNumber === 0 ? 1 : 0);
  const points = card && mode === 'flashcard' ? keyPoints(card.answer) : [];
  const suggestion = suggestedRating(covered.size, points.length);
  const seconds = card && mode === 'flashcard' ? answerSeconds(card) : 0;

  const advance = (saved: StudyCardDto, rating: StudyRating) => {
    savedDue.set(saved.id, saved.dueAt);
    setQueue(current => answer(current, saved, rating, confidence));
    setSelected(null); setRevealed(false); setConfidence(null); setAttemptText(''); setCovered(new Set()); setResult(null);
    reviewAttempt.current = null; review.reset();
  };
  const save = (rating: StudyRating, onSaved: (saved: StudyCardDto) => void) => {
    if (!card || review.isPending) return;
    const selectedOption = mode === 'quiz' ? selected : null;
    const signature = `${card.id}:${attemptNumber}:${rating}:${selectedOption ?? 'recall'}`;
    if (reviewAttempt.current?.signature !== signature) reviewAttempt.current = { signature, id: crypto.randomUUID() };
    review.mutate({ reviewId: reviewAttempt.current.id, cardId: card.id, expectedReviews: card.reviewCount, selectedOption, rating }, { onSuccess: onSaved });
  };
  const reveal = (level: Confidence) => {
    if (!card || review.isPending) return;
    setConfidence(level);
    if (mode === 'flashcard') { setRevealed(true); return; }
    if (selected === null) return;
    save('good', saved => {
      setResult({ saved, rating: selected === saved.correctIndex ? 'good' : 'again' });
      setRevealed(true);
    });
  };
  const rate = (rating: StudyRating) => {
    if (mode !== 'flashcard') return;
    save(rating, saved => {
      // A confident miss stays on screen until the learner has read the correction.
      if (rating === 'again' && confidence === 'sure') setResult({ saved, rating });
      else advance(saved, rating);
    });
  };

  // Number keys: 1-3 say how sure you are, then 1-4 rate the recall. Typing in the answer box is left alone.
  const keys = useRef<(key: string) => void>(() => undefined);
  keys.current = (key: string) => {
    if (!card || review.isPending) return;
    const index = Number(key) - 1;
    if (!revealed && index >= 0 && index < CONFIDENCES.length && (mode === 'flashcard' || selected !== null)) reveal(CONFIDENCES[index]);
    else if (revealed && mode === 'flashcard' && !result && index >= 0 && index < RATINGS.length) rate(RATINGS[index].rating);
  };
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.closest('textarea, select, [contenteditable="true"], input:not([type="checkbox"]):not([type="radio"]):not([type="button"])');
      if (event.metaKey || event.ctrlKey || event.altKey || typing) return;
      keys.current(event.key);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (mode === 'quiz' && cards.length > 0 && sessionCards.length === 0) return <>
    <PageHeader title="No quiz cards available" meta={title} />
    <p className="text-sm text-text-secondary">This deck contains recall cards. Review them as flashcards from the deck.</p>
    <Button className="mt-8" onClick={onClose}>Back to deck</Button>
  </>;
  // A session ended early counts only the cards it reached.
  if (!card) return <Summary queue={queue} mode={mode} total={ended ? new Set(queue.attempts.map(attempt => attempt.cardId)).size : total} title={title} savedDue={savedDue} targetAt={targetAt} onClose={onClose} />;

  const confidentMiss = result?.rating === 'again' && confidence === 'sure';
  return <>
    <PageHeader title={mode === 'quiz' ? 'Practice quiz' : 'Flashcards'} meta={`${Math.min(position, total)} of ${total} · ${title}${attemptNumber > 0 ? ' · relearning' : ''}`} actions={<Button variant="ghost" disabled={review.isPending} onClick={() => queue.attempts.length ? setEnded(true) : onClose()}>End session</Button>} />
    <div className="mb-8 h-1 overflow-hidden rounded-full bg-surface-raised" role="progressbar" aria-label="Session progress" aria-valuenow={done} aria-valuemin={0} aria-valuemax={total}><div className="h-full bg-accent transition-all duration-fast" style={{ width: `${done / total * 100}%` }} /></div>
    {attemptNumber > 0 && !revealed && <p className="mb-4 text-xs text-text-muted">Back again: you missed this earlier in the session. Answer it from memory once more.</p>}
    {(revealed || seconds > 0) && <p className="mb-4 text-xs text-text-muted">{card.topic}</p>}
    <div role="heading" aria-level={2} className="font-serif text-2xl leading-relaxed text-text-primary"><CardText text={card.question} /></div>
    {mode === 'flashcard' && !revealed && seconds > 0 && <AnswerClock key={`${card.id}:${attemptNumber}`} seconds={seconds} />}
    {mode === 'flashcard' && !revealed && <label className="mt-6 block text-xs text-text-muted">
      Your answer <span className="text-text-muted">· committing to one before you look is what makes it stick</span>
      <textarea value={attemptText} onChange={event => setAttemptText(event.target.value)} rows={seconds > 0 ? 6 : 3} maxLength={6000} placeholder={seconds > 0 ? 'Say it aloud, or type the outline of your answer…' : 'Type what you remember…'} className="mt-2 w-full resize-y rounded-md border border-border-subtle bg-background px-3 py-2.5 text-sm leading-6 text-text-primary outline-hidden focus:border-accent" />
    </label>}
    {mode === 'quiz' && <fieldset className="mt-8 space-y-2" disabled={review.isPending || revealed}>
      <legend className="sr-only">Choose an answer</legend>
      {card.options.map((option, i) => <label key={i} className={cn('flex cursor-pointer items-start gap-3 rounded-md border p-4 text-sm leading-relaxed text-text-primary', selected === i ? 'border-accent bg-accent-muted' : 'border-border-subtle', revealed && i === card.correctIndex && 'border-accent')}>
        <input type="radio" name="study-answer" checked={selected === i} onChange={() => setSelected(i)} className="mt-1 accent-accent" />
        <span><span className="mr-2 text-text-muted">{String.fromCharCode(65 + i)}.</span>{' '}{option}{revealed && i === card.correctIndex && <span className="ml-2 font-medium text-accent">Correct answer</span>}</span>
      </label>)}
    </fieldset>}
    {revealed && <div className="mt-8 border-t border-border-subtle pt-6" aria-live="polite">
      {attemptText.trim() && <div className="mb-6 rounded-md bg-surface-raised p-4"><h3 className="mb-2 text-xs font-medium uppercase tracking-wider text-text-muted">Your answer</h3><p className="whitespace-pre-wrap text-sm leading-relaxed text-text-secondary">{attemptText.trim()}</p></div>}
      <h3 className="mb-3 text-sm font-medium text-text-secondary">{mode === 'quiz' ? result?.rating === 'good' ? 'Correct' : 'Review this answer' : points.length ? 'What a strong answer covers' : 'Answer'}</h3>
      {mode === 'flashcard' && points.length > 0 && <fieldset disabled={review.isPending || Boolean(result)}>
        <legend className="mb-3 text-xs text-text-muted">Tick each point your answer covered.</legend>
        <ul className="space-y-2">{points.map((point, index) => <li key={index}><label className="flex cursor-pointer items-start gap-3 rounded-md border border-border-subtle p-3 text-sm leading-relaxed text-text-primary">
          <input type="checkbox" checked={covered.has(index)} onChange={() => setCovered(current => { const next = new Set(current); if (next.has(index)) next.delete(index); else next.add(index); return next; })} className="mt-1 accent-accent" />
          <CardText text={point} />
        </label></li>)}</ul>
      </fieldset>}
      {mode === 'flashcard' && points.length === 0 && <CardText className="mb-4 text-lg leading-relaxed text-text-primary" text={card.answer} />}
      {card.explanation && <details className="mt-5" open={points.length === 0 || mode === 'quiz'}><summary className="cursor-pointer text-sm text-text-secondary">{points.length ? 'The full guide' : 'Explanation'}</summary><CardText className="mt-3 text-sm leading-relaxed text-text-secondary" text={card.explanation} /></details>}
      <FlashcardSource sources={card.citations?.length ? card.citations : [card.source]} />
      {confidentMiss && <div role="note" className="mt-6 rounded-xl border border-accent/40 bg-accent/5 p-4 text-sm leading-relaxed text-text-primary"><strong>You were sure, and it was wrong.</strong> This is the kind of error that is easiest to fix right now: read the answer once more and note exactly where yours went off. It will come back later in this session.</div>}
    </div>}
    {review.isError && <p role="alert" className="mt-4 text-sm text-danger-fg">Couldn’t save this review: {review.error.message}</p>}
    <div className="mt-8 flex flex-wrap items-center justify-end gap-2">
      {!revealed && <>
        <span className="mr-auto text-xs text-text-muted">{mode === 'quiz' && selected === null ? 'Choose an answer, then say how sure you are.' : 'How sure are you? Choose one to see the answer.'}</span>
        {CONFIDENCES.map((level, index) => <Button key={level} variant={level === 'unsure' ? 'default' : 'secondary'} disabled={review.isPending || (mode === 'quiz' && selected === null)} onClick={() => reveal(level)} aria-label={`${mode === 'quiz' ? 'Check answer' : 'Reveal answer'}: ${CONFIDENCE_LABEL[level]}`}>
          <span className="mr-1.5 text-[10px] opacity-60">{index + 1}</span>{CONFIDENCE_LABEL[level]}
        </Button>)}
      </>}
      {mode === 'flashcard' && revealed && !result && <>
        {suggestion && <span className="mr-auto text-xs text-text-muted">You covered {covered.size} of {points.length}: that suggests {capitalized(suggestion)}.</span>}
        {RATINGS.map(({ rating, hint }, index) => <Button key={rating} variant={rating === (suggestion ?? 'good') ? 'default' : 'secondary'} disabled={review.isPending} onClick={() => rate(rating)} title={`${capitalized(rating)}: ${hint} (${index + 1})`}>{capitalized(rating)}</Button>)}
      </>}
      {mode === 'flashcard' && result && <Button onClick={() => advance(result.saved, result.rating)}>Continue</Button>}
      {mode === 'quiz' && revealed && result && <Button onClick={() => advance(result.saved, result.rating)}>{queue.pending.length === 1 && result.rating === 'good' ? 'Finish practice' : 'Next question'}</Button>}
    </div>
  </>;
}
