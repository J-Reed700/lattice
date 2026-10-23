import { describe, expect, it } from 'vitest';

import {
  addSourceTitle, announcement, attachSummary, claimsNeedYou, countsAs, failureLine, fixGroupOf, jobTallyLine, kindWithArticle, preparingLine,
  progressLine, recheckLine, resolvedChip, sourceLine, tallyLine, triedLine, trustedAnnouncement, verifiedLabel, whyItWaits, ALL_RESOLVED,
} from '@/features/learning/lessons/review/claimReviewCopy';
import { support, sufficiency, tried } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { claimItem, fieldGuideSupport } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimAttachReportDto, ClaimReviewItemDto, ClaimStateDto } from '@/lib/bindings';

const withState = (state: ClaimStateDto): ClaimReviewItemDto => claimItem({ state });
const none = { tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null };
const source = { sourceId: 's', sourceVersionId: 'v', title: 'Monarch Watch', url: null, addedBy: 'user' as const };
const checkedResult = (supported: boolean) => ({
  outcome: 'checked' as const,
  check: {
    checker: { role: 'local' as const, model: 'm' }, reason: '', fingerprint: 'f', evidence: [], windowTokens: 1,
    result: supported ? { verdict: 'supported' as const, sourceVersionId: 'v', supportingQuote: 'q' } : { verdict: 'failed' as const, kind: { kind: 'unsupported' as const } },
  },
});
const report = (overrides: Partial<ClaimAttachReportDto> = {}): ClaimAttachReportDto => ({
  revision: 1, source, alreadySaved: false, outcome: checkedResult(true), failure: null, snapshot: { state: 'not_applicable' }, others: none, lines: [], ...overrides,
});

describe('source words', () => {
  it('chooses the article by the first letter, and none for mass nouns', () => {
    expect(kindWithArticle('online_book')).toBe('an online book');
    expect(kindWithArticle('forum')).toBe('a forum');
    expect(kindWithArticle('documentation')).toBe('documentation');
    expect(kindWithArticle('your_file')).toBe('your file');
    expect(kindWithArticle('pasted_text')).toBe('pasted text');
  });

  it('joins the title, site and kind, leaving out what is missing', () => {
    expect(sourceLine(fieldGuideSupport({ site: null }))).toBe('Field Guide to North American Butterflies, ch. 7 · online book');
    expect(sourceLine(fieldGuideSupport())).toBe('Field Guide to North American Butterflies, ch. 7 · example.org · online book');
  });
});

describe('countsAs', () => {
  it('names a contradicting source by standing, or plainly when unweighed', () => {
    expect(countsAs(support({ role: 'contradicts', standing: 'authoritative' }), null)).toBe('an authority on this subject');
    expect(countsAs(support({ role: 'contradicts', standing: null }), null)).toBe('a source in this course');
  });

  it('says a source the user added counts when it states the claim', () => {
    const added = support({ source: { ...support().source, addedBy: 'user' }, counts: { outcome: 'enough' } });
    expect(countsAs(added, null)).toBe('a source you added · enough, it states this');
  });

  it('does not credit a user source that was not counted', () => {
    const added = support({ source: { ...support().source, addedBy: 'user' }, counts: { outcome: 'not_counted', reason: 'it is a forum' }, standing: 'community' });
    expect(countsAs(added, null)).toBe('a community page (forum, wiki, Q&A) · not counted: it is a forum');
  });

  it('calls the course’s best available source enough', () => {
    expect(countsAs(support(), sufficiency(null, { sufficient: true, label: 'best_available' }))).toBe('the course’s strongest source on this subject · enough');
  });

  it('describes each outcome with the source’s standing, or says it is not yet weighed', () => {
    expect(countsAs(support({ standing: 'established', counts: { outcome: 'enough' } }), null)).toBe('a reliable identified source · enough');
    expect(countsAs(support({ standing: 'established', counts: { outcome: 'not_enough' } }), null)).toBe('a reliable identified source · not enough on its own for this claim');
    expect(countsAs(support({ standing: null, counts: { outcome: 'enough' } }), null)).toBe('an online book not yet weighed · enough');
  });
});

