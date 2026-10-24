import { describe, expect, it } from 'vitest';

import { claimsCount } from '@/features/learning/lessons/review/claimReviewCopy';
import {
  actorLabel,
  addedByLabel,
  checkerColumns,
  claimFilterOf,
  confirmNeedsNote,
  describeEvent,
  excerptAround,
  groupClaims,
  groupCountLabel,
  repairOutcomeLabel,
  unitLabel,
} from '@/features/learning/lessons/review/claimReviewPresentation';
import { claimItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimEventPayloadDto, ClaimLocationDto, ClaimReviewItemDto, ClaimStateDto, VerdictKindDto } from '@/lib/bindings';

const event = (payload: unknown) => describeEvent(payload as ClaimEventPayloadDto);
const check = (role: 'local' | 'verifier', result: unknown, reason = '') => ({ checker: { role, model: 'herb-4b' }, result, reason });
const failed = (kind: VerdictKindDto) => ({ verdict: 'failed', kind });

describe('describeEvent', () => {
  it('words what the checker found', () => {
    expect(event({ kind: 'extracted', field: 'body', statement: 'Rome was founded in 753 BC.' })).toBe('Claim found in body: “Rome was founded in 753 BC.”');
    expect(event({ kind: 'checked', check: check('local', { verdict: 'supported', supportingQuote: 'q' }, 'matches Livy') }))
      .toBe('Checker (herb-4b): supported · matches Livy');
    expect(event({ kind: 'checked', check: check('verifier', { verdict: 'supported_elsewhere', supportingQuote: 'q' }) }))
      .toBe('Independent check (herb-4b): supported elsewhere, citation repaired');
  });

  it.each<[VerdictKindDto, string]>([
    [{ kind: 'unsupported' }, 'not stated by the passages compared'],
    [{ kind: 'contradicted' }, 'a source says otherwise'],
    [{ kind: 'no_source' }, 'no saved source mentions it'],
    [{ kind: 'could_not_judge' }, 'could not check'],
    [{ kind: 'general_knowledge_proposed' }, 'proposed as common knowledge'],
    [{ kind: 'disputed', verifier: { kind: 'contradicted' } }, 'disputed by an independent check · a source says otherwise'],
    [{ kind: 'insufficient_standing', bar: 'primary', have: [] } as VerdictKindDto, 'stated, but not by a source enough on its own'],
  ])('words a failed check (%j)', (kind, words) => {
    expect(event({ kind: 'checked', check: check('local', failed(kind)) })).toBe(`Checker (herb-4b): ${words}`);
  });

  it('words research, repairs and runs', () => {
    expect(event({ kind: 'researched', queries: ['Roman founding date', 'Romulus'], passagesFound: 1 })).toBe('Searched: Roman founding date; Romulus · 1 passage found');
    expect(event({ kind: 'researched', queries: ['Romulus'], passagesFound: 3 })).toBe('Searched: Romulus · 3 passages found');
    expect(event({ kind: 'researched', queries: [], passagesFound: 0 })).toBe('Research had nothing to search');
    expect(event({ kind: 'repair_attempted', attempt: 2, escalated: true, outcome: { outcome: 'rewritten' } })).toBe('Repair 2 (last try): Rewritten, then checked again');
    expect(event({ kind: 'repair_attempted', attempt: 1, escalated: false, outcome: { outcome: 'removed' } })).toBe('Repair 1: The repair removed the sentence');
    expect(event({ kind: 'executed', passed: true })).toBe('Example ran as shown');
    expect(event({ kind: 'executed', passed: false })).toBe('Example did not run as shown');
    expect(event({ kind: 'citation_repaired', addedSource: true, quote: 'Founded by Romulus' })).toBe('Citation moved to the passage that supports it, a source added to the lesson: “Founded by Romulus”');
    expect(event({ kind: 'citation_repaired', addedSource: false, quote: 'q' })).toBe('Citation moved to the passage that supports it: “q”');
  });

  it('words what the user did', () => {
    expect(event({ kind: 'user_action', action: { action: 'confirm' }, note: 'Livy says so' })).toBe('Accepted · “Livy says so”');
    expect(event({ kind: 'user_action', action: { action: 'unconfirm' }, note: null })).toBe('Acceptance withdrawn');
    expect(event({ kind: 'user_action', action: { action: 'edit', before: 'old', after: 'new' }, note: null })).toBe('Edited: “old” → “new”');
    expect(event({ kind: 'user_action', action: { action: 'remove' }, note: null })).toBe('Removed the sentence');
    expect(event({ kind: 'user_action', action: { action: 'general_knowledge' }, note: null })).toBe('Accepted as common knowledge');
    expect(event({ kind: 'user_action', action: { action: 'research_with_hint', hint: 'Livy book 1' }, note: null })).toBe('Asked for a search: “Livy book 1”');
  });

  it('words evidence the user added', () => {
    expect(event({ kind: 'evidence_attached', provenance: { input: 'link' }, result: { outcome: 'no_passage' } })).toBe('Added a link: it does not mention the claim');
    expect(event({ kind: 'evidence_attached', provenance: { input: 'document' }, result: { outcome: 'checked', check: check('local', failed({ kind: 'contradicted' })) } }))
      .toBe('Added a file: a source says otherwise');
    expect(event({ kind: 'evidence_attached', provenance: { input: 'pasted_text' }, result: { outcome: 'checked', check: check('local', { verdict: 'supported', supportingQuote: 'q' }) } }))
      .toBe('Added pasted text: it states the claim');
    expect(event({ kind: 'evidence_attached', provenance: { input: 'saved_passage' }, result: { outcome: 'no_passage' } })).toBe('Added a saved passage: it does not mention the claim');
  });

  it('words the claim’s own life', () => {
    expect(event({ kind: 'superseded', successor: 'c2' })).toBe('Replaced by a re-extracted claim');
    expect(event({ kind: 'superseded', successor: null })).toBe('Removed by an edit');
    expect(event({ kind: 'born_of_repair' })).toBe('Added while repairing another claim');
    expect(event({ kind: 'reopened', cause: { cause: 'verifier_dispute' } })).toBe('Reopened: an independent check disagrees');
    expect(event({ kind: 'undone', eventOrdinal: 4 })).toBe('Undid step 4');
    expect(event({ kind: 'stopped', reason: { message: 'The model ran out of room' } })).toBe('Stopped: The model ran out of room');
  });

  it('words how the source was weighed and why', () => {
    expect(event({ kind: 'standing_assessed', assessment: { sufficient: true, cause: null } })).toBe('Weighed: its source is enough on its own');
    expect(event({ kind: 'standing_assessed', assessment: { sufficient: false, cause: { change: 'override' } } }))
      .toBe('Weighed: its source is not enough on its own after you changed how far a source is trusted');
    expect(event({ kind: 'standing_assessed', assessment: { sufficient: false, cause: { change: 'rules' } } }))
      .toBe('Weighed: its source is not enough on its own after the course’s rules moved');
  });
});

