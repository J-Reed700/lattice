import {
  FIX_GROUP_LABELS,
  FIX_GROUP_ORDER,
  claimsCount,
  fixGroupOf,
  kindWord,
} from '@/features/learning/lessons/review/claimReviewCopy';
import type {
  ActorDto,
  CheckerVerdictDto,
  CheckResultDto,
  ClaimEvidencePassageDto,
  ClaimEventPayloadDto,
  ClaimLocationDto,
  ClaimReviewItemDto,
  ClaimSupportDto,
  LearningGenerationJob,
  LearningSourceKind,
  LessonClaimReviewDto,
  RepairOutcomeDto,
  ReopenCauseDto,
  SourceAddedByDto,
  SourceRefDto,
  UserActionDto,
  VerdictKindDto,
} from '@/lib/bindings';

/** The job's result reference for lessons held for review (§6.1). */
export const NEEDS_REVIEW_RESULT_PREFIX = 'needs_review:';

/** Where a claim stands for the user: theirs to act on, the machine's, or settled. */
export function claimFilterOf(item: ClaimReviewItemDto): 'needs_you' | 'repairing' | 'machine' | 'confirmed' | 'general' | 'changed' {
  const state = item.state;
  switch (state.state) {
    case 'queued': return 'needs_you';
    case 'unchecked':
    case 'open': return 'repairing';
    case 'resolved_by_evidence': return 'machine';
    case 'removed': return state.by === 'machine' ? 'machine' : 'changed';
    case 'confirmed': return 'confirmed';
    case 'general_knowledge': return 'general';
    case 'edited': return 'changed';
    case 'reopened': return 'needs_you';
  }
}

export function isWaitingForYou(item: ClaimReviewItemDto): boolean {
  return claimFilterOf(item) === 'needs_you';
}

/** Where a claim sits, in words: "Section 3 · Phase changes", "Question · answer key". */
export function unitLabel(location: ClaimLocationDto): string {
  const title = location.unitTitle.trim();
  switch (location.unitKind) {
    case 'block': return `Section ${location.unitIndex + 1}${title ? ` · ${title}` : ''}`;
    case 'question': return `Question${title ? ` · ${title}` : ''}`;
    case 'answer_key': return `Question${title ? ` · ${title}` : ''} · answer key`;
  }
}

// ── The queue's groups (§3.2) ───────────────────────────────────────────────

export type GroupMode = 'source' | 'section' | 'fix';

export const GROUP_MODE_LABELS: Record<GroupMode, string> = { source: 'Source', section: 'Section', fix: 'What fixes it' };

export type ClaimGroup = {
  key: string;
  label: string;
  /** A second line under the label: the source's site and kind, or a count. */
  detail: string | null;
  claims: ClaimReviewItemDto[];
  /** The source every claim of a source group rests on. */
  source: SourceRefDto | null;
};

/** Claims grouped by unit, in lesson order (blocks, then questions). */
export function groupBySection(claims: ClaimReviewItemDto[]): ClaimGroup[] {
  const sections = new Map<string, ClaimGroup & { index: number }>();
  for (const claim of claims) {
    const { unitIndex, unitKind } = claim.location;
    // A question and its answer key share a unit; the key reads as part of it.
    const key = String(unitIndex);
    const section = sections.get(key) ?? { key, index: unitIndex, label: unitLabel({ ...claim.location, unitKind: unitKind === 'answer_key' ? 'question' : unitKind }), detail: null, claims: [], source: null };
    section.claims.push(claim);
    sections.set(key, section);
  }
  return [...sections.values()].sort((a, b) => a.index - b.index).map(({ index: _index, ...group }) => group);
}

const supportOf = (item: ClaimReviewItemDto): ClaimSupportDto | null => (item.support?.role === 'supports' ? item.support : null);

export const NO_SOURCE_GROUP = 'No source states it';

/**
 * A group per support source, the largest first, titled by the source; the
 * claims no source states form the last group (§3.2).
 */
export function groupBySource(claims: ClaimReviewItemDto[]): ClaimGroup[] {
  const groups = new Map<string, ClaimGroup>();
  const none: ClaimReviewItemDto[] = [];
  for (const claim of claims) {
    const support = supportOf(claim);
    if (!support) { none.push(claim); continue; }
    const key = support.source.sourceId;
    const group = groups.get(key) ?? {
      key: `source:${key}`, label: support.source.title,
      detail: [support.site, kindWord(support.kind)].filter(Boolean).join(' · '),
      claims: [], source: support.source,
    };
    group.claims.push(claim);
    groups.set(key, group);
  }
  const ordered = [...groups.values()].sort((a, b) => b.claims.length - a.claims.length);
  return none.length ? [...ordered, { key: 'source:none', label: NO_SOURCE_GROUP, detail: null, claims: none, source: null }] : ordered;
}