describe('verifiedLabel', () => {
  it.each([
    ['practice', 'A recommendation'], ['attributed', 'Attributed'], ['community_consensus', 'Several community sources agree'],
    ['reported_by', 'As reported by its source'], ['secondary', 'From a secondary source'], ['mirror', 'From a copy of the original'],
    ['best_available', 'The course’s strongest source on this'],
  ])('labels %s', (label, words) => {
    expect(verifiedLabel(sufficiency(null, { label }))).toBe(words);
  });

  it('has no label for a plain verified claim or no sufficiency', () => {
    expect(verifiedLabel(sufficiency(null))).toBeNull();
    expect(verifiedLabel(null)).toBeNull();
  });
});

describe('whyItWaits', () => {
  const waiting = (missing: string | null, support_ = fieldGuideSupport()) =>
    whyItWaits(claimItem({ verdictKind: { kind: 'insufficient_standing' } as never, sufficiency: sufficiency(missing), support: support_ }));

  it.each([
    ['no_primary_for_subject', 'This needs the subject’s own account, and the course has none that states it.', ['add_source', 'accept_source']],
    ['stronger_source_silent', 'The course has a stronger source on this subject, and it does not state this.', ['accept_source', 'edit']],
    ['no_independent_corroboration', 'Only one source states this, and this kind of statement needs a second one.', ['add_source', 'accept_source']],
    ['only_testimonial', 'The sources that state this are reporting what someone else said.', ['add_source', 'accept_source']],
    ['only_stale', 'The sources that state this are older than the newest one on the subject.', ['add_source', 'accept']],
    ['no_measurement_origin', 'This number needs the source that measured it.', ['add_source', 'accept_source']],
    [null, 'No source the course trusts states this.', ['accept_source', 'add_source']],
  ])('explains a source that is not enough: %s', (missing, reason, fixes) => {
    const why = waiting(missing);
    expect(why.chip).toBe('Waiting for you');
    expect(why.reason).toBe(reason);
    expect(why.fixes).toEqual(fixes);
  });

  it('names where the only passage came from when the author is unknown', () => {
    expect(waiting('no_primary_for_subject').detail).toBe('It is stated by an online book from example.org.');
    expect(waiting(null).detail).toBe('The one passage that does is from an online book whose author is not identified.');
  });

  it('has no specific lack without a supporting source, and for stale sources', () => {
    expect(waiting('no_primary_for_subject', null as never).detail).toBeNull();
    expect(waiting('only_stale').detail).toBeNull();
  });

  it('describes an unsupported claim by how many passages mention the subject', () => {
    const unsupported = (pagesMentioning: number, evidence = 0) => whyItWaits(claimItem({
      verdictKind: { kind: 'unsupported' }, tried: tried({ pagesMentioning }), evidence: Array.from({ length: evidence }, () => claimItem().evidence[0]),
    }));
    expect(unsupported(0).detail).toBeNull();
    expect(unsupported(1).detail).toBe('1 passage mentions the subject; none state it.');
    expect(unsupported(3).detail).toBe('3 passages mention the subject; none state it.');
    expect(unsupported(0, 2).detail).toBe('2 passages mention the subject; none state it.');
    expect(unsupported(0).reason).toBe('No saved source states this.');
  });

  it('covers the verdicts without a source', () => {
    expect(whyItWaits(claimItem({ verdictKind: { kind: 'no_source' } as never })).fixes).toEqual(['add_source', 'remove']);
    expect(whyItWaits(claimItem({ verdictKind: { kind: 'could_not_judge' } as never })).chip).toBe('Could not check');
    expect(whyItWaits(claimItem({ verdictKind: { kind: 'general_knowledge_proposed' } as never }))).toMatchObject({ chip: 'Common knowledge?', fixes: ['general_knowledge', 'add_source'] });
    expect(whyItWaits(claimItem({ verdictKind: null }))).toMatchObject({ chip: 'Waiting for you', reason: 'This claim is waiting for you.' });
  });

  it('quotes the contradicting source, and says nothing specific when none is on file', () => {
    const against = whyItWaits(claimItem({ verdictKind: { kind: 'contradicted' }, support: support({ role: 'contradicts', quote: 'It is 2,000 miles.' }) }));
    expect(against.chip).toBe('Says otherwise');
    expect(against.detail).toBe('Field Guide to North American Butterflies: “It is 2,000 miles.”');
    expect(whyItWaits(claimItem({ verdictKind: { kind: 'contradicted' }, support: null })).detail).toBeNull();
  });

  it('reports a dispute with the independent check’s reason', () => {
    const disputed = whyItWaits(claimItem({
      verdictKind: { kind: 'disputed' } as never,
      verifierCheck: { checker: { role: 'verifier', model: 'v' }, reason: 'It reads the passage differently.', quotedSource: null, passages: [], at: 1, result: { verdict: 'failed', kind: { kind: 'unsupported' } } },
    }));
    expect(disputed.chip).toBe('Disputed');
    expect(disputed.detail).toBe('It reads the passage differently.');
  });

  it('turns a reopened claim’s chip into Reopened with the cause first', () => {
    const reopened = whyItWaits(claimItem({ state: { state: 'reopened', cause: { cause: 'source_deleted', sourceVersionId: 'v' }, previous: { state: 'queued' } }, verdictKind: { kind: 'unsupported' }, tried: tried({ pagesMentioning: 1 }) }));
    expect(reopened.chip).toBe('Reopened');
    expect(reopened.detail).toBe('Its source was deleted. 1 passage mentions the subject; none state it.');
    const bare = whyItWaits(claimItem({ state: { state: 'reopened', cause: { cause: 'unconfirmed' }, previous: { state: 'queued' } }, verdictKind: { kind: 'no_source' } as never }));
    expect(bare.detail).toBe('You took back your confirmation.');
  });
});