describe('labels', () => {
  it('words every repair outcome and actor', () => {
    expect(repairOutcomeLabel({ outcome: 'rejected', reason: 'changed the meaning' })).toBe('Repair rejected: changed the meaning');
    expect(repairOutcomeLabel({ outcome: 'superseded_by_user' })).toBe('Your change superseded this repair');
    expect(repairOutcomeLabel({ outcome: 'not_attempted', reason: 'no budget' } as never)).toBe('Not repaired: no budget');
    expect(actorLabel('machine')).toBe('Machine');
    expect(actorLabel('user')).toBe('You');
  });

  it('says who added a source', () => {
    expect(addedByLabel(null)).toBeNull();
    expect(addedByLabel('user', 'pasted' as never)).toBe('Pasted by you');
    expect(addedByLabel('user')).toBe('Added by you');
    expect(addedByLabel('research')).toBe('Found by research');
    expect(addedByLabel('verifier')).toBe('Found by the Verifier');
  });

  it('names units without a title', () => {
    const location = (unitKind: string, unitTitle = '') => ({ unitIndex: 2, unitKind, unitTitle, field: 'body', quote: '' } as unknown as ClaimLocationDto);
    expect(unitLabel(location('block'))).toBe('Section 3');
    expect(unitLabel(location('question'))).toBe('Question');
    expect(unitLabel(location('answer_key'))).toBe('Question · answer key');
    expect(unitLabel(location('answer_key', 'Dates'))).toBe('Question · Dates · answer key');
  });
});

