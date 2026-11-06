import { clearDraft, readDraft, setDraft } from '@/shared/drafts/drafts';

import { SEVERITIES } from './severity';

import type { DraftFinding } from './findings';

/** What the learner has written of a review that is not handed in yet, kept with the unsent text of the app. */
export interface ReviewDraft {
  findings: DraftFinding[];
  /** The text of each drill file the learner changed, by path. */
  fix: Record<string, string>;
  /** The interview's time limit, so a review reopened later still counts down from its start. */
  minutes: number | null;
}

export const EMPTY_DRAFT: ReviewDraft = { findings: [], fix: {}, minutes: null };

const keyOf = (attemptId: string) => `code-review:${attemptId}`;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isFinding(value: unknown): value is DraftFinding {
  if (!isRecord(value) || typeof value.id !== 'string' || typeof value.text !== 'string') return false;
  const { span, severity } = value;
  const spanOk =
    span === null ||
    (isRecord(span) && typeof span.path === 'string' && typeof span.start === 'number' && typeof span.end === 'number');
  const severityOk = severity === null || SEVERITIES.some((known) => known === severity);
  return spanOk && severityOk;
}

export function writeReviewDraft(attemptId: string, draft: ReviewDraft): void {
  const empty = draft.findings.length === 0 && Object.keys(draft.fix).length === 0 && draft.minutes === null;
  setDraft(keyOf(attemptId), empty ? '' : JSON.stringify(draft));
}

/** The draft kept for an attempt; an empty one when there is none or it cannot be read. */
export function readReviewDraft(attemptId: string): ReviewDraft {
  const stored = readDraft(keyOf(attemptId));
  if (!stored) return EMPTY_DRAFT;
  try {
    const parsed: unknown = JSON.parse(stored);
    if (!isRecord(parsed)) return EMPTY_DRAFT;
    const fix = isRecord(parsed.fix)
      ? Object.fromEntries(Object.entries(parsed.fix).filter((entry): entry is [string, string] => typeof entry[1] === 'string'))
      : {};
    return {
      findings: Array.isArray(parsed.findings) ? parsed.findings.filter(isFinding) : [],
      fix,
      minutes: typeof parsed.minutes === 'number' ? parsed.minutes : null,
    };
  } catch {
    return EMPTY_DRAFT;
  }
}

export function clearReviewDraft(attemptId: string): void {
  clearDraft(keyOf(attemptId));
}
