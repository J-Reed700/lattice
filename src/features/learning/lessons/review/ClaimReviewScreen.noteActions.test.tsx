import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { review, teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimAttachReportDto, LessonClaimReviewDto, TeachingFindingDto } from '@/lib/bindings';

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

  it('adds a source for the note\'s first claim and settles the note source_added when the source states it', async () => {
    api.attach.mockResolvedValue(ok(report(true)));
    show(published(aboutSentence()));
    await openSheetFromTheNote();
    await addLink();
    await waitFor(() => expect(api.attach).toHaveBeenCalled());
    expect(api.attach.mock.calls[0][0]).toMatchObject({
      claimId: 'claim-9a', expectedRevision: 7, evidence: { kind: 'attach_link', url: URL_ }, scope: { scope: 'claim' },
    });
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith({
      programId: 'program-1', lessonId: 'lesson-1', settle: 'source_added', findingKeys: ['quotation:section-0'],
    }));
    expect(await screen.findByText('Source added. The note is resolved.')).toBeVisible();
  });

  it('leaves the note waiting when the source does not state the sentence', async () => {
    api.attach.mockResolvedValue(ok(report(false)));
    show(published(aboutSentence()));
    await openSheetFromTheNote();
    await addLink();
    await waitFor(() => expect(api.attach).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Add a source for this claim' })).not.toBeInTheDocument());
    expect(api.settle).not.toHaveBeenCalled();
    expect(screen.queryByText('Source added. The note is resolved.')).not.toBeInTheDocument();
  });

  it('leaves the note waiting when the sheet is closed without adding anything', async () => {
    show(published(aboutSentence()));
    await openSheetFromTheNote();
    fireEvent.keyDown(document.body, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Add a source for this claim' })).not.toBeInTheDocument());
    expect(api.attach).not.toHaveBeenCalled();
    expect(api.settle).not.toHaveBeenCalled();
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