describe('resolvedChip', () => {
  it.each<[ClaimStateDto, string]>([
    [{ state: 'resolved_by_evidence' }, 'Verified'],
    [{ state: 'confirmed', at: 1 }, 'Confirmed by you'],
    [{ state: 'removed', at: 1, by: 'user' }, 'Removed by you'],
    [{ state: 'removed', at: 1, by: 'machine' }, 'Removed'],
    [{ state: 'edited', at: 1, successor: 'claim-2' }, 'Edited'],
    [{ state: 'edited', at: 1, successor: null }, 'Removed by an edit'],
    [{ state: 'general_knowledge', at: 1, by: 'user' }, 'Common knowledge'],
    [{ state: 'unchecked' }, 'Being checked'],
    [{ state: 'open', machineAttemptsLeft: 1 }, 'Being checked'],
    [{ state: 'queued' }, 'Says otherwise'],
  ])('words %j', (state, chip) => {
    expect(resolvedChip(withState(state))).toBe(chip);
  });
});

describe('fixGroupOf', () => {
  it('groups claims by what fixes them', () => {
    const group = (kind: string, extra: Partial<ClaimReviewItemDto> = {}) => fixGroupOf(claimItem({ verdictKind: { kind } as never, ...extra }));
    expect(group('insufficient_standing', { sufficiency: sufficiency('only_stale') })).toBe('accept_source');
    expect(group('insufficient_standing', { sufficiency: sufficiency('no_measurement_origin') })).toBe('needs_source');
    expect(group('contradicted')).toBe('says_otherwise');
    expect(group('disputed')).toBe('says_otherwise');
    expect(group('general_knowledge_proposed')).toBe('common_knowledge');
    expect(group('could_not_judge')).toBe('could_not_check');
    expect(group('unsupported')).toBe('needs_source');
    expect(fixGroupOf(claimItem({ verdictKind: null }))).toBe('needs_source');
  });
});

describe('triedLine', () => {
  it('says nothing when nothing was tried', () => {
    expect(triedLine(tried({ searches: 0, pagesRead: 0 }))).toBeNull();
  });

  it('describes searches, pages read and what they said', () => {
    expect(triedLine(tried({ searches: 9, pagesRead: 214 }))).toBe('Searched the web 9 times and read 214 pages; none stated it.');
    expect(triedLine(tried({ searches: 1, pagesRead: 1 }))).toBe('Searched the web once and read 1 page; none stated it.');
    expect(triedLine(tried({ searches: 0, pagesRead: 3 }))).toBe('Read 3 pages; none stated it.');
    expect(triedLine(tried({ searches: 2, pagesRead: 5, stated: true }))).toBe('Searched the web 2 times and read 5 pages; one of them states it.');
    expect(triedLine(tried({ searches: 2, pagesRead: 5, pagesMentioning: 1 }))).toBe('Searched the web 2 times and read 5 pages; 1 page mentions it but does not state it.');
    expect(triedLine(tried({ searches: 2, pagesRead: 5, pagesMentioning: 2 }))).toBe('Searched the web 2 times and read 5 pages; 2 pages mention it but do not state it.');
  });
});