/** Claims grouped by what fixes them (§4.2). */
export function groupByFix(claims: ClaimReviewItemDto[]): ClaimGroup[] {
  return FIX_GROUP_ORDER
    .map((fix) => ({ key: `fix:${fix}`, label: FIX_GROUP_LABELS[fix], detail: null, claims: claims.filter((claim) => fixGroupOf(claim) === fix), source: null }))
    .filter((group) => group.claims.length > 0);
}

export function groupClaims(claims: ClaimReviewItemDto[], mode: GroupMode): ClaimGroup[] {
  if (mode === 'source') return groupBySource(claims);
  if (mode === 'fix') return groupByFix(claims);
  return groupBySection(claims);
}

/** By source when two or more waiting claims share a support source, else by section. */
export function defaultGroupMode(claims: ClaimReviewItemDto[]): GroupMode {
  const seen = new Set<string>();
  for (const claim of claims) {
    const source = supportOf(claim)?.source.sourceId;
    if (!source) continue;
    if (seen.has(source)) return 'source';
    seen.add(source);
  }
  return 'section';
}

export const groupCountLabel = (group: ClaimGroup) => claimsCount(group.claims.length);

// ── The evidence excerpt (§3.4) ─────────────────────────────────────────────

/** A UTF-8 byte offset into `text` as a string index. */
export function byteToIndex(text: string, byte: number): number {
  const encoder = new TextEncoder();
  let bytes = 0;
  let index = 0;
  for (const char of text) {
    if (bytes >= byte) break;
    bytes += encoder.encode(char).length;
    index += char.length;
  }
  return index;
}

/**
 * Sentence bounds as the backend finds them: a sentence ends at a line
 * break, or at '.', '!' or '?' before whitespace or the end.
 */
export function sentenceBounds(text: string): Array<[number, number]> {
  const bounds: Array<[number, number]> = [];
  let start = 0;
  const push = (from: number, to: number) => {
    const piece = text.slice(from, to);
    const lead = piece.length - piece.trimStart().length;
    const trimmed = piece.trim();
    if (trimmed) bounds.push([from + lead, from + lead + trimmed.length]);
  };
  for (let at = 0; at < text.length; at += 1) {
    const ch = text[at];
    const next = text[at + 1];
    if (ch === '\n' || ('.!?'.includes(ch) && (next === undefined || /\s/.test(next)))) {
      push(start, at + 1);
      start = at + 1;
    }
  }
  push(start, text.length);
  return bounds;
}

export type Excerpt = { before: string; quote: string; after: string; clippedStart: boolean; clippedEnd: boolean };