describe('claimFilterOf', () => {
  it.each<[ClaimStateDto, ReturnType<typeof claimFilterOf>]>([
    [{ state: 'unchecked' }, 'repairing'],
    [{ state: 'open', machineAttemptsLeft: 1 }, 'repairing'],
    [{ state: 'resolved_by_evidence' }, 'machine'],
    [{ state: 'removed', at: 1, by: 'machine' }, 'machine'],
    [{ state: 'removed', at: 1, by: 'user' }, 'changed'],
    [{ state: 'confirmed', at: 1 }, 'confirmed'],
    [{ state: 'general_knowledge', at: 1, by: 'user' }, 'general'],
    [{ state: 'edited', at: 1, successor: null }, 'changed'],
    [{ state: 'reopened', cause: { cause: 'unconfirmed' }, previous: { state: 'confirmed', at: 1 } }, 'needs_you'],
  ])('files %j as %s', (state, filter) => {
    expect(claimFilterOf(claimItem({ state }))).toBe(filter);
  });
});

describe('grouping by what fixes a claim', () => {
  const withKind = (claimId: string, verdictKind: VerdictKindDto | null) => claimItem({ claimId, verdictKind } as Partial<ClaimReviewItemDto>);

  it('groups in the fixed order, leaving out empty groups', () => {
    const groups = groupClaims([
      withKind('a', { kind: 'could_not_judge' }),
      withKind('b', { kind: 'contradicted' }),
      withKind('c', null),
      withKind('d', { kind: 'disputed', verifier: { kind: 'unsupported' } }),
    ], 'fix');
    expect(groups.map((g) => [g.label, g.claims.map((c) => c.claimId)])).toEqual([
      ['Needs a source', ['c']],
      ['Says otherwise', ['b', 'd']],
      ['Could not check', ['a']],
    ]);
    expect(groupCountLabel(groups[1])).toBe(claimsCount(2));
  });
});

describe('excerpts and checks', () => {
  it('falls back to the quote alone when it is not in the passage', () => {
    expect(excerptAround('Carthage fell in 146 BC.', null, 'Rome burned')).toEqual({ before: '', quote: 'Rome burned', after: '', clippedStart: false, clippedEnd: false });
  });

  it('finds the quote by text when the byte span does not match', () => {
    const text = 'Rome grew. Carthage fell in 146 BC. Greece followed.';
    const excerpt = excerptAround(text, [0, 3], 'Carthage fell in 146 BC.');
    expect(excerpt.quote).toBe('Carthage fell in 146 BC.');
    expect(excerpt.before).toBe('Rome grew. ');
    expect(excerpt.after).toBe(' Greece followed.');
  });

  it('leaves code out of the context around a quote', () => {
    const text = 'Run the tally.\n```\ncount = 3\n```\nThen record the result here.';
    const excerpt = excerptAround(text, null, 'Then record the result here.');
    expect(excerpt.before).toBe('');
    expect(excerpt.clippedStart).toBe(true);
  });

  it('needs no note for a claim with no verdict', () => {
    expect(confirmNeedsNote(claimItem({ verdictKind: null } as Partial<ClaimReviewItemDto>))).toBe(false);
  });

  it('shows no columns before the independent check has run, and only its side without a first check', () => {
    expect(checkerColumns(claimItem({ verifierCheck: null } as Partial<ClaimReviewItemDto>))).toBeNull();
    const verifierCheck = { checker: { role: 'verifier', model: 'herb-27b' }, result: failed({ kind: 'unsupported' }), reason: 'not stated', quotedSource: null, passages: [] };
    const columns = checkerColumns(claimItem({ verifierCheck, localCheck: null } as unknown as Partial<ClaimReviewItemDto>));
    expect(columns).toEqual({
      local: null,
      verifier: { model: 'herb-27b', verdict: 'Not stated by the passages compared', reason: 'not stated', supportingQuote: null, quotedSource: null, passages: [] },
    });
  });
});
