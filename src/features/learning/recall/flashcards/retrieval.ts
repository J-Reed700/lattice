/**
 * The study session's order and bookkeeping, kept free of React so it can be
 * tested. Each choice follows the learning research the session is built on:
 *
 * - Retrieval first, then feedback: an attempt from memory before the answer is
 *   shown is what makes a card stick (Roediger & Karpicke 2006), and feedback
 *   after it roughly doubles the gain (Rowland 2014).
 * - Successive relearning: a missed card comes back later in the same session
 *   until it is recalled once, then again in later sessions (Rawson &
 *   Dunlosky 2011; one correct recall in each of three spaced sessions kept
 *   68% after a week against 26% for three in one session, Rawson et al. 2018).
 * - Interleaving: topics are mixed rather than blocked, so each card also
 *   asks which kind of question it is (Brunmair & Richter 2019, g = 0.42).
 * - Confidence before the answer: an error made with confidence is the one
 *   most often fixed once corrected (the hypercorrection effect, Butterfield &
 *   Metcalfe 2001), so those are named, and comparing confidence with results
 *   shows where the learner over- or under-trusts their memory.
 */
import type { StudyCardDto, StudyRating } from '@/lib/bindings';

export type Confidence = 'guess' | 'unsure' | 'sure';

export const CONFIDENCE_LABEL: Record<Confidence, string> = {
  guess: 'Guessing',
  unsure: 'Not sure',
  sure: 'I know this',
};

/** A missed card comes back after this many other cards, so the recall is not just short-term echo. */
export const RELEARN_GAP = 3;
/** Attempts per card per session before it is left for the next session. */
export const MAX_ATTEMPTS = 4;

export interface Attempt {
  cardId: string;
  topic: string;
  question: string;
  rating: StudyRating;
  confidence: Confidence | null;
  /** The card's first attempt in this session. */
  firstTry: boolean;
}

export interface SessionQueue {
  /** Cards still to answer, next first. */
  pending: StudyCardDto[];
  attempts: Attempt[];
}

/** Topics in turn, each topic's cards in their given order. */
export function interleaveByTopic(cards: StudyCardDto[]): StudyCardDto[] {
  const groups = new Map<string, StudyCardDto[]>();
  for (const card of cards) {
    const key = card.topic.trim() || card.deckId;
    const group = groups.get(key) ?? [];
    group.push(card);
    groups.set(key, group);
  }
  const lists = [...groups.values()];
  const result: StudyCardDto[] = [];
  for (let round = 0; result.length < cards.length; round++) {
    for (const list of lists) if (round < list.length) result.push(list[round]);
  }
  return result;
}

export function startQueue(cards: StudyCardDto[]): SessionQueue {
  const seen = new Set<string>();
  return { pending: cards.filter(card => !seen.has(card.id) && seen.add(card.id)), attempts: [] };
}

export function attemptsFor(queue: SessionQueue, cardId: string): number {
  return queue.attempts.filter(attempt => attempt.cardId === cardId).length;
}

/** A recall that failed: Again on a flashcard, or a wrong quiz choice. */
export function isMiss(rating: StudyRating): boolean {
  return rating === 'again';
}

/**
 * The queue after the current card was answered. `saved` is the card as the
 * backend returned it, so a relearning attempt sends its new review count.
 */
export function answer(queue: SessionQueue, saved: StudyCardDto, rating: StudyRating, confidence: Confidence | null): SessionQueue {
  const [current, ...rest] = queue.pending;
  if (!current) return queue;
  const tries = attemptsFor(queue, current.id);
  const attempts = [...queue.attempts, { cardId: current.id, topic: current.topic, question: current.question, rating, confidence, firstTry: tries === 0 }];
  if (!isMiss(rating) || tries + 1 >= MAX_ATTEMPTS) return { pending: rest, attempts };
  const at = Math.min(RELEARN_GAP, rest.length);
  return { pending: [...rest.slice(0, at), saved, ...rest.slice(at)], attempts };
}

export interface SessionSummary {
  cards: number;
  recalledFirstTry: number;
  /** Missed at first, then recalled later in the session. */
  relearned: number;
  /** Still missed after every attempt this session. */
  stillMissed: string[];
  /** Questions answered wrongly with confidence: the best ones to study now. */
  confidentErrors: string[];
  /** How often each confidence level was right on a first try. */
  calibration: { confidence: Confidence; right: number; total: number }[];
}

