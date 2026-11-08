import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { review, teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimAttachReportDto, ClaimReviewItemDto, LessonClaimReviewDto, TeachingFindingDto } from '@/lib/bindings';

// Acting on a teaching note from the review screen: Add a source, and the line after the last statement.
const api = vi.hoisted(() => ({
  review: vi.fn(), resolve: vi.fn(), attach: vi.fn(), settle: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve, attachLearningClaimEvidence: api.attach,
  settleLearningTeachingFindings: api.settle, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => undefined) }) }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = (queued: number) => ({ verified: 12, confirmed: 0, queued, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 });
const URL_ = 'https://monarchwatch.example.org/tags';
const source = { sourceId: 'source-watch', sourceVersionId: 'version-watch', title: 'Monarch Watch', url: URL_, addedBy: 'user' as const };
const checked = (supported: boolean): ClaimAttachReportDto['outcome'] => ({ outcome: 'checked', check: {
  checker: { role: 'local', model: 'qwen' }, reason: '', fingerprint: 'f', evidence: [], windowTokens: 1,
  result: supported ? { verdict: 'supported', sourceVersionId: 'version-watch', supportingQuote: 'They fly up to 3,000 miles.' } : { verdict: 'failed', kind: { kind: 'unsupported' } },
} });
const report = (supported: boolean): ClaimAttachReportDto => ({
  revision: 8, source, alreadySaved: false, outcome: checked(supported), failure: null, snapshot: { state: 'not_applicable' },
  others: { tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null }, lines: [],
});

const aboutSentence = (overrides: Partial<TeachingFindingDto> = {}) => teachingFinding({
  findingKey: 'quotation:section-0', kind: 'quotation', passageRole: 'quotation', unitIndex: 0, unitTitle: 'Section 1: Migration',
  summary: 'No source is cited for this sentence: “Monarchs fly 3,000 miles.”', passage: null,
  sentence: 'Monarchs fly 3,000 miles.', claimIds: ['claim-9a', 'claim-9b'], ...overrides,
});

function show(data: LessonClaimReviewDto) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

const published = (finding: TeachingFindingDto) => review([], { state: 'ready', revision: 7, teachingFindings: [finding], counts: counts(0) });

async function openSheetFromTheNote() {
  await userEvent.click(await screen.findByRole('button', { name: 'Open the teaching notes' }));
  const notes = await screen.findByRole('region', { name: 'Teaching review' });
  await userEvent.click(within(notes).getByRole('button', { name: 'Add a source' }));
  expect(await screen.findByRole('heading', { name: 'Add a source for this claim' })).toBeVisible();
}

const confirmedClaim = (index: number): ClaimReviewItemDto => ({ ...waitingOnSource(index), state: { state: 'confirmed', at: 2 }, availableActions: [] });
const withClaims = (finding: TeachingFindingDto, claims: ClaimReviewItemDto[]) => review(claims, { state: 'ready', revision: 7, teachingFindings: [finding], counts: counts(claims.length) });

async function openNotesFromTheQueue() {
  await userEvent.click(await screen.findByRole('button', { name: /^Teaching notes/ }));
  expect(await screen.findByRole('region', { name: 'Teaching review' })).toBeVisible();
}

async function openSheet() {
  const notes = screen.getByRole('region', { name: 'Teaching review' });
  await userEvent.click(within(notes).getByRole('button', { name: 'Add a source' }));
  expect(await screen.findByRole('heading', { name: 'Add a source for this claim' })).toBeVisible();
}

/** The sheet is gone, the notes are still shown, and no statement took the user's place. */
async function staysOnTheNotes() {
  await waitFor(() => expect(screen.queryByRole('heading', { name: 'Add a source for this claim' })).not.toBeInTheDocument());
  expect(await screen.findByRole('region', { name: 'Teaching review' })).toBeVisible();
  expect(screen.getByRole('button', { name: /^Teaching notes/ })).toHaveAttribute('aria-current', 'true');
  expect(screen.queryByRole('button', { name: 'Edit the sentence' })).not.toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('article', { name: 'Section 1: Migration: Source quotation' })).toHaveFocus());
}

async function addLink() {
  await userEvent.type(screen.getByLabelText('Page address'), URL_);
  await userEvent.click(screen.getByRole('button', { name: 'Add and check' }));
}