describe('tallies', () => {
  it('leaves out zero counts and agrees in number', () => {
    const counts = { verified: 12, confirmed: 0, generalKnowledge: 2, queued: 1, open: 0, removedByYou: 0, editedByYou: 3, rewritten: 74 };
    expect(tallyLine(counts)).toBe('1 claim needs you · 12 verified · 2 common knowledge · 74 rewritten by the machine · 3 edited by you');
    expect(tallyLine({ ...counts, queued: 33, confirmed: 1, removedByYou: 1, open: 4 })).toBe(
      '33 claims need you · 12 verified · 1 confirmed by you · 2 common knowledge · 74 rewritten by the machine · 1 removed by you · 3 edited by you · 4 being checked',
    );
  });

  it('describes the running job’s claims', () => {
    const tally = { unchecked: 1, searching: 2, sentBack: 3, repairing: 4, waiting: 1, verified: 10, resolvedByYou: 2, rewritten: 5 };
    expect(jobTallyLine(tally)).toBe('1 claim needs you · 10 verified · 2 resolved by you · 5 rewritten by the machine · 10 being checked');
    expect(jobTallyLine({ ...tally, waiting: 2, rewritten: undefined, unchecked: 0, searching: 0, sentBack: 0, repairing: 0 })).toBe('2 claims need you · 10 verified · 2 resolved by you');
  });

  it('counts how far preparation has got', () => {
    expect(preparingLine(null)).toBe('Preparing');
    expect(preparingLine(undefined)).toBe('Preparing');
    const empty = { unchecked: 0, searching: 0, sentBack: 0, repairing: 0, waiting: 0, verified: 0, resolvedByYou: 0 };
    expect(preparingLine(empty)).toBe('Preparing');
    expect(preparingLine({ ...empty, unchecked: 4, verified: 40 })).toBe('Preparing · 40 of 44 checked');
  });

  it('agrees in number for the counts the queue shows', () => {
    expect(claimsNeedYou(1)).toBe('1 claim needs you');
    expect(claimsNeedYou(3)).toBe('3 claims need you');
  });
});

describe('announcements', () => {
  it('tells how many are left, or that everything is resolved', () => {
    expect(announcement('confirm', 4)).toBe('Accepted. 4 left.');
    expect(announcement('confirm', 0)).toBe(`Accepted. ${ALL_RESOLVED}`);
    expect(announcement('edit', 3)).toBe('Sentence edited. It is checked when you resume.');
    expect(announcement('remove', 1)).toBe('Removed. 1 left.');
    expect(announcement('general_knowledge', 2)).toBe('Accepted as common knowledge. 2 left.');
    expect(announcement('accept_all_general_knowledge', 2)).toBe('Common-knowledge proposals accepted. 2 left.');
    expect(announcement('research_with_hint', 2)).toBe('Searched with your hint. 2 left.');
    expect(announcement('unconfirm', 5)).toBe('Acceptance withdrawn. 5 left.');
    expect(announcement('undo', 5)).toBe('Undone. 5 left.');
  });

  it('announces a trusted source with the claims it settled', () => {
    expect(trustedAnnouncement('Monarch Watch', 3, 1)).toBe('Trusted Monarch Watch for this course: 3 claims verified, 1 still needs you.');
    expect(trustedAnnouncement('Monarch Watch', 1, 2)).toBe('Trusted Monarch Watch for this course: 1 claim verified, 2 still need you.');
  });
});

