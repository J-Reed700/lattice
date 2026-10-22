import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { whyItWaits } from '@/features/learning/lessons/review/claimReviewCopy';
import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { support, sufficiency, thirtyThreeClaims, tried, waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { review, teachingFinding, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, LessonClaimReviewDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({
  review: vi.fn(), resolve: vi.fn(), resume: vi.fn(), audit: vi.fn(), keep: vi.fn(), trust: vi.fn(), setTrust: vi.fn(), clearTrust: vi.fn(), version: vi.fn(), plan: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve, resumeLearningLessonReview: api.resume,
  keepLearningTeachingFinding: api.keep, getLearningLessonAudit: api.audit, getLearningSourceTrust: api.trust,
  setSourceTrustOverride: api.setTrust, clearSourceTrustOverride: api.clearTrust, getLearningSourceVersion: api.version,
  getLearningPlan: api.plan, courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
// Internal names (§4 house style); "kind of" is plain English the copy deck uses (§4.1), "kind" alone is a field name.
const BANNED = [/[0-9a-f]{8}-[0-9a-f]{4}-/i, /bytes/i, /hybrid/i, /\bPass\b/, /established/i, /community/i, /\bprimary\b/i, /authoritative/i, /machine/i, /\bkind\b(?! of\b)/i];

/** Each review read answers the next fixture; the last one repeats. */
function show(data: LessonClaimReviewDto | LessonClaimReviewDto[], onBack = vi.fn()) {
  const sequence = Array.isArray(data) ? data : [data];
  sequence.forEach((value) => api.review.mockResolvedValueOnce(ok(value)));
  api.review.mockResolvedValue(ok(sequence[sequence.length - 1]));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={onBack} />
  </QueryClientProvider>);
  return onBack;
}

const statement = () => document.querySelector<HTMLElement>('[data-claim-statement]');
const confirmedFrom = (claims: ClaimReviewItemDto[], id: string) => claims.map((claim) => (claim.claimId === id ? { ...claim, state: { state: 'confirmed' as const, at: 2 } } : claim));
const counts = (queued: number) => ({ verified: 12, confirmed: 0, generalKnowledge: 0, queued, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 74 });

describe('Claim review screen', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
  });
  afterEach(() => vi.useRealTimers());

  it('opens on the first waiting claim grouped by source with counts, and moves to the next after accepting', async () => {
    const claims = thirtyThreeClaims();
    show([review(claims, { counts: counts(33) }), review(confirmedFrom(claims, 'claim-1'), { revision: 8, counts: counts(32) })]);
    const queue = await screen.findByRole('navigation', { name: 'Claims waiting for you' });
    const first = within(queue).getByRole('region', { name: 'Field Guide to North American Butterflies · 15 claims' });
    expect(within(first).getByRole('button', { name: 'Accept this source for all 15' })).toBeVisible();
    expect(screen.getByText('33 claims need you · 12 verified · 74 rewritten by the machine')).toBeVisible();
    expect(statement()).toHaveTextContent('Monarch fact number 1 is stated in the guide.');
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({
      claimId: 'claim-1', expectedRevision: 7, action: { action: 'confirm', note: null, sourceVersionIds: ['version-source-guide'] },
    })));
    expect(await screen.findByText('Accepted. 32 left.')).toBeVisible();
    await waitFor(() => expect(statement()).toHaveTextContent('Monarch fact number 2 is stated in the guide.'));
    await waitFor(() => expect(statement()).toHaveFocus());
  });

  it('shows the checked statement in plain text inside its sentence, never markdown or citation markers', async () => {
    const item = waitingOnSource(1);
    item.location = { ...item.location, quote: 'The **eastern** `population` migrates *up to* 3,000 miles [5].' };
    item.presentation = { statement: 'The eastern population migrates up to 3,000 miles.', sentence: 'The eastern population migrates up to 3,000 miles.', contextBefore: 'Monarchs breed in the north.', contextAfter: 'Western monarchs winter in California.', place: 'Section 2 · Migration' };
    show(review([item]));
    await waitFor(() => expect(statement()).not.toBeNull());
    const shown = statement()!;
    expect(shown).toHaveTextContent('Monarchs breed in the north. The eastern population migrates up to 3,000 miles. Western monarchs winter in California.');
    expect(shown.textContent).not.toMatch(/\*|`|\[5\]/);
    expect(screen.getByText('Section 2 · Migration')).toBeVisible();
  });

  it('explains why a claim waits in one plain line with no internal vocabulary', async () => {
    const missing = ['no_identified_source', 'no_primary_for_subject', 'stronger_source_silent', 'no_independent_corroboration', 'only_testimonial', 'only_stale', 'no_measurement_origin'];
    const items: ClaimReviewItemDto[] = [
      ...missing.map((lack, index) => ({ ...waitingOnSource(index), sufficiency: sufficiency(lack) })),
      { ...unsupportedItem(20), support: null, tried: tried(), presentation: waitingOnSource(20).presentation, alsoSupported: [] },
      { ...unsupportedItem(21), verdictKind: { kind: 'no_source' }, support: null, tried: tried(), presentation: waitingOnSource(21).presentation, alsoSupported: [] },
      { ...waitingOnSource(22), verdictKind: { kind: 'contradicted' }, support: support({ role: 'contradicts' }) },
      { ...waitingOnSource(23), verdictKind: { kind: 'general_knowledge_proposed' }, support: null },
      { ...waitingOnSource(24), verdictKind: { kind: 'could_not_judge' } },
      { ...waitingOnSource(25), verdictKind: { kind: 'disputed', verifier: { kind: 'unsupported' } } },
    ];
    for (const item of items) {
      const why = whyItWaits(item);
      for (const banned of BANNED) {
        expect(why.reason, `${item.verdictKind?.kind} ${item.sufficiency?.missing}`).not.toMatch(banned);
        expect(why.detail ?? '').not.toMatch(banned);
      }
    }
    show(review([items[0]]));
    expect(await screen.findByText('No source the course trusts states this.')).toBeVisible();
    expect(screen.getByText('The one passage that does is from an online book whose author is not identified.')).toBeVisible();
    expect(screen.getByText('Waiting for you')).toBeVisible();
  });

  it('highlights the supporting sentence with context, names the source’s kind and site, and keeps the full passage behind a disclosure', async () => {
    const fenced = 'Tagged monarchs report their route.\n\n```json\n{"route": "south", "miles": 3000}\n```\n\nEach autumn the eastern population migrates up to 3,000 miles to wintering sites in central Mexico.';
    const evidence = support({ passage: { ...support().passage, text: fenced }, quoteSpan: null });
    api.version.mockResolvedValue(ok({ sourceId: 'source-guide', version: { id: 'version-guide', versionNumber: 1, title: 'Field Guide', publisher: null, resolvedUrl: null, excerpt: '', contentSha256: 's', wordCount: 20, truncated: false, extractionVersion: 'text-v1', acquiredAt: 1 }, fullText: fenced, usage: [] }));
    show(review([{ ...waitingOnSource(1), support: evidence }]));
    const best = await screen.findByRole('region', { name: 'best evidence' });
    expect(within(best).getByText(evidence.quote, { selector: 'mark' })).toBeVisible();
    expect(within(best).getByText('Field Guide to North American Butterflies · butterflies.example.org · online book')).toBeVisible();
    expect(within(best).getByText(/an unidentified author’s work · not enough on its own for this claim/)).toBeVisible();
    expect(screen.queryByText(/"route": "south"/)).not.toBeInTheDocument();
    await userEvent.click(within(best).getByRole('button', { name: 'Full passage' }));
    // The code block is highlighted token by token, so read the passage's text rather than one element.
    await waitFor(() => expect(best.textContent).toMatch(/"route":\s*"south"/));
    expect(best.textContent).not.toMatch(/jsonjson/);
  });

  it('sums what was tried into one line and lists only relevant pages in its details', async () => {
    const relevant = [{ sourceId: 's-2', sourceVersionId: 'v-2', title: 'Monarch Watch tagging results', url: 'https://monarchwatch.example.org/tags', addedBy: 'research' as const }];
    show(review([{ ...waitingOnSource(1), tried: tried({ relevant, pagesMentioning: 0 }), research: [{ queries: ['monarch migration distance'], sourcesAdded: [{ sourceId: 'x', sourceVersionId: 'y', title: 'LinkedIn Login', url: null, addedBy: 'research' }], passagesFound: 0, at: 1 }] }]));
    expect(await screen.findByText('Searched the web 9 times and read 214 pages; none stated it.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Details' }));
    const list = screen.getByRole('list', { name: 'Pages that mention it' });
    expect(within(list).getAllByRole('listitem').map((row) => row.textContent)).toEqual(['Monarch Watch tagging results · monarchwatch.example.orgOpen']);
    expect(screen.queryByText(/LinkedIn Login/)).not.toBeInTheDocument();
  });

  it('accepts a source for every claim it supports with a count and undoes it', async () => {
    const claims = thirtyThreeClaims();
    show(review(claims, { counts: counts(33) }));
    api.trust.mockResolvedValue(ok({ sources: [{ sourceId: 'source-guide', impact: { source: { versions: 1, claims: 15, lessons: 1 }, site: null } }], claim: null }));
    api.setTrust.mockResolvedValue(ok({ change: { scope: { scope: 'source', programId: 'program-1', sourceId: 'source-guide' }, sourceVersionIds: ['version-source-guide'] }, impact: { versions: 1, claims: 15, lessons: 1 }, redecided: { verified: 12, reopened: 0, stillFailing: 3, notLevelled: 0 } }));
    api.clearTrust.mockResolvedValue(ok(null));
    const queue = await screen.findByRole('navigation', { name: 'Claims waiting for you' });
    await userEvent.click(within(queue).getByRole('button', { name: 'Accept this source for all 15' }));
    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByText(/15 claims rest on this source/)).toBeVisible();
    expect(within(dialog).getByRole('radio', { name: /a reliable identified source/ })).toBeChecked();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Accept for 15 claims' }));
    await waitFor(() => expect(api.setTrust).toHaveBeenCalledWith({ scope: 'source', programId: 'program-1', sourceId: 'source-guide', level: 'established', note: null }));
    expect(await screen.findByText('Trusted Field Guide to North American Butterflies for this course: 12 claims verified, 3 still need you.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.clearTrust).toHaveBeenCalledWith({ scope: 'source', programId: 'program-1', sourceId: 'source-guide' }));
  });

  it('works from the keyboard: J/K, A, E, R, S, U, Escape', async () => {
    const claims = thirtyThreeClaims();
    const onBack = show([review(claims), review(confirmedFrom(claims, 'claim-1'), { revision: 8 })]);
    await waitFor(() => expect(statement()).toHaveTextContent('Monarch fact number 1'));
    fireEvent.keyDown(document.body, { key: 'j' });
    await waitFor(() => expect(statement()).toHaveTextContent('Monarch fact number 2'));
    fireEvent.keyDown(document.body, { key: 'k' });
    await waitFor(() => expect(statement()).toHaveTextContent('Monarch fact number 1'));

    fireEvent.keyDown(document.body, { key: 'e' });
    const sentence = await screen.findByLabelText('The sentence');
    fireEvent.keyDown(sentence, { key: 'r' });
    expect(api.resolve).not.toHaveBeenCalled();
    fireEvent.keyDown(document.body, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByLabelText('The sentence')).not.toBeInTheDocument());

    fireEvent.keyDown(document.body, { key: 's' });
    expect(await screen.findByRole('heading', { name: 'Add a source for this claim' })).toBeVisible();
    fireEvent.keyDown(document.body, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Add a source for this claim' })).not.toBeInTheDocument());

    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    fireEvent.keyDown(document.body, { key: 'a' });
    await waitFor(() => expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({ claimId: 'claim-1', action: expect.objectContaining({ action: 'confirm' }) })));
    expect(await screen.findByText('Accepted. 32 left.')).toBeVisible();
    fireEvent.keyDown(document.body, { key: 'u' });
    await waitFor(() => expect(api.resolve).toHaveBeenLastCalledWith(expect.objectContaining({ claimId: 'claim-1', action: { action: 'undo' } })));

    fireEvent.keyDown(document.body, { key: 'r' });
    await waitFor(() => expect(api.resolve).toHaveBeenLastCalledWith(expect.objectContaining({ action: { action: 'remove' } })));
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(onBack).toHaveBeenCalled();
  });

  it('requires a note to accept a contradicted claim and sends it with the review revision', async () => {
    const item = { ...waitingOnSource(1), verdictKind: { kind: 'contradicted' as const }, support: support({ role: 'contradicts' }) };
    show(review([item]));
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    expect(await screen.findByRole('region', { name: 'says otherwise' })).toBeVisible();
    expect(screen.getByText('A source says otherwise.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Accept with a note' }));
    const submit = screen.getByRole('button', { name: 'Accept with this note' });
    expect(submit).toBeDisabled();
    await userEvent.type(screen.getByLabelText('Why the claim is right (required)'), 'The guide rounds the figure.');
    await userEvent.click(submit);
    await waitFor(() => expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({
      expectedRevision: 7, action: { action: 'confirm', note: 'The guide rounds the figure.', sourceVersionIds: [] },
    })));
  });

  it('shows a stale-revision refusal on the claim and reloads the review', async () => {
    api.resolve.mockResolvedValue({ ok: false, error: 'This claim changed; reload and act again.' });
    show(review([waitingOnSource(1)]));
    await userEvent.click(await screen.findByRole('button', { name: 'Remove it' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('This claim changed; reload and act again.');
    await waitFor(() => expect(api.review).toHaveBeenCalledTimes(2));
  });

  it('accepts every common-knowledge proposal of the no-source group in one action', async () => {
    const proposals = [1, 2].map((index) => ({ ...waitingOnSource(index), support: null, verdictKind: { kind: 'general_knowledge_proposed' as const } }));
    show(review(proposals));
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    await userEvent.click(await screen.findByRole('button', { name: 'Accept all 2 common-knowledge proposals' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({ claimId: 'claim-1', action: { action: 'accept_all_general_knowledge' } })));
  });

  it('keeps the first and the independent check under More › History', async () => {
    const passage = { passage: support().passage, addedBy: 'research' as const };
    const item = { ...waitingOnSource(1), verdictKind: { kind: 'disputed' as const, verifier: { kind: 'contradicted' as const } },
      localCheck: { checker: { role: 'local' as const, model: 'qwen' }, result: { verdict: 'supported' as const, sourceVersionId: 'v', supportingQuote: 'up to 3,000 miles' }, reason: 'Stated.', quotedSource: null, passages: [passage], at: 1 },
      verifierCheck: { checker: { role: 'verifier' as const, model: 'claude' }, result: { verdict: 'failed' as const, kind: { kind: 'contradicted' as const } }, reason: 'The guide gives 2,500 miles.', quotedSource: null, passages: [passage], at: 2 } };
    show(review([item]));
    expect(await screen.findByText('The guide gives 2,500 miles.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'More' }));
    await userEvent.click(screen.getByRole('menuitem', { name: /History/ }));
    const checks = screen.getByRole('group', { name: 'First check and independent check' });
    expect(within(checks).getByText('Supported')).toBeVisible();
    expect(within(checks).getByText('A source says otherwise')).toBeVisible();
  });

  it('lists a published lesson’s teaching notes as the queue’s last group and keeps one as written', async () => {
    api.keep.mockResolvedValue(ok(null));
    show(review([], { state: 'ready', teachingFindings: [teachingFinding()], counts: { ...counts(0), rewritten: 0, verified: 40 } }));
    await userEvent.click(await screen.findByRole('button', { name: /Teaching notes/ }));
    expect(screen.getByText(/the example shows "result": "4"/)).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Keep as written' }));
    await waitFor(() => expect(api.keep).toHaveBeenCalledWith({ programId: 'program-1', lessonId: 'lesson-1', findingKey: 'teaching:section-2' }));
  });

  it('renders the empty state: everything resolved, Resume enabled, the resolved claims one toggle away', async () => {
    const resolvedClaim = { ...waitingOnSource(1), state: { state: 'resolved_by_evidence' as const } };
    show(review([resolvedClaim], { changedSinceStop: true, counts: { ...counts(0), rewritten: 0 } }));
    expect(await screen.findByRole('heading', { name: 'Everything is resolved' })).toBeVisible();
    expect(screen.getByText(/Resume verification to publish the lesson/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Resume verification' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Show resolved (1)' })).toBeVisible();
  });

  it.each([
    ['running', () => {
      api.plan.mockResolvedValue(ok({ jobs: [{ id: 'job', kind: 'lesson_preparation', status: 'running', retryOfJobId: null, resultId: null, activity: { lessonTitle: 'Monarch migration' } }] }));
      return review([waitingOnSource(1)], { state: 'verifying' });
    }, /Checking your changes/],
    ['audit-held', () => review([], { changedSinceStop: false }), /Waiting for the independent check/],
    ['stopped', () => review([], { state: 'stopped', stopReason: { code: 'model_rejected', message: 'The model server rejected the request twice.', fix: 'lower the context window in Settings', at: 1, request: null } }), /Lesson preparation stopped/],
  ] as const)('renders the %s state', async (_, fixture, words) => {
    show(fixture());
    expect(await screen.findByText(words)).toBeVisible();
  });

  it('renders the error state with Load again', async () => {
    api.review.mockResolvedValueOnce({ ok: false, error: 'The review could not be read.' }).mockResolvedValue(ok(review([waitingOnSource(1)])));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(<QueryClientProvider client={client}><ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} /></QueryClientProvider>);
    expect(await screen.findByRole('alert')).toHaveTextContent('The review could not be read.');
    await userEvent.click(screen.getByRole('button', { name: 'Load again' }));
    await waitFor(() => expect(statement()).toHaveTextContent('Monarch fact number 1'));
  });

  it('stacks the queue and the claim at narrow widths with a sticky action bar', async () => {
    const original = window.matchMedia;
    window.matchMedia = ((query: string) => ({ matches: query.includes('max-width: 767px'), media: query, addEventListener: vi.fn(), removeEventListener: vi.fn(), addListener: vi.fn(), removeListener: vi.fn(), dispatchEvent: vi.fn(), onchange: null })) as typeof window.matchMedia;
    try {
      show(review(thirtyThreeClaims()));
      const queue = await screen.findByRole('navigation', { name: 'Claims waiting for you' });
      expect(screen.queryByRole('article', { name: 'Claim in focus' })).not.toBeInTheDocument();
      await userEvent.click(within(queue).getByRole('button', { name: /Monarch fact number 3 / }));
      const claim = await screen.findByRole('article', { name: 'Claim in focus' });
      expect(within(claim).getByText('3 of 33')).toBeVisible();
      expect(within(claim).getByRole('toolbar', { name: 'Claim actions' }).closest('.sticky')).not.toBeNull();
      await userEvent.click(within(claim).getByRole('button', { name: 'Back to the queue' }));
      expect(await screen.findByRole('navigation', { name: 'Claims waiting for you' })).toBeVisible();
    } finally {
      window.matchMedia = original;
    }
  });
});
