import type { LearningCriterionRationaleDto } from '@/lib/bindings';

export type AnswerSegment = { text: string; quote?: number };

export type NumberedQuotes = {
  /** The answer cut into plain text and numbered quotes, in reading order. */
  segments: AnswerSegment[];
  /** Quotes that could not be placed inline (overlapping or not found), listed under the answer. */
  loose: { quote: number; text: string }[];
  /** Per criterion, in criterion order, the numbers of the quotes it rests on. */
  byCriterion: number[][];
};

const collapse = (value: string) => value.replace(/\s+/g, ' ').trim();

function locate(answer: string, span: string): { start: number; end: number } | null {
  const exact = answer.indexOf(span);
  if (exact >= 0) return { start: exact, end: exact + span.length };
  // The grader checks quotes after whitespace normalisation, so match the same way.
  const pattern = collapse(span).split(' ').map((word) => word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('\\s+');
  if (!pattern) return null;
  const match = new RegExp(pattern).exec(answer);
  return match ? { start: match.index, end: match.index + match[0].length } : null;
}

/**
 * Numbers the learner's quoted words in reading order (1, 2, 3 as they appear
 * in the answer), so each criterion can say "rests on 1 and 3" and the
 * highlight carries the same number. A quote two criteria share keeps one number.
 */
export function numberQuotes(answer: string, criteria: Pick<LearningCriterionRationaleDto, 'yourWords'>[]): NumberedQuotes {
  const unique: string[] = [];
  for (const criterion of criteria) {
    for (const span of criterion.yourWords) {
      const key = collapse(span);
      if (key && !unique.some((seen) => collapse(seen) === key)) unique.push(span);
    }
  }
  const placed = unique.map((text, order) => ({ text, order, at: locate(answer, text) }));
  const inReadingOrder = [...placed].sort((a, b) => (a.at?.start ?? Number.MAX_SAFE_INTEGER) - (b.at?.start ?? Number.MAX_SAFE_INTEGER) || a.order - b.order);
  const numberOf = new Map<string, number>();
  inReadingOrder.forEach((entry, index) => numberOf.set(collapse(entry.text), index + 1));

  const segments: AnswerSegment[] = [];
  const loose: NumberedQuotes['loose'] = [];
  let cursor = 0;
  for (const entry of inReadingOrder) {
    const quote = numberOf.get(collapse(entry.text))!;
    if (!entry.at || entry.at.start < cursor) { loose.push({ quote, text: entry.text }); continue; }
    if (entry.at.start > cursor) segments.push({ text: answer.slice(cursor, entry.at.start) });
    segments.push({ text: answer.slice(entry.at.start, entry.at.end), quote });
    cursor = entry.at.end;
  }
  if (cursor < answer.length) segments.push({ text: answer.slice(cursor) });

  const byCriterion = criteria.map((criterion) => [...new Set(criterion.yourWords.map((span) => numberOf.get(collapse(span))).filter((n): n is number => n !== undefined))].sort((a, b) => a - b));
  return { segments, loose, byCriterion };
}

/** "1", "1 and 3", "1, 2 and 4". */
export function listNumbers(numbers: number[]): string {
  if (numbers.length <= 1) return numbers.join('');
  return `${numbers.slice(0, -1).join(', ')} and ${numbers[numbers.length - 1]}`;
}
