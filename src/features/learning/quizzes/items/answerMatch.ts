/**
 * The same forgiving comparison the grader applies to short answers and cloze
 * blanks: case, spacing, punctuation, accents, a leading article and thousands
 * separators do not count. Used only to point at which blank was right.
 */
export function normaliseAnswer(value: string): string {
  return value
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/(\d),(?=\d{3}\b)/g, '$1')
    .replace(/[^\p{L}\p{N}\s.]/gu, ' ')
    .replace(/(?<!\d)\.|\.(?!\d)/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^(a|an|the) /, '');
}

/**
 * Which blanks were right, when the item's score allows saying so. A blank
 * that matches the shown key is right; the rest are wrong only when the count
 * of right blanks the score implies is already accounted for. Otherwise the
 * blank is "unknown": another accepted spelling may have matched.
 */
export function blankStates(responses: string[], keys: string[], earnedShare: number | null): ('right' | 'wrong' | 'unknown')[] {
  const matched = keys.map((key, index) => normaliseAnswer(responses[index] ?? '') === normaliseAnswer(key));
  if (earnedShare === null) return matched.map((hit) => (hit ? 'right' : 'unknown'));
  if (earnedShare >= 1) return keys.map(() => 'right');
  const rightCount = Math.round(earnedShare * keys.length);
  const accounted = matched.filter(Boolean).length === rightCount;
  return matched.map((hit, index) => (hit ? 'right' : accounted || !(responses[index] ?? '').trim() ? 'wrong' : 'unknown'));
}