/** The spans of fenced code blocks (``` or ~~~), fences included; an unclosed fence runs to the end. */
function fencedRanges(text: string): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  let open = -1;
  for (const match of text.matchAll(/^[ \t]*(?:```|~~~)/gm)) {
    if (open < 0) { open = match.index; continue; }
    const lineEnd = text.indexOf('\n', match.index);
    ranges.push([open, lineEnd < 0 ? text.length : lineEnd]);
    open = -1;
  }
  if (open >= 0) ranges.push([open, text.length]);
  return ranges;
}

/**
 * The quoted sentence with up to one sentence of context on each side, from
 * the passage. Falls back to the quote alone when it is not found there.
 */
export function excerptAround(text: string, span: [number, number] | null, quote: string): Excerpt {
  let from = -1;
  let to = -1;
  if (span) { from = byteToIndex(text, span[0]); to = byteToIndex(text, span[1]); }
  if (from < 0 || to <= from || text.slice(from, to).trim() !== quote.trim()) {
    from = text.indexOf(quote);
    to = from + quote.length;
  }
  if (from < 0) return { before: '', quote, after: '', clippedStart: false, clippedEnd: false };
  // Context is the neighbouring sentence when it is prose: never code, a fence
  // or a rule line, which stay behind Full passage.
  const code = fencedRanges(text);
  const prose = ([start, end]: [number, number]) => /\w+\W+\w+/.test(text.slice(start, end)) && !code.some(([open, close]) => start < close && end > open);
  const bounds = sentenceBounds(text);
  const first = bounds.findIndex(([, end]) => end > from);
  const last = bounds.findIndex(([, end]) => end >= to);
  const previous = first > 0 ? bounds[first - 1] : null;
  const next = last >= 0 && last + 1 < bounds.length ? bounds[last + 1] : null;
  const startAt = previous && prose(previous) ? previous[0] : first >= 0 ? bounds[first][0] : from;
  const endAt = next && prose(next) ? next[1] : last >= 0 ? bounds[last][1] : to;
  return {
    before: text.slice(Math.min(startAt, from), from),
    quote: text.slice(from, to),
    after: text.slice(to, Math.max(endAt, to)),
    clippedStart: Math.min(startAt, from) > 0,
    clippedEnd: Math.max(endAt, to) < text.length,
  };
}

// ── Actions and history ─────────────────────────────────────────────────────

/** Contradicted claims, local or by the Verifier, need a note to accept (D3). */
export function confirmNeedsNote(item: ClaimReviewItemDto): boolean {
  const kind = item.verdictKind;
  if (!kind) return false;
  return kind.kind === 'contradicted' || (kind.kind === 'disputed' && kind.verifier.kind === 'contradicted');
}

const ADDED_BY: Record<SourceAddedByDto, string> = {
  user: 'Added by you',
  research: 'Found by research',
  outline: 'Saved with the course',
  import: 'Imported',
  verifier: 'Found by the Verifier',
};

export function addedByLabel(addedBy: SourceAddedByDto | null, kind?: LearningSourceKind): string | null {
  if (!addedBy) return null;
  if (addedBy === 'user' && kind === 'pasted') return 'Pasted by you';
  return ADDED_BY[addedBy];
}

const REOPEN_CAUSES: Record<ReopenCauseDto['cause'], string> = {
  sentence_changed: 'Reopened: the sentence changed',
  source_deleted: 'Reopened: its source was deleted',
  source_replaced: 'Reopened: its source was replaced',
  unconfirmed: 'Reopened: you took back your acceptance',
  verifier_dispute: 'Reopened: an independent check disagrees',
  trust_changed: 'Reopened: how far a source is trusted changed',
};

/** A verdict in words, for the history and the checks. */
function verdictWords(kind: VerdictKindDto): string {
  switch (kind.kind) {
    case 'unsupported': return 'Not stated by the passages compared';
    case 'contradicted': return 'A source says otherwise';
    case 'no_source': return 'No saved source mentions it';
    case 'could_not_judge': return 'Could not check';
    case 'general_knowledge_proposed': return 'Proposed as common knowledge';
    case 'disputed': return `Disputed by an independent check · ${verdictWords(kind.verifier).toLowerCase()}`;
    case 'insufficient_standing': return 'Stated, but not by a source enough on its own';
  }
}

export function repairOutcomeLabel(outcome: RepairOutcomeDto): string {
  switch (outcome.outcome) {
    case 'rewritten': return 'Rewritten, then checked again';
    case 'removed': return 'The repair removed the sentence';
    case 'rejected': return `Repair rejected: ${outcome.reason}`;
    case 'superseded_by_user': return 'Your change superseded this repair';
  }
}

const ACTOR: Record<ActorDto, string> = { machine: 'Machine', user: 'You' };
export const actorLabel = (actor: ActorDto) => ACTOR[actor];

function describeUserAction(action: UserActionDto, note: string | null): string {
  const quoted = note ? ` · “${note}”` : '';
  switch (action.action) {
    case 'confirm': return `Accepted${quoted}`;
    case 'unconfirm': return `Acceptance withdrawn${quoted}`;
    case 'edit': return `Edited: “${action.before}” → “${action.after}”`;
    case 'remove': return 'Removed the sentence';
    case 'general_knowledge': return 'Accepted as common knowledge';
    case 'research_with_hint': return `Asked for a search: “${action.hint}”`;
  }
}

/** A check's verdict in words. A claim the course supports elsewhere than
 *  its section's citation stands, and its citation was moved to the passage
 *  found. */
function checkVerdict(result: CheckResultDto): string {
  switch (result.verdict) {
    case 'supported': return 'Supported';
    case 'supported_elsewhere': return 'Supported elsewhere, citation repaired';
    case 'failed': return verdictWords(result.kind);
  }
}

/** One history row in words. */
export function describeEvent(payload: ClaimEventPayloadDto): string {
  switch (payload.kind) {
    case 'extracted': return `Claim found in ${payload.field}: “${payload.statement}”`;
    case 'checked': {
      const who = payload.check.checker.role === 'verifier' ? 'Independent check' : 'Checker';
      const verdict = checkVerdict(payload.check.result).toLowerCase();
      return `${who} (${payload.check.checker.model}): ${verdict}${payload.check.reason ? ` · ${payload.check.reason}` : ''}`;
    }
    case 'researched': return payload.queries.length
      ? `Searched: ${payload.queries.join('; ')} · ${payload.passagesFound} ${payload.passagesFound === 1 ? 'passage' : 'passages'} found`
      : 'Research had nothing to search';
    case 'repair_attempted': return `Repair ${payload.attempt}${payload.escalated ? ' (last try)' : ''}: ${repairOutcomeLabel(payload.outcome)}`;
    case 'executed': return payload.passed ? 'Example ran as shown' : 'Example did not run as shown';
    case 'citation_repaired': return `Citation moved to the passage that supports it${payload.addedSource ? ', a source added to the lesson' : ''}: “${payload.quote}”`;
    case 'user_action': return describeUserAction(payload.action, payload.note);
    case 'evidence_attached': {
      const input = { saved_passage: 'a saved passage', link: 'a link', document: 'a file', pasted_text: 'pasted text' }[payload.provenance.input];
      const result = payload.result.outcome === 'no_passage' ? 'it does not mention the claim'
        : payload.result.check.result.verdict === 'failed' ? verdictWords(payload.result.check.result.kind).toLowerCase() : 'it states the claim';
      return `Added ${input}: ${result}`;
    }
    case 'superseded': return payload.successor ? 'Replaced by a re-extracted claim' : 'Removed by an edit';
    case 'born_of_repair': return 'Added while repairing another claim';
    case 'reopened': return REOPEN_CAUSES[payload.cause.cause];
    case 'undone': return `Undid step ${payload.eventOrdinal}`;
    case 'standing_assessed': {
      const { assessment } = payload;
      const decided = assessment.sufficient ? 'its source is enough on its own' : 'its source is not enough on its own';
      const why = assessment.cause ? (assessment.cause.change === 'override' ? ' after you changed how far a source is trusted' : ' after the course’s rules moved') : '';
      return `Weighed: ${decided}${why}`;
    }
    case 'stopped': return `Stopped: ${payload.reason.message}`;
  }
}

export type CheckerSide = {
  model: string; verdict: string; reason: string; supportingQuote: string | null;
  quotedSource: SourceRefDto | null; passages: ClaimEvidencePassageDto[];
};

function side(check: CheckerVerdictDto): CheckerSide {
  const result = check.result;
  return {
    model: check.checker.model,
    verdict: checkVerdict(result),
    reason: check.reason,
    supportingQuote: result.verdict === 'failed' ? null : result.supportingQuote,
    quotedSource: check.quotedSource,
    passages: check.passages,
  };
}

/** "First check / Independent check" (§14.5), once the Verifier has judged the claim. */
export function checkerColumns(item: ClaimReviewItemDto): { local: CheckerSide | null; verifier: CheckerSide } | null {
  if (!item.verifierCheck) return null;
  return { local: item.localCheck ? side(item.localCheck) : null, verifier: side(item.verifierCheck) };
}

// ── Holds and jobs ──────────────────────────────────────────────────────────

/** The code every Verifier audit stop starts with (`verifier_unset`, `verifier_turned_off`…). */
export const AUDIT_STOP_PREFIX = 'verifier_';

/** What holds a waiting lesson besides its claims: the audit stop, if any. */
export type ReviewHold = 'claims' | 'audit_stop' | 'audit' | 'resolved';

/**
 * Why a lesson in Needs review is held. With no claim waiting it is held by
 * the Verifier audit (its stop reason, or none recorded), unless the user has
 * resolved everything since the job stopped (`changedSinceStop`, §6.2).
 */
export function reviewHold(review: LessonClaimReviewDto, waiting: number): ReviewHold {
  if (waiting > 0) return 'claims';
  if (review.stopReason?.code.startsWith(AUDIT_STOP_PREFIX)) return 'audit_stop';
  return review.changedSinceStop ? 'resolved' : 'audit';
}

export const NOTHING_CHANGED = 'Nothing changed since the last check. Resolve a claim or add a source first.';

/** Lessons a completed job held for review, from its result reference. */
export function heldLessonIds(job: Pick<LearningGenerationJob, 'status' | 'resultId'> | null | undefined): string[] {
  if (job?.status !== 'completed' || !job.resultId?.startsWith(NEEDS_REVIEW_RESULT_PREFIX)) return [];
  return job.resultId.slice(NEEDS_REVIEW_RESULT_PREFIX.length).split(',').filter(Boolean);
}

/** The latest attempt of each lesson preparation job (a retry supersedes its parent). */
export function currentPreparationJobs(jobs: LearningGenerationJob[]): LearningGenerationJob[] {
  const superseded = new Set(jobs.map((job) => job.retryOfJobId).filter(Boolean));
  return jobs.filter((job) => job.kind === 'lesson_preparation' && !superseded.has(job.id));
}

/** The job that holds `lessonId` for review, if its latest attempt does. */
export function jobHoldingLesson(jobs: LearningGenerationJob[], lessonId: string): LearningGenerationJob | null {
  return currentPreparationJobs(jobs).find((job) => heldLessonIds(job).includes(lessonId)) ?? null;
}