export function summarize(attempts: Attempt[]): SessionSummary {
  const byCard = new Map<string, Attempt[]>();
  for (const attempt of attempts) byCard.set(attempt.cardId, [...(byCard.get(attempt.cardId) ?? []), attempt]);
  let recalledFirstTry = 0;
  let relearned = 0;
  const stillMissed: string[] = [];
  const confidentErrors: string[] = [];
  for (const cardAttempts of byCard.values()) {
    const first = cardAttempts[0];
    const last = cardAttempts[cardAttempts.length - 1];
    if (!isMiss(first.rating)) recalledFirstTry++;
    else if (!isMiss(last.rating)) relearned++;
    else stillMissed.push(first.question);
    if (cardAttempts.some(attempt => attempt.confidence === 'sure' && isMiss(attempt.rating))) confidentErrors.push(first.question);
  }
  const firsts = attempts.filter(attempt => attempt.firstTry && attempt.confidence);
  const calibration = (['sure', 'unsure', 'guess'] as const)
    .map(confidence => {
      const rated = firsts.filter(attempt => attempt.confidence === confidence);
      return { confidence, right: rated.filter(attempt => !isMiss(attempt.rating)).length, total: rated.length };
    })
    .filter(row => row.total > 0);
  return { cards: byCard.size, recalledFirstTry, relearned, stillMissed, confidentErrors, calibration };
}

/**
 * Key points of an answer written as a Markdown list, one per item, so an
 * attempt can be checked point by point. Empty when the answer is prose.
 */
export function keyPoints(answer: string): string[] {
  const items = answer.split('\n').map(line => /^\s*(?:[-*+•]|\d+[.)])\s+(.+)$/.exec(line)?.[1]?.trim()).filter((item): item is string => Boolean(item));
  return items.length >= 2 ? items : [];
}

/** The rating the ticked key points suggest: most covered is a recall, few is a miss. */
export function suggestedRating(covered: number, total: number): StudyRating | null {
  if (total === 0) return null;
  const share = covered / total;
  if (share >= 0.85) return 'good';
  if (share >= 0.5) return 'hard';
  return 'again';
}

/** Seconds a learner should aim to answer in aloud: short for facts, longer for interview answers. */
export function answerSeconds(card: StudyCardDto): number {
  const points = keyPoints(card.answer).length;
  if (points === 0 && card.answer.length < 160) return 0;
  return /system design/i.test(card.topic) ? 300 : 120;
}

/** "in 3 h", "in 25 min", "now": when a time is from `now`, for a next-session hint. */
export function fromNow(at: number, now: number): string {
  const minutes = Math.round((at - now) / 60_000);
  if (minutes <= 1) return 'now';
  if (minutes < 90) return `in ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 36) return `in ${hours} h`;
  return `in ${Math.round(hours / 24)} days`;
}

export interface PlanStep {
  at: number;
  title: string;
  detail: string;
}

/**
 * The sessions to fit in before a target such as an interview: the first pass
 * now, then each time cards come due, one before sleep (sleep between sessions
 * makes relearning faster and longer lasting, Mazza et al. 2016), and a short
 * warm-up of retrieval only, with no new material, before the target.
 */
export function prepPlan(now: number, target: number, nextDue: number | null): PlanStep[] {
  const left = target - now;
  if (left <= 0) return [];
  const hour = 3_600_000;
  const steps: PlanStep[] = [{ at: now, title: 'First pass, now', detail: 'Answer every card from memory before you look. Misses come back until you recall each one once.' }];
  const evening = new Date(target);
  evening.setDate(evening.getDate() - 1);
  evening.setHours(21, 30, 0, 0);
  const sleepSession = evening.getTime() > now + hour && evening.getTime() < target - 8 * hour;
  // Cards due within two hours of the session before sleep are reviewed in it.
  if (nextDue && nextDue > now && nextDue < target - 2 * hour && !(sleepSession && Math.abs(nextDue - evening.getTime()) < 2 * hour)) {
    steps.push({ at: nextDue, title: 'Next session, when cards come due', detail: 'Only the cards that are due. Hours between sessions, not minutes, is what makes recall last.' });
  }
  if (sleepSession) {
    steps.push({ at: evening.getTime(), title: 'Before you sleep', detail: 'A short session on what is due. Sleep after it consolidates what you just recalled.' });
  }
  if (left > 3 * hour) {
    steps.push({ at: target - 2 * hour, title: 'Warm-up', detail: 'Ten minutes: say your key answers and stories out loud. Nothing new.' });
  }
  steps.push({ at: target, title: 'Target', detail: 'Stop drilling in the last hour. Rest, read your notes once, and arrive early.' });
  return steps.sort((a, b) => a.at - b.at);
}
