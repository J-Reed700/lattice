import { describe, expect, it } from 'vitest';

import {
  attachSummary,
  countsAs,
  jobTallyLine,
  kindWord,
  standingWords,
  tallyLine,
  triedLine,
  whyItWaits,
} from '@/features/learning/lessons/review/claimReviewCopy';
import {
  checkerColumns,
  claimFilterOf,
  confirmNeedsNote,
  defaultGroupMode,
  describeEvent,
  excerptAround,
  groupBySection,
  groupBySource,
  heldLessonIds,
  jobHoldingLesson,
  reviewHold,
  unitLabel,
} from '@/features/learning/lessons/review/claimReviewPresentation';
import { support, sufficiency, thirtyThreeClaims, tried, waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { claimItem, review, STOPPED_AT, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { CheckerVerdictDto, ClaimAttachReportDto, ClaimEventDto, LearningGenerationJob, LearningTrustLevel, SourceKindDto } from '@/lib/bindings';

const job = (overrides: Partial<LearningGenerationJob>): LearningGenerationJob => ({
  id: 'job-1', programId: 'program-1', operationId: 'op', kind: 'lesson_preparation', payloadSha256: 'hash', baseRevisionNumber: 1,
  status: 'completed', progressCompleted: 1, progressTotal: 1, progressMessage: 'Needs your review: 2 claims', activity: null,
  resultId: 'needs_review:lesson-1,lesson-2', error: null, retryOfJobId: null, createdAt: 1, startedAt: 1, finishedAt: STOPPED_AT,
  ...overrides,
});

const KINDS: SourceKindDto[] = ['online_book', 'documentation', 'reference_work', 'peer_reviewed_paper', 'report', 'article', 'guide', 'forum', 'wiki', 'marketing_page', 'your_file', 'pasted_text', 'page'];
const LEVELS: LearningTrustLevel[] = ['primary', 'authoritative', 'established', 'community', 'unknown', 'distrusted'];

describe('claim review presentation', () => {
  it('requires a note to accept a contradiction, local or by the independent check, and only then', () => {
    expect(confirmNeedsNote(claimItem({ verdictKind: { kind: 'contradicted' } }))).toBe(true);
    expect(confirmNeedsNote(claimItem({ verdictKind: { kind: 'disputed', verifier: { kind: 'contradicted' } } }))).toBe(true);
    expect(confirmNeedsNote(claimItem({ verdictKind: { kind: 'disputed', verifier: { kind: 'unsupported' } } }))).toBe(false);
    expect(confirmNeedsNote(unsupportedItem(2))).toBe(false);
  });

  it('files each state as waiting for the user or not', () => {
    const states = [
      claimItem({ state: { state: 'queued' } }),
      claimItem({ claimId: 'reopened', state: { state: 'reopened', cause: { cause: 'verifier_dispute' }, previous: { state: 'confirmed', at: 1 } } }),
      claimItem({ claimId: 'open', state: { state: 'open', machineAttemptsLeft: 1 } }),
      claimItem({ claimId: 'verified', state: { state: 'resolved_by_evidence' } }),
      claimItem({ claimId: 'machine-removed', state: { state: 'removed', at: 1, by: 'machine' } }),
      claimItem({ claimId: 'user-removed', state: { state: 'removed', at: 1, by: 'user' } }),
    ];
    expect(states.map(claimFilterOf)).toEqual(['needs_you', 'needs_you', 'repairing', 'machine', 'machine', 'changed']);
  });

  it('groups claims by unit in lesson order with the unit in words', () => {
    const sections = groupBySection([unsupportedItem(1, 4), unsupportedItem(2, 0), unsupportedItem(3, 4)]);
    expect(sections.map((section) => [section.label, section.claims.length])).toEqual([['Section 1 · Section title 1', 1], ['Section 5 · Section title 5', 2]]);
    expect(unitLabel({ unitIndex: 6, unitKind: 'answer_key', unitTitle: 'Boiling', field: 'explanation', quote: '', span: null })).toBe('Question · Boiling · answer key');
  });

  it('groups by source, largest first, with the claims no source states last', () => {
    const groups = groupBySource(thirtyThreeClaims());
    expect(groups.map((group) => [group.label, group.claims.length])).toEqual([
      ['Field Guide to North American Butterflies', 15], ['Chronicle of the Monarch Reserves', 6], ['No source states it', 12],
    ]);
    expect(groups[0].detail).toBe('butterflies.example.org · online book');
    expect(defaultGroupMode(thirtyThreeClaims())).toBe('source');
    expect(defaultGroupMode([waitingOnSource(1), waitingOnSource(2, 'other', 'Other')])).toBe('section');
  });

  it('finds the lessons a completed job holds, ignoring superseded and unfinished attempts', () => {
    expect(heldLessonIds(job({}))).toEqual(['lesson-1', 'lesson-2']);
    expect(heldLessonIds(job({ status: 'running' }))).toEqual([]);
    expect(heldLessonIds(job({ resultId: 'lesson-1' }))).toEqual([]);
    const retry = job({ id: 'job-2', status: 'pending', resultId: null, retryOfJobId: 'job-1' });
    expect(jobHoldingLesson([job({}), retry], 'lesson-1')).toBeNull();
    expect(jobHoldingLesson([job({})], 'lesson-2')?.id).toBe('job-1');
  });

  it('puts the first check and the independent check side by side', () => {
    const passage = claimItem().evidence[0] ?? { passage: support().passage, addedBy: 'research' as const };
    const local: CheckerVerdictDto = { checker: { role: 'local', model: 'qwen' }, result: { verdict: 'supported', sourceVersionId: 'v1', supportingQuote: 'migrates up to 3,000 miles' }, reason: 'Entailed.', quotedSource: null, passages: [passage], at: 1 };
    const verifier: CheckerVerdictDto = { checker: { role: 'verifier', model: 'claude' }, result: { verdict: 'failed', kind: { kind: 'contradicted' } }, reason: 'The guide says 2,500 miles.', quotedSource: null, passages: [passage], at: 2 };
    expect(checkerColumns(claimItem({ localCheck: local, verifierCheck: verifier }))).toMatchObject({
      local: { verdict: 'Supported', model: 'qwen' },
      verifier: { verdict: 'A source says otherwise', reason: 'The guide says 2,500 miles.', model: 'claude' },
    });
    expect(checkerColumns(claimItem({ localCheck: local, verifierCheck: null }))).toBeNull();
    const event: ClaimEventDto = { ordinal: 2, actor: 'machine', at: 2, jobId: null, appVersion: 'test', checkerModel: null,
      event: { kind: 'checked', check: { checker: verifier.checker, result: verifier.result, reason: verifier.reason, fingerprint: 'f', evidence: [], windowTokens: 8192 } } };
    expect(describeEvent(event.event)).toBe('Independent check (claude): a source says otherwise · The guide says 2,500 miles.');
  });

  it('tells a hold by claims from a hold by the Verifier audit, so nothing reads "0 claims"', () => {
    const auditStop = { code: 'verifier_turned_off', message: 'Audit stopped: you turned the Verifier off.', fix: 'Resume to publish without the audit.', at: 1, request: null };
    expect(reviewHold(review([claimItem()], { changedSinceStop: true }), 1)).toBe('claims');
    expect(reviewHold(review([], { stopReason: auditStop }), 0)).toBe('audit_stop');
    expect(reviewHold(review([]), 0)).toBe('audit');
    expect(reviewHold(review([], { changedSinceStop: true }), 0)).toBe('resolved');
  });

  it('the tally never counts the machine’s rewrites as the user’s', () => {
    const counts = { verified: 12, confirmed: 0, generalKnowledge: 0, queued: 33, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 74 };
    expect(tallyLine(counts)).toBe('33 claims need you · 12 verified · 74 rewritten by the machine');
    expect(tallyLine({ ...counts, queued: 1, confirmed: 2, removedByYou: 1 })).toBe('1 claim needs you · 12 verified · 2 confirmed by you · 74 rewritten by the machine · 1 removed by you');
    const tally = { unchecked: 0, searching: 2, sentBack: 1, repairing: 0, waiting: 5, verified: 40, resolvedByYou: 0, rewritten: 9 };
    expect(jobTallyLine(tally)).toBe('5 claims need you · 40 verified · 9 rewritten by the machine · 3 being checked');
    expect(jobTallyLine(tally)).not.toMatch(/resolved by you/);
  });

  it('kind and standing words cover every DTO value', () => {
    for (const kind of KINDS) expect(kindWord(kind)).toMatch(/^[a-z][a-z -]+$/);
    expect(new Set(KINDS.map(kindWord)).size).toBe(KINDS.length);
    for (const level of LEVELS) expect(standingWords(level)).not.toMatch(new RegExp(`\\b${level}\\b`));
    expect(countsAs(support(), sufficiency('no_identified_source'))).toBe('an unidentified author’s work · not enough on its own for this claim');
    expect(countsAs(support({ standing: 'established', counts: { outcome: 'enough' } }), null)).toBe('a reliable identified source · enough');
    expect(countsAs(support({ source: { ...support().source, addedBy: 'user' } }), null)).toBe('a source you added · enough, it states this');
    expect(countsAs(support({ counts: { outcome: 'enough' } }), sufficiency(null, { sufficient: true, label: 'best_available' }))).toBe('the course’s strongest source on this subject · enough');
  });

  it('explains a history claim and a biology claim in the same plain words', () => {
    const history = { ...waitingOnSource(1, 'source-chronicle', 'Chronicle of the Monarch Reserves'), sufficiency: sufficiency('no_independent_corroboration') };
    expect(whyItWaits(history)).toMatchObject({ chip: 'Waiting for you', reason: 'Only one source states this, and this kind of statement needs a second one.' });
    expect(whyItWaits(waitingOnSource(2)).detail).toBe('The one passage that does is from an online book whose author is not identified.');
  });

  it('sums what was tried into one line', () => {
    expect(triedLine(tried())).toBe('Searched the web 9 times and read 214 pages; none stated it.');
    expect(triedLine(tried({ pagesMentioning: 2 }))).toBe('Searched the web 9 times and read 214 pages; 2 pages mention it but do not state it.');
    expect(triedLine(tried({ searches: 0, pagesRead: 0 }))).toBeNull();
  });

  it('cuts the excerpt at one sentence either side of the quote, from a byte span', () => {
    const evidence = support();
    const excerpt = excerptAround(evidence.passage.text, evidence.quoteSpan, evidence.quote);
    expect(excerpt).toMatchObject({ before: 'Monarchs breed across the northern range in summer. ', quote: evidence.quote, after: ' The return trip takes several generations.' });
    expect(excerptAround('Nothing here.', null, 'Absent sentence.')).toMatchObject({ before: '', quote: 'Absent sentence.', after: '' });
  });

  it('sums an attach for a group into one line', () => {
    const report: ClaimAttachReportDto = {
      revision: 8, source: { ...support().source, title: 'Monarch Watch' }, alreadySaved: false,
      outcome: { outcome: 'checked', check: { checker: { role: 'local', model: 'qwen' }, result: { verdict: 'supported', sourceVersionId: 'v', supportingQuote: 'q' }, reason: '', fingerprint: 'f', evidence: [], windowTokens: 1 } },
      failure: null, snapshot: { state: 'not_applicable' }, others: { tried: 14, verified: 11, notVerified: 2, noPassage: 1, stopped: null }, lines: [],
    };
    expect(attachSummary(report, 15, 18).line).toBe('Added Monarch Watch. Verified 12 of 15 claims; 2 do not match what it says; 1 is not mentioned by it.');
    expect(attachSummary(report, 1, 32).line).toBe('Added Monarch Watch; it states this. 32 left.');
  });
});