describe('Claim review screen, acting on a teaching note', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
    api.settle.mockResolvedValue(ok(null));
  });

  it('adds a source for a note whose claim is not waiting, settles the note, and stays on the notes', async () => {
    api.attach.mockResolvedValue(ok(report(true)));
    show(withClaims(aboutSentence({ claimIds: ['claim-9'] }), [waitingOnSource(1), confirmedClaim(9)]));
    await openNotesFromTheQueue();
    await openSheet();
    await addLink();
    await waitFor(() => expect(api.attach).toHaveBeenCalled());
    expect(api.attach.mock.calls[0][0]).toMatchObject({
      claimId: 'claim-9', expectedRevision: 7, evidence: { kind: 'attach_link', url: URL_ }, scope: { scope: 'claim' },
    });
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith({
      programId: 'program-1', lessonId: 'lesson-1', settle: 'source_added', findingKeys: ['quotation:section-0'],
    }));
    expect(await screen.findByText('Source added. The note is resolved.')).toBeVisible();
    await staysOnTheNotes();
  });

  it('adds a source for a note whose claim is waiting, settles the note, and stays on the notes', async () => {
    api.attach.mockResolvedValue(ok(report(true)));
    show(withClaims(aboutSentence({ claimIds: ['claim-1'] }), [waitingOnSource(1)]));
    await openNotesFromTheQueue();
    await openSheet();
    await addLink();
    await waitFor(() => expect(api.attach.mock.calls[0]?.[0]).toMatchObject({ claimId: 'claim-1' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith({
      programId: 'program-1', lessonId: 'lesson-1', settle: 'source_added', findingKeys: ['quotation:section-0'],
    }));
    expect(await screen.findByText('Source added. The note is resolved.')).toBeVisible();
    await staysOnTheNotes();
  });

  it('says why the note stays when the source is added but the settle fails', async () => {
    api.attach.mockResolvedValue(ok(report(true)));
    api.settle.mockResolvedValue({ ok: false, error: 'Some of these notes changed since you opened them. Reload the lesson.' });
    show(withClaims(aboutSentence({ claimIds: ['claim-9'] }), [waitingOnSource(1), confirmedClaim(9)]));
    await openNotesFromTheQueue();
    await openSheet();
    await addLink();
    expect(await screen.findByText('Some of these notes changed since you opened them. Reload the lesson.')).toBeVisible();
    expect(screen.queryByText('Source added. The note is resolved.')).not.toBeInTheDocument();
    await staysOnTheNotes();
  });

  it('leaves the note waiting and says what the source did when it does not state the sentence', async () => {
    api.attach.mockResolvedValue(ok(report(false)));
    show(withClaims(aboutSentence({ claimIds: ['claim-9'] }), [waitingOnSource(1), confirmedClaim(9)]));
    await openNotesFromTheQueue();
    await openSheet();
    await addLink();
    expect(await screen.findByText(/Added Monarch Watch; it does not say this\./)).toBeVisible();
    expect(api.settle).not.toHaveBeenCalled();
    expect(screen.queryByText('Source added. The note is resolved.')).not.toBeInTheDocument();
    await staysOnTheNotes();
  });

  it('closes the sheet opened from a note with Escape from the focused address field, back to the notes', async () => {
    show(published(aboutSentence()));
    await openSheetFromTheNote();
    expect(screen.getByLabelText('Page address')).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Add a source for this claim' })).not.toBeInTheDocument());
    expect(await screen.findByRole('region', { name: 'Teaching review' })).toBeVisible();
    expect(api.attach).not.toHaveBeenCalled();
    expect(api.settle).not.toHaveBeenCalled();
  });

  it('closes a statement\'s sheet with Escape from the focused address field, and stays on the statement', async () => {
    show(review([waitingOnSource(1)], { counts: counts(1) }));
    await screen.findByRole('button', { name: 'Edit the sentence' });
    await userEvent.keyboard('s');
    expect(await screen.findByRole('heading', { name: 'Add a source for this claim' })).toBeVisible();
    expect(screen.getByLabelText('Page address')).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Add a source for this claim' })).not.toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Edit the sentence' })).toBeVisible();
  });

  it('leaves the edit form open with its text when Escape is typed in its textarea', async () => {
    show(review([waitingOnSource(1)], { counts: counts(1) }));
    await screen.findByRole('button', { name: 'Edit the sentence' });
    await userEvent.click(await screen.findByRole('button', { name: 'Edit the sentence' }));
    const sentence = await screen.findByLabelText('The sentence');
    await userEvent.click(sentence);
    await userEvent.type(sentence, ' Edited.');
    await userEvent.keyboard('{Escape}');
    expect(screen.getByLabelText('The sentence')).toBeVisible();
    expect((screen.getByLabelText('The sentence') as HTMLTextAreaElement).value).toContain('Edited.');
  });

  it('says everything is resolved, and never to resume, after the last statement on a published lesson', async () => {
    const claim = waitingOnSource(1);
    const waiting = review([claim], { state: 'ready', counts: counts(1) });
    const done = review([{ ...claim, state: { state: 'confirmed', at: 2 } }], { state: 'ready', revision: 8, counts: counts(0) });
    api.review.mockResolvedValueOnce(ok(waiting));
    api.review.mockResolvedValue(ok(done));
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    render(<QueryClientProvider client={client}>
      <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
    </QueryClientProvider>);
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    expect(await screen.findByText('Accepted. Everything is resolved.')).toBeVisible();
    expect(screen.queryByText(/Resume verification to publish/)).not.toBeInTheDocument();
  });

  it('keeps saying to resume verification after the last statement of a draft lesson', async () => {
    const claim = waitingOnSource(1);
    const waiting = review([claim], { state: 'needs_review', counts: counts(1) });
    const done = review([{ ...claim, state: { state: 'confirmed', at: 2 } }], { state: 'needs_review', revision: 8, counts: counts(0) });
    api.review.mockResolvedValueOnce(ok(waiting));
    api.review.mockResolvedValue(ok(done));
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    render(<QueryClientProvider client={client}>
      <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
    </QueryClientProvider>);
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    expect(await screen.findByText('Accepted. Everything is resolved. Resume verification to publish the lesson.')).toBeVisible();
  });
});
