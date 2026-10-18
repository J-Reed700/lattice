import type {
  ActorDto,
  CheckerVerdictDto,
  ClaimEvidencePassageDto,
  ClaimActionKindDto,
  ClaimEventPayloadDto,
  ClaimLocationDto,
  ClaimReviewItemDto,
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

/** The filters of §10, in display order. */
export const CLAIM_FILTERS = ['needs_you', 'repairing', 'machine', 'confirmed', 'general', 'changed', 'all'] as const;
export type ClaimFilter = (typeof CLAIM_FILTERS)[number];

export const FILTER_LABELS: Record<ClaimFilter, string> = {
  needs_you: 'Needs you',
  repairing: 'Being repaired',
  machine: 'Resolved by the machine',
  confirmed: 'Confirmed by you',
  general: 'General knowledge',
  changed: 'Removed or edited',
  all: 'All',
};

/** Card actions in the fixed order of §10; History is always offered. */
export const ACTION_ORDER: ClaimActionKindDto[] = [
  'confirm', 'edit', 'remove', 'general_knowledge', 'attach_evidence', 'research_with_hint', 'unconfirm', 'undo',
];

export function claimFilterOf(item: ClaimReviewItemDto): Exclude<ClaimFilter, 'all'> {
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

export function filterCounts(claims: ClaimReviewItemDto[]): Record<ClaimFilter, number> {
  const counts = Object.fromEntries(CLAIM_FILTERS.map((filter) => [filter, 0])) as Record<ClaimFilter, number>;
  for (const claim of claims) counts[claimFilterOf(claim)] += 1;
  counts.all = claims.length;
  return counts;
}

export function applyFilter(claims: ClaimReviewItemDto[], filter: ClaimFilter): ClaimReviewItemDto[] {
  return filter === 'all' ? claims : claims.filter((claim) => claimFilterOf(claim) === filter);
}

/** The filter to open on: the user's work first, else everything. */
export function initialFilter(claims: ClaimReviewItemDto[]): ClaimFilter {
  return claims.some(isWaitingForYou) ? 'needs_you' : 'all';
}

export type ClaimSection = { key: string; label: string; claims: ClaimReviewItemDto[] };

/** Where a claim sits, in words: "Section 3 · Phase changes", "Question · answer key". */
export function unitLabel(location: ClaimLocationDto): string {
  const title = location.unitTitle.trim();
  switch (location.unitKind) {
    case 'block': return `Section ${location.unitIndex + 1}${title ? ` · ${title}` : ''}`;
    case 'question': return `Question${title ? ` · ${title}` : ''}`;
    case 'answer_key': return `Question${title ? ` · ${title}` : ''} · answer key`;
  }
}

/** Claims grouped by unit, in lesson order (blocks, then questions). */
export function groupBySection(claims: ClaimReviewItemDto[]): ClaimSection[] {
  const sections = new Map<string, ClaimSection & { index: number }>();
  for (const claim of claims) {
    const { unitIndex, unitKind } = claim.location;
    // A question and its answer key share a unit; the key reads as part of it.
    const key = String(unitIndex);
    const section = sections.get(key) ?? { key, index: unitIndex, label: unitLabel({ ...claim.location, unitKind: unitKind === 'answer_key' ? 'question' : unitKind }), claims: [] };
    section.claims.push(claim);
    sections.set(key, section);
  }
  return [...sections.values()].sort((a, b) => a.index - b.index).map(({ key, label, claims: items }) => ({ key, label, claims: items }));
}

/** The honest name of a verdict (§3 VerdictKind). */
export function verdictLabel(kind: VerdictKindDto | null): string {
  if (!kind) return 'Not checked yet';
  switch (kind.kind) {
    case 'unsupported': return 'Unsupported';
    case 'contradicted': return 'Contradicted';
    case 'no_source': return 'No source found';
    case 'could_not_judge': return 'Could not judge';
    case 'general_knowledge_proposed': return 'General knowledge proposed';
    case 'disputed': return `Disputed by the Verifier · ${verdictLabel(kind.verifier).toLowerCase()}`;
  }
}

/** One sentence on what the verdict means, so the chip never stands alone. */
export function verdictMeaning(kind: VerdictKindDto | null): string {
  if (!kind) return 'The checker has not judged this claim yet.';
  switch (kind.kind) {
    case 'unsupported': return 'Evidence was compared and none of it supports the claim.';
    case 'contradicted': return 'A passage says otherwise.';
    case 'no_source': return 'No saved passage mentions the claim’s terms.';
    case 'could_not_judge': return 'The checker could not give a usable answer for this claim.';
    case 'general_knowledge_proposed': return 'The checker proposes this is general knowledge. It was not checked against a source.';
    case 'disputed': return 'The local checker verified this claim; the Verifier audit disagrees.';
  }
}

/** Contradicted claims, local or by the Verifier, need a note to confirm (D3). */
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
  unconfirmed: 'Reopened: you unconfirmed it',
  verifier_dispute: 'Reopened: the Verifier disputes it',
};

function date(at: number): string {
  return new Date(at).toLocaleDateString([], { day: 'numeric', month: 'short' });
}

/** The state in words, for the card's status line. Text, never colour alone. */
export function stateLabel(item: ClaimReviewItemDto): string {
  const state = item.state;
  switch (state.state) {
    case 'unchecked': return 'Waiting to be checked';
    case 'open': return `Being repaired · ${state.machineAttemptsLeft} ${state.machineAttemptsLeft === 1 ? 'try' : 'tries'} left`;
    case 'queued': return 'Needs your review';
    case 'resolved_by_evidence': return 'Verified against a source';
    case 'confirmed': return `Confirmed by you · ${date(state.at)}`;
    case 'removed': return state.by === 'machine' ? `Removed by the machine · ${date(state.at)}` : `Removed by you · ${date(state.at)}`;
    case 'edited': return state.successor ? `Edited · ${date(state.at)}` : `Removed by an edit · ${date(state.at)}`;
    case 'general_knowledge': return `General knowledge · not checked against a source${state.by === 'user' ? ' · accepted by you' : ''}`;
    case 'reopened': return REOPEN_CAUSES[state.cause.cause];
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
    case 'confirm': return `Confirmed${quoted}`;
    case 'unconfirm': return `Unconfirmed${quoted}`;
    case 'edit': return `Edited: “${action.before}” → “${action.after}”`;
    case 'remove': return 'Removed the sentence';
    case 'general_knowledge': return 'Marked as general knowledge';
    case 'research_with_hint': return `Asked for research: “${action.hint}”`;
  }
}

/** One history row in words. */
export function describeEvent(payload: ClaimEventPayloadDto): string {
  switch (payload.kind) {
    case 'extracted': return `Claim found in ${payload.field}: “${payload.quote}”`;
    case 'checked': {
      const who = payload.check.checker.role === 'verifier' ? 'Verifier' : 'Checker';
      const result = payload.check.result;
      const verdict = result.verdict === 'supported' ? 'supported' : verdictLabel(result.kind).toLowerCase();
      return `${who} (${payload.check.checker.model}): ${verdict}${payload.check.reason ? ` · ${payload.check.reason}` : ''}`;
    }
    case 'researched': return payload.queries.length
      ? `Searched: ${payload.queries.join('; ')} · ${payload.passagesFound} ${payload.passagesFound === 1 ? 'passage' : 'passages'} found`
      : 'Research had nothing to search';
    case 'repair_attempted': return `Repair ${payload.attempt}${payload.escalated ? ' (last try)' : ''}: ${repairOutcomeLabel(payload.outcome)}`;
    case 'executed': return payload.passed ? 'Example ran as shown' : 'Example did not run as shown';
    case 'user_action': return describeUserAction(payload.action, payload.note);
    case 'evidence_attached': {
      const input = { saved_passage: 'a saved passage', link: 'a link', document: 'a file', pasted_text: 'pasted text' }[payload.provenance.input];
      const result = payload.result.outcome === 'no_passage' ? 'no passage mentions the claim'
        : payload.result.check.result.verdict === 'supported' ? 'verified' : verdictLabel(payload.result.check.result.kind).toLowerCase();
      return `Added ${input}: ${result}`;
    }
    case 'superseded': return payload.successor ? 'Replaced by a re-extracted claim' : 'Removed by an edit';
    case 'reopened': return REOPEN_CAUSES[payload.cause.cause];
    case 'undone': return `Undid step ${payload.eventOrdinal}`;
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
    verdict: result.verdict === 'supported' ? 'Supported' : verdictLabel(result.kind),
    reason: check.reason,
    supportingQuote: result.verdict === 'supported' ? result.supportingQuote : null,
    quotedSource: check.quotedSource,
    passages: check.passages,
  };
}

/** "Local checker said / Verifier said" (§14.5), once the Verifier has judged the claim. */
export function checkerColumns(item: ClaimReviewItemDto): { local: CheckerSide | null; verifier: CheckerSide } | null {
  if (!item.verifierCheck) return null;
  return { local: item.localCheck ? side(item.localCheck) : null, verifier: side(item.verifierCheck) };
}

/** Whether a user event came after the job stopped: Resume reruns nothing unchanged (§6.2). */
export function changedSince(review: LessonClaimReviewDto, stoppedAt: number | null): boolean {
  if (stoppedAt == null) return true;
  return review.claims.some((claim) => claim.history.some((event) => event.actor === 'user' && event.at >= stoppedAt));
}

/** The code every Verifier audit stop starts with (`verifier_unset`, `verifier_turned_off`…). */
export const AUDIT_STOP_PREFIX = 'verifier_';

/** What holds a waiting lesson besides its claims: the audit stop, if any. */
export type ReviewHold = 'claims' | 'audit_stop' | 'audit' | 'resolved';

/**
 * Why a lesson in Needs review is held. With no claim waiting it is held by
 * the Verifier audit (its stop reason, or none recorded), unless the user has
 * resolved everything since the job stopped.
 */
export function reviewHold(review: LessonClaimReviewDto, waiting: number, changed: boolean): ReviewHold {
  if (waiting > 0) return 'claims';
  if (review.stopReason?.code.startsWith(AUDIT_STOP_PREFIX)) return 'audit_stop';
  return changed ? 'resolved' : 'audit';
}

export const NOTHING_CHANGED = 'Nothing changed since the last pass. Resolve a claim or add evidence first.';

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

export function claimsNoun(count: number): string {
  return `${count} ${count === 1 ? 'claim' : 'claims'}`;
}

/** The panel's one-line summary, e.g. "12 verified · 3 confirmed by you · 2 general knowledge". */
export function countsLine(review: LessonClaimReviewDto): string {
  const { verified, confirmed, generalKnowledge, queued, open, removed } = review.counts;
  return [
    `${verified} verified`,
    confirmed ? `${confirmed} confirmed by you` : null,
    generalKnowledge ? `${generalKnowledge} general knowledge` : null,
    open ? `${open} being repaired` : null,
    removed ? `${removed} removed` : null,
    `${queued} waiting for you`,
  ].filter(Boolean).join(' · ');
}

/** What the live region says after an action. */
export function announcement(action: ClaimActionKindDto | 'accept_all_general_knowledge', remaining: number): string {
  const done: Record<string, string> = {
    confirm: 'Claim confirmed.', edit: 'Sentence edited.', remove: 'Claim removed.', general_knowledge: 'Marked as general knowledge.',
    attach_evidence: 'Evidence checked.', research_with_hint: 'Research finished.', unconfirm: 'Confirmation withdrawn.', undo: 'Undone.',
    accept_all_general_knowledge: 'General-knowledge proposals accepted.',
  };
  return `${done[action]} ${remaining} ${remaining === 1 ? 'remains' : 'remain'}.`;
}