describe('adding a source', () => {
  it('titles the sheet by what the source is for', () => {
    expect(addSourceTitle('claim', 5)).toBe('Add a source for this claim');
    expect(addSourceTitle('group', 1)).toBe('Add a source for this claim');
    expect(addSourceTitle('group', 4)).toBe('Add a source for these 4 claims');
    expect(addSourceTitle('queue', 9)).toBe('Add a source for all 9 claims');
  });

  it('reports progress at each stage', () => {
    expect(progressLine({ stage: 'fetching', host: 'example.org' })).toBe('Reading example.org…');
    expect(progressLine({ stage: 'fetching', host: null })).toBe('Reading it…');
    expect(progressLine({ stage: 'saving' })).toBe('Saving it to your sources');
    expect(progressLine({ stage: 'done' })).toBe('Done');
    expect(progressLine({ stage: 'checking', done: 1, total: 4, statement: 'Short.' })).toBe('Checking claim 2 of 4: Short.');
    expect(progressLine({ stage: 'checking', done: 9, total: 4, statement: 'x'.repeat(100) })).toBe(`Checking claim 4 of 4: ${'x'.repeat(79)}…`);
  });

  it('explains each failure in one sentence, naming the host when known', () => {
    expect(failureLine('refused', 'example.org', 'HTTP 403 Forbidden')).toBe('example.org refused the request (HTTP 403).');
    expect(failureLine('refused', null, 'no status here')).toBe('The site refused the request.');
    expect(failureLine('paywall', 'example.org', '')).toBe('example.org shows a sign-in or subscription wall.');
    expect(failureLine('thin', null, '')).toBe('The page has almost no text to check.');
    expect(failureLine('browser_unavailable', null, '')).toBe('The page needs a browser to read and none is available.');
    expect(failureLine('blocked', null, '')).toBe('That address is private or local and cannot be fetched.');
    expect(failureLine('unreachable', 'example.org', '')).toBe('example.org could not be reached.');
  });

  describe('attachSummary', () => {
    it('says the source states the claim in focus', () => {
      expect(attachSummary(report(), 1, 3)).toEqual({ stated: true, notStated: false, line: 'Added Monarch Watch; it states this. 3 left.' });
    });

    it('says when the page does not say it, pointing at the closest passage', () => {
      const summary = attachSummary(report({ outcome: checkedResult(false) }), 1, 3);
      expect(summary).toMatchObject({ stated: false, notStated: true });
      expect(summary.line).toContain('it does not say this. The closest passage is shown');
    });

    it('says nothing mentions the claim when no passage was found', () => {
      const summary = attachSummary(report({ outcome: { outcome: 'no_passage' } }), 1, 3);
      expect(summary).toEqual({ stated: false, notStated: false, line: 'Nothing in Monarch Watch mentions this claim. It is saved in your sources.' });
      expect(attachSummary(report({ source: null, outcome: null }), 1, 0).line).toBe('Nothing in the source mentions this claim. It is saved in your sources.');
    });

    it('counts a group’s results together, with the claims that did not match or were not mentioned', () => {
      const line = attachSummary(report({ others: { tried: 14, verified: 11, notVerified: 2, noPassage: 1, stopped: null } }), 15, 4).line;
      expect(line).toBe('Added Monarch Watch. Verified 12 of 15 claims; 2 do not match what it says; 1 is not mentioned by it.');
    });

    it('uses singular verbs, counts the claim in focus, and notes a source already saved', () => {
      const line = attachSummary(report({ alreadySaved: true, outcome: checkedResult(false), others: { tried: 1, verified: 0, notVerified: 0, noPassage: 1, stopped: null } }), 2, 4).line;
      expect(line).toBe('Monarch Watch was already one of your sources; the claims were checked against it. Verified 0 of 2 claims; 1 does not match what it says; 1 is not mentioned by it.');
    });

    it('says where checking stopped, so the user can add and check again', () => {
      const line = attachSummary(report({ outcome: { outcome: 'no_passage' }, others: { tried: 5, verified: 2, notVerified: 0, noPassage: 1, stopped: 'the model went away' } }), 10, 4).line;
      expect(line).toContain('Checked 4 of 10; the rest were not checked: the model went away. Add and check again to continue.');
    });
  });

  describe('recheckLine', () => {
    it('is empty when nothing waited', () => {
      expect(recheckLine(null)).toBeNull();
      expect(recheckLine(undefined)).toBeNull();
      expect(recheckLine({ lessons: 0, tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null })).toBeNull();
    });

    it('counts the waiting claims checked and says where it stopped', () => {
      expect(recheckLine({ lessons: 1, tried: 1, verified: 1, notVerified: 0, noPassage: 0, stopped: null })).toBe('Checked 1 waiting claim against it: 1 verified.');
      expect(recheckLine({ lessons: 2, tried: 6, verified: 4, notVerified: 1, noPassage: 0, stopped: 'out of time' })).toBe('Checked 6 waiting claims against it: 4 verified. The rest were not checked: out of time.');
    });
  });
});
