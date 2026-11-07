import type {
  AttemptMode,
  CodeReviewSpanDto,
  DecidedBy,
  FindingResultKind,
  FixOutcome,
  IssueResultKind,
  PatternRating,
} from '@/lib/bindings';

export interface Words {
  label: string;
  tone: string;
}

const GOOD = 'border-transparent bg-success-muted text-success-fg';
const MIDDLING = 'border-transparent bg-warning-muted text-warning-fg';
const BAD = 'border-transparent bg-danger-muted text-danger-fg';
const NOTE = 'border-transparent bg-accent-muted text-accent';

export const MODE_LABEL: Record<AttemptMode, string> = {
  practice: 'Practice',
  interview: 'Interview',
  rapid: 'Rapid',
};

export const ISSUE_OUTCOME: Record<IssueResultKind, Words> = {
  found: { label: 'Found', tone: GOOD },
  half_found: { label: 'Half found', tone: MIDDLING },
  missed: { label: 'Missed', tone: BAD },
};

export const ALSO_VALID_NOT_RAISED: Words = { label: 'Not raised', tone: NOTE };

export const FINDING_OUTCOME: Record<FindingResultKind, Words> = {
  matches: { label: 'Matches', tone: GOOD },
  valid_extra: { label: 'Valid extra', tone: NOTE },
  false_positive: { label: 'False positive', tone: BAD },
};

export const FIX_OUTCOME: Record<FixOutcome, Words> = {
  fixed: { label: 'Fixed', tone: GOOD },
  partly: { label: 'Partly', tone: MIDDLING },
  not_fixed: { label: 'Not fixed', tone: BAD },
};

export const DECIDED_BY: Record<DecidedBy, string> = {
  test: 'the issue’s test',
  suite: 'the hidden suite',
  judge: 'a judge',
};

export const RATING: Record<PatternRating, Words> = {
  again: { label: 'Again', tone: BAD },
  hard: { label: 'Hard', tone: MIDDLING },
  good: { label: 'Good', tone: GOOD },
  easy: { label: 'Easy', tone: GOOD },
};

export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

export function joinList(items: readonly string[]): string {
  if (items.length <= 1) return items.join('');
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}

export function percentText(share: number): string {
  return `${Math.round(share * 100)}%`;
}

export function durationText(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes < 60) return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`;
  const hours = Math.floor(minutes / 60);
  const left = minutes % 60;
  return left === 0 ? `${hours} h` : `${hours} h ${left} min`;
}

/** A moment as the reader's clock writes it, and as machines read it; none for a number that is no time. */
export function momentOf(ms: number): { text: string; iso: string } | null {
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return null;
  return {
    text: date.toLocaleString([], { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' }),
    iso: date.toISOString(),
  };
}

export function lineLabel(span: CodeReviewSpanDto): string {
  return span.start === span.end
    ? `${span.path} · line ${span.start}`
    : `${span.path} · lines ${span.start}–${span.end}`;
}
